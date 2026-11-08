use super::{address, run};
use crate::domain::find::{exact, is_dust, search};
use crate::domain::market::{known_symbol, lookalike, same_ends};
use crate::runtime::{DottieApp, Fail, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindArgs {
    /// Ticker ("BONK", "$WIF"), name, or an exact Solana mint.
    pub query: String,
}

pub struct Find;

impl DynAomiTool for Find {
    type App = DottieApp;
    type Args = FindArgs;
    const NAME: &'static str = "dottie_find";
    const DESCRIPTION: &'static str = "Resolve a Solana ticker, name or exact mint. A mint returns exactly that token or NOT_FOUND, never a substitute. A ticker returns candidate mints, exact symbol first, then by liquidity, with KOL holder counts and honeypot flags; lookalikes are hidden. SOL, stables, LSTs, majors and xStocks resolve from Dottie's verified catalog first.";

    fn run(app: &DottieApp, args: FindArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let q = args.query.trim().trim_start_matches('$');
            if q.is_empty() || q.len() > 64 {
                return Err(Fail::Invalid("query must be 1–64 characters".into()));
            }
            if crate::providers::pubkey(q).is_ok() {
                let token = address(q)?;
                let exact = exact(rt, turn, &token).await?;
                return Ok(
                    json!({ "match": "exact_mint", "token": crate::view::token::candidate(&exact) }),
                );
            }
            if let Some(asset) = known_symbol(q) {
                return Ok(json!({
                    "query": q,
                    "official": { "symbol": asset.symbol, "token": asset.mint, "kind": asset.kind },
                    "note": "from Dottie's verified catalog; other mints with this ticker are unrelated copies",
                }));
            }
            // Candidates come deepest liquidity first, so a spoof sits below
            // the mint it copies.
            let mut found: Vec<crate::domain::find::Candidate> = vec![];
            let mut spoofs = 0;
            for c in search(rt, turn, q).await? {
                if lookalike(&c.token).is_some()
                    || found.iter().any(|k| same_ends(&k.token, &c.token))
                {
                    spoofs += 1;
                } else {
                    found.push(c);
                }
            }
            if found.is_empty() {
                return Err(Fail::NotFound("no Solana token matches that name".into()));
            }
            let same = found.iter().filter(|c| c.exact_symbol).count();
            let dust = found.iter().filter(|c| is_dust(c)).count();
            let shown: Vec<Value> = found
                .iter()
                .filter(|c| !is_dust(c))
                .take(5)
                .map(crate::view::token::candidate)
                .collect();
            Ok(json!({
                "query": q,
                "same_symbol_mints": same,
                "dead_copies_hidden": dust,
                "spoofed_addresses_hidden": spoofs,
                "ranked_by": "exact symbol, then liquidity (not proof of the real coin)",
                "candidates": shown,
            }))
        })
    }
}
