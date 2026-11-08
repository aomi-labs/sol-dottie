//! The envelope around every reply, and the size rule: a reply must fit in
//! [`MAX_REPLY`] chars as the model sees it (the host pretty-prints tool
//! JSON, and guests share a 64k-byte input with the prompt and history).
use super::now;
use crate::runtime::{Fail, Turn};
use serde_json::{Map, Value, json};

pub const MAX_REPLY: usize = 3000;

/// A successful reply: `partial` when anything was missing, with the gaps
/// and the sources its numbers came from.
pub fn reply(data: Value, turn: &Turn) -> Value {
    let gaps = turn.gaps();
    let mut out = Map::new();
    out.insert(
        "status".into(),
        json!(if gaps.is_empty() { "ok" } else { "partial" }),
    );
    out.insert("as_of".into(), json!(now()));
    if let Value::Object(map) = data {
        out.extend(map);
    }
    out.insert("sources".into(), json!(turn.sources()));
    if !gaps.is_empty() {
        out.insert("gaps".into(), json!(gaps));
    }
    fit(compact(Value::Object(out)))
}

/// A failure the model can act on: what failed, whether retrying in this
/// answer can help, and when.
pub fn error(fail: &Fail) -> Value {
    let retry = fail.retry_after_s();
    compact(json!({
        "status": "error",
        "as_of": now(),
        "error": {
            "code": fail.code(),
            "message": fail.message(),
            "retry_in_this_answer": retry.is_some_and(|s| s <= 5),
            "retry_after_s": retry,
        }
    }))
}

/// Drops nulls and empty containers so replies stay small and "missing"
/// is never confused with zero.
pub fn compact(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, compact(v)))
                .filter(|(_, v)| match v {
                    Value::Null => false,
                    Value::Object(m) => !m.is_empty(),
                    Value::Array(a) => !a.is_empty(),
                    _ => true,
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(compact).collect()),
        other => other,
    }
}

/// Chars the model sees for this value (pretty-printed, like the host).
pub fn size(value: &Value) -> usize {
    serde_json::to_string_pretty(value)
        .map(|s| s.len())
        .unwrap_or(0)
}

/// Keeps a reply under [`MAX_REPLY`] by dropping the last items of its
/// largest list of objects (rows, holders, trades) and saying so.
pub fn fit(mut value: Value) -> Value {
    let mut trimmed: Option<(String, usize)> = None;
    // Leave room for the gap note added below.
    while size(&value) > MAX_REPLY - 100 {
        let Some(map) = value.as_object_mut() else {
            break;
        };
        let largest = map
            .iter()
            .filter(|(_, v)| {
                v.as_array()
                    .is_some_and(|a| a.len() > 1 && a.iter().all(Value::is_object))
            })
            .max_by_key(|(_, v)| size(v))
            .map(|(k, _)| k.clone());
        let Some(key) = largest else { break };
        let Some(list) = map.get_mut(&key).and_then(Value::as_array_mut) else {
            break;
        };
        list.pop();
        trimmed = Some((key, list.len()));
    }
    if let Some((key, kept)) = trimmed {
        let note = json!(format!("{key} cut to {kept} to keep the reply short"));
        match value.get_mut("gaps").and_then(Value::as_array_mut) {
            Some(gaps) => gaps.push(note),
            None => value["gaps"] = json!([note]),
        }
        value["status"] = json!("partial");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_drops_unknowns() {
        let v = compact(json!({"a": null, "b": {"c": null}, "d": [], "e": 0}));
        assert_eq!(v, json!({"e": 0}));
    }

    #[test]
    fn fit_trims_the_largest_row_list() {
        let rows: Vec<Value> = (0..40)
            .map(|i| json!({ "i": i, "pad": "x".repeat(80) }))
            .collect();
        let turn = Turn::default();
        let out = reply(
            json!({ "rows": rows, "small": [{ "a": 1 }, { "a": 2 }] }),
            &turn,
        );
        assert!(size(&out) <= MAX_REPLY);
        assert_eq!(out["status"], "partial");
        assert_eq!(out["small"].as_array().unwrap().len(), 2);
        assert!(out["gaps"][0].as_str().unwrap().starts_with("rows cut to"));
    }

    #[test]
    fn errors_say_when_to_retry() {
        let out = error(&Fail::RateLimited {
            source: crate::runtime::Source::Gmgn,
            retry_after_s: 3,
        });
        assert_eq!(out["error"]["code"], "RATE_LIMITED");
        assert_eq!(out["error"]["retry_in_this_answer"], true);
    }
}
