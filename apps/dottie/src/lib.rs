//! Dottie: a Solana trading friend. Hoodit's habits on Solana: GMGN finds
//! coins and labels wallets, Fomo brings real traders and their theses,
//! RugCheck reads authorities and fees, Jupiter proves the exit, and the
//! host's Jupiter path places trades.
//!
//! Layers, one way down: `tools` (thin adapters) → `domain` (Dottie's
//! rules) → `providers` (one per source) → `runtime` (cache, gates, the
//! one request path); `view` builds every model-facing reply.
use aomi_sdk::*;

pub mod card;
pub mod domain;
pub mod providers;
pub mod runtime;
pub mod tools;
pub mod view;

/// Bump this when you ship a change so `dottie_about` shows which build is running.
pub const BUILD: &str = "v4.0";

const PREAMBLE: &str = include_str!("preamble.md");

const GMGN: Secret = Secret::new(
    runtime::GMGN_KEY,
    "GMGN OpenAPI key for discovery boards, KOL and smart-money feeds, token research and wallet records",
    true,
);
const FOMO: Secret = Secret::new(
    runtime::FOMO_KEY,
    "Fomo API key for top traders, their positions and token theses (credits)",
    false,
);
const JUPITER: Secret = Secret::new(
    runtime::JUPITER_KEY,
    "Jupiter API key for exit quotes; keyless quotes are paced",
    false,
);
const RPC: Secret = Secret::new(
    runtime::RPC_URL,
    "Solana mainnet RPC URL for balances and PnL receipts; default is the public endpoint",
    false,
);
const SETTINGS_URL: Secret = Secret::new(
    runtime::SETTINGS_URL,
    "Base URL of Dottie's settings Worker",
    false,
);
const SETTINGS_TOKEN: Secret = Secret::new(
    runtime::SETTINGS_TOKEN,
    "Bearer token for Dottie's settings Worker",
    false,
);

