use super::{address, run};
use crate::card::{self, Card, Hero, Look, Show, Style, format as f};
use crate::runtime::{DottieApp, Fail, Lane, Turn};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CardArgs {
    /// Exact Solana mint of the coin. null only with redraw.
    pub token: Option<String>,
    /// Solana wallet. null = the user's.
    pub wallet: Option<String>,
    /// Signature of the sell the host just reported, so the card covers that round. null = the latest round.
    pub signature: Option<String>,
    /// card_id of an earlier card: same numbers, new look. Carry over only the change the user asked for.
    pub redraw: Option<String>,
    /// Only when the user names a style. null = keep (redraw) or a random one that fits the result.
    pub style: Option<Style>,
    /// true = a different random style (the 🎲 button). null = false.
    pub another_style: Option<bool>,
    /// The big number. null = keep (redraw) or pct.
    pub big: Option<Hero>,
    /// Every detail to show. null = keep (redraw) or usd, pct, mult, inout, held.
    pub show: Option<Vec<Show>>,
    /// The user's name, only when they ask for it on the card.
    pub name: Option<String>,
}

pub struct CardTool;

/// What a redraw needs: the frozen figures and the last look.
#[derive(Serialize, Deserialize)]
struct Saved {
    card: Card,
    look: Look,
    style: Style,
}

const KEEP: Duration = Duration::from_secs(24 * 3600);

impl DynAomiTool for CardTool {
    type App = DottieApp;
    type Args = CardArgs;
    const NAME: &'static str = "dottie_card";
    const DESCRIPTION: &'static str = "Draws a shareable PnL card (square image) for a wallet's trade round in one coin, from its trade records: profit, return, multiple, cost in and dollars out, time held. Telegram sends the image as a photo with buttons. Use after a sell that closes a position, when the user asks for a PnL card, or for a \"Redraw my … card (card …)\" message. Never pass numbers; the card computes them.";

    fn run(app: &DottieApp, args: CardArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let (figures, mut look, last) = match args.redraw.as_deref() {
                Some(id) => {
                    let saved = rt
                        .cache
                        .fresh(&cache_key(turn, id.trim()))
                        .and_then(|v| serde_json::from_value::<Saved>(v).ok())
                        .ok_or_else(|| {
                            Fail::NotFound("that card has expired; make a new one".into())
                        })?;
                    (saved.card, saved.look, Some(saved.style))
                }
                None => {
                    let token = address(
                        args.token
                            .as_deref()
                            .ok_or_else(|| Fail::Invalid("token is required".into()))?,
                    )?;
                    let wallet = match args.wallet.as_deref() {
                        Some(w) => address(w)?,
                        None => turn.wallet.clone().ok_or(Fail::NoWallet)?,
                    };
                    let figures = crate::domain::pnl::card(
                        rt,
                        turn,
                        &wallet,
                        &token,
                        args.signature.as_deref(),
                    )
                    .await?;
                    (figures, Look::default(), None)
                }
            };
            if let Some(big) = args.big {
                look.hero = big;
            }
            if let Some(show) = args.show {
                look.show = show.into_iter().collect();
            }
            if let Some(name) = args.name.filter(|n| !n.trim().is_empty()) {
                look.name = Some(name);
                look.show.insert(Show::Name);
            }
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as u64);
            let shuffle = args.another_style.unwrap_or(false);
            let (style, style_note) = match last {
                // A redraw keeps its style unless asked for a new one.
                Some(kept) if args.style.is_none() && !shuffle => (kept, None),
                // "Another style" never lands on the same one.
                _ => card::pick(&figures, args.style, last.filter(|_| shuffle), seed),
            };
            let id = args
                .redraw
                .as_deref()
                .map(str::trim)
                .map(str::to_string)
                .unwrap_or_else(card_id);
            let png = card::png(&figures, &look, style).map_err(Fail::Invalid)?;
            let path = save(&id, &png)?;
            rt.cache.put(
                cache_key(turn, &id),
                serde_json::to_value(Saved {
                    card: figures.clone(),
                    look: look.clone(),
                    style,
                })
                .unwrap_or(Value::Null),
                KEEP,
            );
            Ok(json!({
                "card_id": id,
                "card": summary(&figures),
                "style_used": style.name(),
                "style_note": style_note,
                "shown": look.show,
                "big": look.hero,
                "image_path": path,
                "caption": caption(&figures),
                "controls": controls(&figures, &look, &id),
            }))
        })
    }
}

fn cache_key(turn: &Turn, id: &str) -> String {
    format!("card:{}:{}:{id}", turn.thread.len(), turn.thread)
}

