//! Admission control, one gate per provider. Every request asks its gate
//! before it goes out; a refusal turns into a stale answer or a gap.
use super::{Fail, Source};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Watch polls run in their own lane so they can never starve a chat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lane {
    #[default]
    Chat,
    Watch,
}

/// GMGN's free plan allows 5 weight a second per IP and refuses bursts that
/// look within it on paper (1 then 5 weight 0.3 s apart; 3 then 2 weight
/// 0.2 s apart); repeats earn an IP ban that blocks every user on the host.
/// We allow 4 weight per 1.4 s rolling window; a heavier call gets a
/// window to itself.
const GMGN_CAPACITY: u32 = 4;
const GMGN_WINDOW: Duration = Duration::from_millis(1400);
/// A chat call waits at most this long in the queue; longer means a gap.
const GMGN_MAX_WAIT: Duration = Duration::from_secs(4);
/// Weight a watch poll must leave free in the window for chats.
const GMGN_WATCH_HEADROOM: u32 = 2;

/// Fomo's free plan allows 20 requests a minute per key; we keep 18 so a
/// burst from two users never earns a 429. Every call also spends credits
/// (250k a month), so a refusal is cheaper than a retry.
const FOMO_CAPACITY: usize = 18;
const FOMO_WINDOW: Duration = Duration::from_secs(60);
/// A chat call waits at most this long for a Fomo slot.
const FOMO_MAX_WAIT: Duration = Duration::from_secs(8);
/// Slots a watch poll must leave free in the window for chats.
const FOMO_WATCH_HEADROOM: usize = 6;
/// Paced sources (keyless Jupiter, RugCheck, partners) wait at most this
/// long for their turn.
const PACED_MAX_WAIT: Duration = Duration::from_secs(6);

#[derive(Default)]
pub struct Gates {
    gmgn: Mutex<Window>,
    fomo: Mutex<Fomo>,
    /// When each paced source may send next.
    paced: Mutex<HashMap<Source, Instant>>,
}

/// GMGN requests sent or scheduled, by send time, oldest first.
#[derive(Default)]
struct Window {
    sent: VecDeque<(Instant, u32)>,
    paused_until: Option<Instant>,
}

/// Fomo requests sent or scheduled, oldest first.
#[derive(Default)]
struct Fomo {
    sent: VecDeque<Instant>,
    paused_until: Option<Instant>,
}

/// What a request costs at its gate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cost {
    /// GMGN rate-limit weight.
    Gmgn(u32),
    /// One Fomo request (credits and a per-minute cap).
    Fomo,
    /// At most one request per `gap` to this source, FIFO.
    Paced(Source, Duration),
    Free,
}

impl Gates {
    /// Admits one request, returning how long to wait before sending it.
    pub fn admit(&self, cost: Cost, lane: Lane) -> Result<Duration, Fail> {
        let now = Instant::now();
        match cost {
            Cost::Gmgn(weight) => self.admit_gmgn(weight, lane, now),
            Cost::Fomo => self.admit_fomo(lane, now),
            Cost::Paced(source, gap) => self.admit_paced(source, gap, now),
            Cost::Free => Ok(Duration::ZERO),
        }
    }

    /// Whether a request that already waited in the queue may still go.
    pub fn still_open(&self, cost: Cost) -> Result<(), Fail> {
        let (source, paused) = match cost {
            Cost::Gmgn(_) => (
                Source::Gmgn,
                self.gmgn.lock().ok().and_then(|w| w.paused_until),
            ),
            Cost::Fomo => (
                Source::Fomo,
                self.fomo.lock().ok().and_then(|w| w.paused_until),
            ),
            _ => return Ok(()),
        };
        match paused {
            Some(until) if Instant::now() < until => Err(Fail::RateLimited {
                source,
                retry_after_s: (until - Instant::now()).as_secs().max(1),
            }),
            _ => Ok(()),
        }
    }

    /// Stops a source's traffic until `retry_after_s` from now (a 429 or a
    /// ban). Only GMGN and Fomo keep a pause; others answer per request.
    pub fn pause(&self, source: Source, retry_after_s: u64) {
        let until = Instant::now() + Duration::from_secs(retry_after_s.max(2));
        let extend = |slot: &mut Option<Instant>| {
            *slot = Some(slot.map_or(until, |p| p.max(until)));
        };
        match source {
            Source::Gmgn => {
                if let Ok(mut w) = self.gmgn.lock() {
                    extend(&mut w.paused_until);
                }
            }
            Source::Fomo => {
                if let Ok(mut w) = self.fomo.lock() {
                    extend(&mut w.paused_until);
                }
            }
            _ => {}
        }
    }

