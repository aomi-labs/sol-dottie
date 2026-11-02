use crate::runtime::{Failure, Read};
use serde_json::{Value, json};
pub fn result(value: Result<Value, Failure>) -> Result<Value, String> {
    Ok(value.unwrap_or_else(
        |e| json!({"status":"unavailable","error":{"code":e.code,"message":e.message}}),
    ))
}
pub fn sourced(read: &Read, source: &str, data: Value) -> Value {
    json!({"status":"ok","source":source,"fetched_at_unix":read.fetched_at,"cached":read.cached,"data":data})
}
pub fn unavailable(code: &str, reason: &str) -> Value {
    json!({"status":"unavailable","error":{"code":code,"message":reason},"transaction":null})
}
