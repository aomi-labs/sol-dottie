//! Dottie's settings store: one small JSON document per host account in
//! Cloudflare KV, behind the settings Worker (`worker/index.ts`).
use crate::runtime::{Cost, Fail, Request, Runtime, Source, Turn};
use serde_json::Value;
use std::time::Duration;

const TTL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
pub struct Settings<'a> {
    rt: &'a Runtime,
    turn: &'a Turn,
}

impl<'a> Settings<'a> {
    pub fn new(rt: &'a Runtime, turn: &'a Turn) -> Self {
        Self { rt, turn }
    }

    /// The account's saved document; `{}` when it has none.
    pub async fn get(self, account: &str) -> Result<Value, Fail> {
        let (url, token) = self.store()?;
        let request = Request::new(Source::Settings, key(account), Cost::Free, TTL).live_only();
        self.rt
            .fetch(self.turn, request, || async {
                let response = self
                    .rt
                    .http
                    .get(format!("{url}/api/settings/{account}"))
                    .bearer_auth(token)
                    .send()
                    .await
                    .map_err(|_| Fail::unavailable(Source::Settings, "request failed"))?;
                read(response).await
            })
            .await
    }

    /// Replaces the account's document; later reads see it at once.
    pub async fn put(self, account: &str, doc: &Value) -> Result<(), Fail> {
        let (url, token) = self.store()?;
        let response = self
            .rt
            .http
            .put(format!("{url}/api/settings/{account}"))
            .bearer_auth(token)
            .json(doc)
            .send()
            .await
            .map_err(|_| Fail::unavailable(Source::Settings, "request failed"))?;
        read(response).await?;
        self.rt.cache.put(key(account), doc.clone(), TTL);
        Ok(())
    }

    fn store(self) -> Result<(&'a str, &'a str), Fail> {
        let (url, token) = self
            .turn
            .settings_store()
            .ok_or(Fail::Unconfigured(Source::Settings))?;
        Ok((url.trim_end_matches('/'), token))
    }
}

fn key(account: &str) -> String {
    format!("settings:{account}")
}

async fn read(response: reqwest::Response) -> Result<Value, Fail> {
    match response.status().as_u16() {
        200..=299 => response
            .json()
            .await
            .map_err(|_| Fail::bad(Source::Settings, "reply not JSON")),
        401 | 403 => Err(Fail::Unconfigured(Source::Settings)),
        status => Err(Fail::unavailable(
            Source::Settings,
            format!("HTTP {status}"),
        )),
    }
}
