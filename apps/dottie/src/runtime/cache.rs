//! Process-wide TTL cache shared by every user on a host. Entries remember
//! when they were stored, so a stale answer can say how old it is.
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CAPACITY: usize = 4096;

struct Entry {
    stored_at: Instant,
    ttl: Duration,
    value: Value,
}

impl Entry {
    fn expires(&self) -> Instant {
        self.stored_at + self.ttl
    }
}

#[derive(Debug, PartialEq)]
pub enum Hit {
    Fresh(Value),
    /// Past its TTL; `age` is the time since it was stored.
    Stale {
        value: Value,
        age: Duration,
    },
}

#[derive(Default)]
pub struct Cache {
    map: Mutex<HashMap<String, Entry>>,
}

impl Cache {
    pub fn get(&self, key: &str) -> Option<Hit> {
        self.get_at(key, Instant::now())
    }

    pub fn fresh(&self, key: &str) -> Option<Value> {
        match self.get(key)? {
            Hit::Fresh(value) => Some(value),
            Hit::Stale { .. } => None,
        }
    }

    pub fn put(&self, key: String, value: Value, ttl: Duration) {
        self.put_at(key, value, ttl, Instant::now());
    }

    fn get_at(&self, key: &str, now: Instant) -> Option<Hit> {
        let map = self.map.lock().ok()?;
        let entry = map.get(key)?;
        Some(if entry.expires() > now {
            Hit::Fresh(entry.value.clone())
        } else {
            Hit::Stale {
                value: entry.value.clone(),
                age: now.saturating_duration_since(entry.stored_at),
            }
        })
    }

    fn put_at(&self, key: String, value: Value, ttl: Duration, now: Instant) {
        let Ok(mut map) = self.map.lock() else {
            return;
        };
        if map.len() >= CAPACITY && !map.contains_key(&key) {
            // Drop what expires soonest, so long-lived entries (decimals,
            // launch records of graduated coins) outlive a burst of boards.
            let mut by_expiry: Vec<(Instant, String)> =
                map.iter().map(|(k, e)| (e.expires(), k.clone())).collect();
            by_expiry.sort();
            for (_, k) in by_expiry.into_iter().take(CAPACITY / 8) {
                map.remove(&k);
            }
        }
        map.insert(
            key,
            Entry {
                stored_at: now,
                ttl,
                value,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stale_reads_carry_their_age() {
        let cache = Cache::default();
        let t0 = Instant::now();
        cache.put_at("k".into(), json!(1), Duration::from_secs(30), t0);
        assert_eq!(cache.get_at("k", t0), Some(Hit::Fresh(json!(1))));
        let later = t0 + Duration::from_secs(90);
        assert_eq!(
            cache.get_at("k", later),
            Some(Hit::Stale {
                value: json!(1),
                age: Duration::from_secs(90)
            })
        );
    }

    #[test]
    fn eviction_keeps_long_lived_entries() {
        let cache = Cache::default();
        let t0 = Instant::now();
        cache.put_at("forever".into(), json!(1), Duration::from_secs(86_400), t0);
        for i in 0..CAPACITY {
            cache.put_at(format!("board{i}"), json!(i), Duration::from_secs(30), t0);
        }
        assert!(cache.get_at("forever", t0).is_some());
        assert!(cache.map.lock().unwrap().len() <= CAPACITY);
    }
}
