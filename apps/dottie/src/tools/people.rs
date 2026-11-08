use super::run;
use crate::domain::people::{self, Mode, Query};
use crate::runtime::{DottieApp, Lane};
use aomi_sdk::schemars::JsonSchema;
use aomi_sdk::{DynAomiTool, DynToolCallCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ModeArg {
    /// Top traders by PnL over `window`.
    Leaders,
    /// One trader by handle: verified wallet, PnL, Solana positions.
    Trader,
    /// The most-liked theses holders wrote on a coin (one page).
    Theses,
    /// Which Fomo trader owns a wallet.
    Owner,
    /// The coins Fomo users trade now (`board`).
    Trending,
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
pub enum WindowArg {
    #[serde(rename = "24h")]
    H24,
    #[serde(rename = "7d")]
    D7,
    #[serde(rename = "30d")]
    D30,
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum BoardArg {
    Trending,
    MostHeld,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PeopleArgs {
    pub mode: ModeArg,
    /// leaders window. null = 24h.
    pub window: Option<WindowArg>,
    /// trending board. null = trending.
    pub board: Option<BoardArg>,
    /// trader: Fomo handle, e.g. "@Binkieee".
    pub handle: Option<String>,
    /// theses: exact Solana mint.
    pub token: Option<String>,
    /// owner: Solana wallet address.
    pub wallet: Option<String>,
    /// leaders and trending rows, 1–20. null = 10.
    pub limit: Option<u8>,
}

pub struct People;

impl DynAomiTool for People {
    type App = DottieApp;
    type Args = PeopleArgs;
    const NAME: &'static str = "dottie_people";
    const DESCRIPTION: &'static str = "Real traders from Fomo. leaders: top traders by PnL (24h/7d/30d) with their Solana wallet and top coins. trader: one handle's verified wallet, PnL and Solana positions (or holdings; coverage top_token_summary means neither had Solana rows). theses: what holders wrote about a mint, most liked, with each writer's position and PnL. owner: which trader owns a wallet. trending: coins Fomo users trade now or hold most. Each call spends credits: one call per question.";

    fn run(app: &DottieApp, args: PeopleArgs, ctx: DynToolCallCtx) -> Result<Value, String> {
        run(app, &ctx, Lane::Chat, async |rt, turn| {
            let query = Query {
                mode: match args.mode {
                    ModeArg::Leaders => Mode::Leaders,
                    ModeArg::Trader => Mode::Trader,
                    ModeArg::Theses => Mode::Theses,
                    ModeArg::Owner => Mode::Owner,
                    ModeArg::Trending => Mode::Trending,
                },
                window: match args.window.unwrap_or(WindowArg::H24) {
                    WindowArg::H24 => "24h",
                    WindowArg::D7 => "7d",
                    WindowArg::D30 => "30d",
                },
                board: match args.board.unwrap_or(BoardArg::Trending) {
                    BoardArg::Trending => "trending",
                    BoardArg::MostHeld => "most-held",
                },
                handle: args.handle.as_deref(),
                token: args.token.as_deref(),
                wallet: args.wallet.as_deref(),
                limit: u32::from(args.limit.unwrap_or(10).clamp(1, 20)),
            };
            people::people(rt, turn, &query).await
        })
    }
}