dyn_aomi_app!(
    app = runtime::DottieApp,
    name = "dottie",
    version = "0.2.0",
    preamble = PREAMBLE,
    tools = [
        tools::Signals,
        tools::Scan,
        tools::Find,
        tools::Token,
        tools::People,
        tools::Chart,
        tools::Trades,
        tools::Holders,
        tools::Wallet,
        tools::Exit,
        tools::Check,
        tools::CardTool,
        tools::History,
        tools::Positions,
        tools::Pnl,
        tools::About,
        tools::Assets,
        tools::Preview,
        tools::Buy,
        tools::Sell,
        tools::PositionsView,
        tools::SettingsView
    ],
    secrets = [GMGN, FOMO, JUPITER, RPC, SETTINGS_URL, SETTINGS_TOKEN],
    namespaces = ["aomi-core", "svm-core"],
    skills = [
        {
            id: "dottie/research",
            description: "Find and judge Solana memecoins and small coins: what KOLs, smart money and top Fomo traders are buying, what's trending in a market-cap band, pump.fun and other curves near graduation, opinions on a ticker or mint, charts, who is buying or dumping, holders and the dev, what holders say, wallets, bag checks, and whether a size can get out. Not needed for SOL, stables or stock tokens.",
            tags: ["memecoins", "solana", "pump.fun", "kol", "smart money", "fomo", "scan", "chart", "holders", "exit"],
            sections: { instructions: "skills/research.md" },
        },
        {
            id: "dottie/trade",
            description: "Execute a requested Solana buy, sell or send through the host's Jupiter path (stage_jupiter_swap, then svm_simulate and svm_commit_txs). Use dottie_token mode trade for small-coin buy checks; preserve simulation and wallet consent. Activate only missing skills.",
            tags: ["buy", "sell", "ape", "swap", "slippage", "jupiter", "stocks"],
            sections: { instructions: "skills/trade.md" },
        },
        {
            id: "dottie/card",
            description: "PnL cards: a shareable image of a trade's profit, after a sell that closes a position, when the user asks for a pnl or flex card, or for a \"Redraw my … card\" message from a card's button.",
            tags: ["pnl", "card", "flex", "share", "image", "redraw"],
            sections: { instructions: "skills/card.md" },
        },
        {
            id: "dottie/watch",
            description: "Set, list or cancel alerts on a Solana coin: tell me when the curve hits X%, the price crosses a level, the dev sells, KOLs pile in.",
            tags: ["alert", "watch", "notify", "remind"],
            sections: { instructions: "skills/watch.md" },
        },
        {
            id: "dottie/settings",
            description: "Change quick-action settings: the Telegram buy and sell buttons, what pays for quick buys (SOL or USDC), default slippage.",
            tags: ["settings", "quick buy", "buttons", "slippage", "preferences"],
            tools: [tools::SettingsGet, tools::SettingsSet],
            sections: { instructions: "skills/settings.md" },
        },
        {
            id: "dottie/fluxbeam",
            description: "FluxBeam only when the user names it: direct WSOL/FLUXB quotes and why FluxBeam execution is disabled.",
            tags: ["fluxbeam", "fluxb"],
            tools: [tools::FluxbeamQuote, tools::FluxbeamPrepare],
            sections: { instructions: "skills/fluxbeam.md" },
        },
        {
            id: "dottie/earn",
            description: "Yield on Solana: Solomon USDv rewards and a wallet's enrollment, plus Kamino lending and Marinade staking through their host skills, without inventing yield.",
            tags: ["solomon", "usdv", "rewards", "yield", "kamino", "marinade", "staking"],
            tools: [tools::SolomonProduct, tools::SolomonPosition, tools::SolomonPrepare],
            sections: { instructions: "skills/earn.md" },
        },
    ],
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_has_unowned_direct_and_skill_owned_tools_and_small_prompts() {
        let manifest = runtime::DottieApp::default().manifest();
        assert_eq!(manifest.version, "0.2.0");
        let names: Vec<&str> = manifest.tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "dottie_signals",
                "dottie_scan",
                "dottie_find",
                "dottie_token",
                "dottie_people",
                "dottie_chart",
                "dottie_trades",
                "dottie_holders",
                "dottie_wallet",
                "dottie_exit",
                "dottie_check",
                "dottie_card",
                "dottie_history",
                "dottie_positions",
                "dottie_pnl",
                "dottie_about",
                "dottie_assets",
                "dottie_preview",
                "dottie_buy",
                "dottie_sell",
                "dottie_positions_view",
                "dottie_settings_view",
                "dottie_settings_get",
                "dottie_settings_set",
                "dottie_fluxbeam_quote",
                "dottie_fluxbeam_prepare",
                "dottie_solomon_product",
                "dottie_solomon_position",
                "dottie_solomon_prepare"
            ]
        );
        assert!(!names.contains(&"dottie_token_check"));
        let direct: Vec<&str> = manifest
            .tools
            .iter()
            .filter(|t| t.namespace.as_deref() == Some("direct"))
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(direct, &names[17..22]);
        let owned = |id: &str| {
            manifest
                .skills
                .iter()
                .find(|s| s.id == id)
                .unwrap()
                .injected_tools
                .clone()
        };
        assert_eq!(
            owned("dottie/settings"),
            ["dottie_settings_get", "dottie_settings_set"]
        );
        assert_eq!(
            owned("dottie/fluxbeam"),
            ["dottie_fluxbeam_quote", "dottie_fluxbeam_prepare"]
        );
        assert_eq!(
            owned("dottie/earn"),
            [
                "dottie_solomon_product",
                "dottie_solomon_position",
                "dottie_solomon_prepare"
            ]
        );
        let secrets: Vec<(&str, bool)> = manifest
            .secrets
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|s| (s.name.as_str(), s.required))
            .collect();
        assert_eq!(secrets[0], ("GMGN_API_KEY", true));
        assert!(secrets[1..].iter().all(|(_, required)| !required));
        assert!(
            manifest
                .tools
                .iter()
                .all(|t| t.parameters_schema["additionalProperties"] == false)
        );
        // Always-on tools only: direct tools never reach the model, and
        // skill-owned ones only once their skill is active.
        let owned: Vec<&String> = manifest
            .skills
            .iter()
            .flat_map(|s| &s.injected_tools)
            .collect();
        let descriptions: usize = manifest
            .tools
            .iter()
            .filter(|t| t.namespace.is_none() && !owned.contains(&&t.name))
            .map(|t| t.description.len())
            .sum();
        assert!(
            descriptions < 5600,
            "tool descriptions are {descriptions} chars"
        );
        for skill in &manifest.skills {
            let chars: usize = skill.sections.iter().map(|s| s.content.len()).sum();
            assert!(chars < 7000, "{} is {chars} chars", skill.id);
        }
        assert!(manifest.preamble.len() < 3000);
    }

    /// The host makes every property required, so optional ones must accept null.
    #[test]
    fn optional_arguments_accept_null() {
        let manifest = runtime::DottieApp::default().manifest();
        for tool in &manifest.tools {
            let schema = &tool.parameters_schema;
            let required: Vec<&str> = schema["required"]
                .as_array()
                .map(|r| r.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            for (name, prop) in schema["properties"].as_object().into_iter().flatten() {
                if required.contains(&name.as_str()) {
                    continue;
                }
                let text = prop.to_string();
                assert!(
                    text.contains("\"null\""),
                    "{}.{name} is optional but not nullable: {text}",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn tool_object_schemas_always_have_properties() {
        fn check(schema: &serde_json::Value, tool: &str) {
            match schema {
                serde_json::Value::Object(fields) => {
                    if schema["type"] == "object"
                        || schema["type"]
                            .as_array()
                            .is_some_and(|types| types.iter().any(|kind| kind == "object"))
                    {
                        assert!(schema["properties"].is_object(), "{tool}: {schema}");
                    }
                    for value in fields.values() {
                        check(value, tool);
                    }
                }
                serde_json::Value::Array(items) => {
                    for value in items {
                        check(value, tool);
                    }
                }
                _ => {}
            }
        }
        for tool in runtime::DottieApp::default().manifest().tools {
            check(&tool.parameters_schema, &tool.name);
        }
    }
}
