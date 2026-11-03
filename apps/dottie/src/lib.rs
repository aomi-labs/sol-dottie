use aomi_sdk::*;
pub mod domain;
pub mod providers;
pub mod runtime;
pub mod tools;
pub mod view;

/// Bump this when you ship a change so `dottie_about` shows which build is running.
pub const BUILD: &str = "v1";

dyn_aomi_app!(
    app = runtime::DottieApp,
    name = "dottie",
    version = "0.1.0",
    preamble = include_str!("preamble.md"),
    tools = [tools::About, tools::Assets, tools::TokenCheck, tools::FluxbeamQuote, tools::FluxbeamPrepare,
             tools::SolomonProduct, tools::SolomonPosition, tools::SolomonPrepare],
    namespaces = ["aomi-core", "svm-core"],
    skills = [
        { id: "dottie/research", description: "Research a Solana token with RugCheck; distinguish missing data from low risk.", tags: ["research", "rugcheck", "solana"], sections: { instructions: "skills/research.md" } },
        { id: "dottie/trade", description: "Direct FluxBeam quotes and preparation readiness; use existing Jupiter for common swaps and curated stock tokens.", tags: ["swap", "fluxbeam", "stocks"], sections: { instructions: "skills/trade.md" } },
        { id: "dottie/earn", description: "Explain current Solomon USDv rewards and read wallet enrollment and positions without inventing yield.", tags: ["solomon", "usdv", "rewards"], sections: { instructions: "skills/earn.md" } },
    ],
);