    /// Schedules a GMGN request at the earliest time its weight fits in
    /// the rolling window, after every request already scheduled (FIFO).
    fn admit_gmgn(&self, weight: u32, lane: Lane, now: Instant) -> Result<Duration, Fail> {
        let mut window = self
            .gmgn
            .lock()
            .map_err(|_| Fail::Budget("gmgn gate poisoned"))?;
        if let Some(until) = window.paused_until {
            if now < until {
                return Err(Fail::RateLimited {
                    source: Source::Gmgn,
                    retry_after_s: (until - now).as_secs().max(1),
                });
            }
            window.paused_until = None;
        }
        while window
            .sent
            .front()
            .is_some_and(|(at, _)| *at + GMGN_WINDOW <= now)
        {
            window.sent.pop_front();
        }
        let weight = weight.min(GMGN_CAPACITY);
        let mut at = window.sent.back().map_or(now, |(last, _)| (*last).max(now));
        // Move `at` past the oldest requests until the window from `at`
        // back holds room for this one.
        loop {
            let used: u32 = window
                .sent
                .iter()
                .filter(|(t, _)| *t + GMGN_WINDOW > at)
                .map(|(_, w)| w)
                .sum();
            if used + weight <= GMGN_CAPACITY {
                break;
            }
            let Some(next) = window
                .sent
                .iter()
                .map(|(t, _)| *t + GMGN_WINDOW)
                .find(|expires| *expires > at)
            else {
                break;
            };
            at = next;
        }
        let wait = at.saturating_duration_since(now);
        let refused = match lane {
            Lane::Chat => wait > GMGN_MAX_WAIT,
            Lane::Watch => {
                let used: u32 = window.sent.iter().map(|(_, w)| w).sum();
                !wait.is_zero() || used + weight > GMGN_CAPACITY - GMGN_WATCH_HEADROOM
            }
        };
        if refused {
            return Err(Fail::Budget("gmgn request queue is full right now"));
        }
        window.sent.push_back((at, weight));
        Ok(wait)
    }

    /// Schedules a Fomo request at the earliest time it fits in the
    /// one-minute window, after every request already scheduled (FIFO).
    fn admit_fomo(&self, lane: Lane, now: Instant) -> Result<Duration, Fail> {
        let mut fomo = self
            .fomo
            .lock()
            .map_err(|_| Fail::Budget("fomo gate poisoned"))?;
        if let Some(until) = fomo.paused_until {
            if now < until {
                return Err(Fail::RateLimited {
                    source: Source::Fomo,
                    retry_after_s: (until - now).as_secs().max(1),
                });
            }
            fomo.paused_until = None;
        }
        while fomo.sent.front().is_some_and(|at| *at + FOMO_WINDOW <= now) {
            fomo.sent.pop_front();
        }
        let mut at = fomo.sent.back().map_or(now, |last| (*last).max(now));
        let inside = |at: Instant| fomo.sent.iter().filter(|t| **t + FOMO_WINDOW > at).count();
        while inside(at) >= FOMO_CAPACITY {
            let Some(next) = fomo
                .sent
                .iter()
                .map(|t| *t + FOMO_WINDOW)
                .find(|expires| *expires > at)
            else {
                break;
            };
            at = next;
        }
        let wait = at.saturating_duration_since(now);
        let refused = match lane {
            Lane::Chat => wait > FOMO_MAX_WAIT,
            Lane::Watch => {
                !wait.is_zero() || fomo.sent.len() + FOMO_WATCH_HEADROOM >= FOMO_CAPACITY
            }
        };
        if refused {
            return Err(Fail::Budget("fomo request queue is full right now"));
        }
        fomo.sent.push_back(at);
        Ok(wait)
    }