/// New cards have independent looks, even for the same round. Redraws keep their ID.
fn card_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "c{time:x}-{:x}-{:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Writes the PNG where the Telegram host sends photos from.
fn save(id: &str, png: &[u8]) -> Result<String, Fail> {
    let dir = std::env::var("AOMI_TELEGRAM_IMAGE_DIR")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = dir.join(format!("dottie-card-{id}-{}.png", card_id()));
    let mut file = std::fs::File::create_new(&path)
        .map_err(|_| Fail::Invalid("couldn't save the card image".into()))?;
    std::io::Write::write_all(&mut file, png)
        .map_err(|_| Fail::Invalid("couldn't save the card image".into()))?;
    Ok(path.to_string_lossy().into_owned())
}

/// The card's own numbers, for the model to quote.
pub(crate) fn summary(c: &Card) -> Value {
    let plain = |s: String| f::plain(&s);
    let mut out = json!({
        "symbol": c.symbol,
        "state": if c.open { "open" } else { "closed" },
        "result": c.mood(),
        "profit": c.pnl().map(|p| plain(f::usd(p, true))),
        "return": c.pct().map(f::pct),
        "multiple": c.mult().map(f::mult),
        "cost_in": c.cost.map(|v| plain(f::usd(v, false))),
        "held": c.held_s.map(f::held),
        "cost_usd": c.cost,
        "basis_known": c.cost.is_some(),
        "proceeds_or_value_usd": c.out,
        "profit_usd": c.pnl(),
        "return_pct": c.pct(),
        "return_scope": "Whole round: (sale proceeds + value now - total buy cost) / total buy cost.",
        "sale_proceeds_usd": c.proceeds,
        "realized_pnl_usd": c.realized,
        "unrealized_pnl_usd": c.unrealized,
        "fees_sol": c.fee_sol,
        "fee_transactions": c.fee_transactions,
        "accounting": c.accounting_note,
        "fee_policy": "Headline PnL excludes network fees and token-account rent (rent comes back when the account closes). Fees are what the wallet paid on these fills. Route fees are already in wallet flows. USDC is valued at $1; SOL legs use the round's implied SOL price, an estimate. Realized/unrealized use moving weighted-average cost.",
    });
    out[if c.open && c.sells > 0 {
        "sold_plus_value_now"
    } else if c.open {
        "value_now"
    } else {
        "out"
    }] = json!(plain(f::usd(c.out, false)));
    out
}

fn caption(c: &Card) -> String {
    let sym = format!("${}", c.symbol);
    let text = match (c.pnl(), c.pct()) {
        (Some(p), Some(r)) if c.open => {
            let kind = if c.sells > 0 {
                "PnL so far"
            } else {
                "unrealized"
            };
            format!("{sym} open: {} ({}) {kind}", f::usd(p, true), f::pct(r))
        }
        (Some(p), Some(r)) => format!("{sym} closed: {} ({})", f::usd(p, true), f::pct(r)),
        _ if c.open => format!("{sym} open: estimated value {}", f::usd(c.out, false)),
        _ => format!("{sym} sold for {}", f::usd(c.out, false)),
    };
    let fees = c
        .fee_sol
        .map(|g| format!(" · fees {g:.6} SOL"))
        .unwrap_or_default();
    f::plain(&format!(
        "{text} · {}{fees}",
        c.accounting_note.to_lowercase()
    ))
}

/// Two buttons: another style, and switching the big number.
fn controls(c: &Card, look: &Look, id: &str) -> Value {
    let who = format!("my ${} card (card {id})", f::clamp(&c.symbol, 14));
    let mut out = vec![json!({
        "label": "🎲 Another style",
        "action": "say",
        "text": format!("Redraw {who} in a different random style."),
    })];
    let (label, what, value) = match look.hero {
        Hero::Pct if c.mult().is_some() => {
            ("🚀 Show multiplier", "the multiple", c.mult().map(f::mult))
        }
        Hero::Pct => (
            "💵 Show $ profit",
            "the dollar profit",
            c.pnl().map(|p| f::usd(p, true)),
        ),
        _ => ("📈 Show %", "the return %", c.pct().map(f::pct)),
    };
    if let Some(value) = value {
        out.push(json!({
            "label": label,
            "action": "say",
            "text": format!("Redraw {who} with {what} ({}) as the big number.", f::plain(&value)),
        }));
    }
    json!(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cards_do_not_overwrite_each_others_redraws() {
        assert_ne!(card_id(), card_id());
    }
}