    /// One request per `gap` to `source`, in arrival order.
    fn admit_paced(&self, source: Source, gap: Duration, now: Instant) -> Result<Duration, Fail> {
        let mut paced = self
            .paced
            .lock()
            .map_err(|_| Fail::Budget("pacing gate poisoned"))?;
        let at = paced.get(&source).map_or(now, |next| (*next).max(now));
        let wait = at.saturating_duration_since(now);
        if wait > PACED_MAX_WAIT {
            return Err(Fail::RateLimited {
                source,
                retry_after_s: wait.as_secs().max(1),
            });
        }
        paced.insert(source, at + gap);
        Ok(wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmgn_never_puts_more_than_4_weight_in_a_window() {
        let gates = Gates::default();
        let t0 = Instant::now();
        // A card (1 + 1) then a holder list (5): the list waits for the
        // window to clear and then takes it alone.
        assert_eq!(gates.admit_gmgn(1, Lane::Chat, t0).unwrap(), Duration::ZERO);
        assert_eq!(gates.admit_gmgn(1, Lane::Chat, t0).unwrap(), Duration::ZERO);
        assert_eq!(gates.admit_gmgn(5, Lane::Chat, t0).unwrap(), GMGN_WINDOW);
        // FIFO: a small call queues behind it rather than jumping ahead.
        assert_eq!(
            gates.admit_gmgn(1, Lane::Chat, t0).unwrap(),
            GMGN_WINDOW * 2
        );
        // Past four seconds of queue the call is refused, not queued.
        assert!(gates.admit_gmgn(5, Lane::Chat, t0).is_err());
        // A board (3) and a trenches call (3) never share a window.
        let gates = Gates::default();
        assert_eq!(gates.admit_gmgn(3, Lane::Chat, t0).unwrap(), Duration::ZERO);
        assert_eq!(gates.admit_gmgn(3, Lane::Chat, t0).unwrap(), GMGN_WINDOW);
        let sent = gates.gmgn.lock().unwrap().sent.clone();
        for (start, _) in &sent {
            let inside: u32 = sent
                .iter()
                .filter(|(t, _)| *t >= *start && *t < *start + GMGN_WINDOW)
                .map(|(_, w)| w)
                .sum();
            assert!(inside <= GMGN_CAPACITY, "{inside} in one window");
        }
        let later = t0 + Duration::from_secs(10);
        assert_eq!(
            gates.admit_gmgn(5, Lane::Chat, later).unwrap(),
            Duration::ZERO
        );
    }

    #[test]
    fn watch_polls_leave_room_for_chat() {
        let gates = Gates::default();
        let t0 = Instant::now();
        assert!(gates.admit_gmgn(1, Lane::Watch, t0).is_ok());
        assert!(gates.admit_gmgn(1, Lane::Chat, t0).is_ok());
        // 2 of 4 used: another watch poll would leave less than 2 for chat.
        assert!(gates.admit_gmgn(1, Lane::Watch, t0).is_err());
        assert!(gates.admit_gmgn(2, Lane::Chat, t0).is_ok());
    }

    #[test]
    fn a_pause_refuses_until_reset() {
        let gates = Gates::default();
        gates.pause(Source::Gmgn, 30);
        let err = gates.admit_gmgn(1, Lane::Chat, Instant::now()).unwrap_err();
        assert!(matches!(err, Fail::RateLimited { retry_after_s, .. } if retry_after_s >= 29));
        let later = Instant::now() + Duration::from_secs(31);
        assert!(gates.admit_gmgn(1, Lane::Chat, later).is_ok());
    }

    #[test]
    fn fomo_keeps_18_a_minute_and_watch_leaves_room() {
        let gates = Gates::default();
        let t0 = Instant::now();
        for _ in 0..FOMO_CAPACITY {
            assert_eq!(gates.admit_fomo(Lane::Chat, t0).unwrap(), Duration::ZERO);
        }
        // The 19th waits a minute, which is past a chat's patience.
        assert!(gates.admit_fomo(Lane::Chat, t0).is_err());
        let later = t0 + FOMO_WINDOW + Duration::from_secs(1);
        assert!(gates.admit_fomo(Lane::Chat, later).is_ok());
        let gates = Gates::default();
        for _ in 0..FOMO_CAPACITY - FOMO_WATCH_HEADROOM {
            gates.admit_fomo(Lane::Chat, t0).unwrap();
        }
        assert!(gates.admit_fomo(Lane::Watch, t0).is_err());
        gates.pause(Source::Fomo, 30);
        assert!(matches!(
            gates.admit_fomo(Lane::Chat, t0),
            Err(Fail::RateLimited { .. })
        ));
    }

    #[test]
    fn paced_sources_queue_fifo() {
        let gates = Gates::default();
        let t0 = Instant::now();
        let gap = Duration::from_secs(2);
        assert_eq!(
            gates.admit_paced(Source::Jupiter, gap, t0).unwrap(),
            Duration::ZERO
        );
        assert_eq!(gates.admit_paced(Source::Jupiter, gap, t0).unwrap(), gap);
        assert_eq!(
            gates.admit_paced(Source::Rugcheck, gap, t0).unwrap(),
            Duration::ZERO
        );
        for _ in 0..2 {
            gates.admit_paced(Source::Jupiter, gap, t0).unwrap();
        }
        assert!(gates.admit_paced(Source::Jupiter, gap, t0).is_err());
    }
}
