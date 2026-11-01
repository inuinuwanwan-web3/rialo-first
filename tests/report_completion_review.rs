//! Focused review gaps: operation isolation and exclusion of arbitrary input text.
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Inputs(PathBuf);
impl Inputs {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rialo-review-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn put(&self, n: &str, v: &Value) -> String {
        let p = self.0.join(n);
        std::fs::write(&p, serde_json::to_vec(v).unwrap()).unwrap();
        p.to_str().unwrap().into()
    }
}
impl Drop for Inputs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn sample() -> Value {
    json!({"schema_version":"1.0","network":"devnet","operation":"account_lookup","status":"not_found","context":{"slot":1},"data":{"address":"11111111111111111111111111111111","account":null},"error":null,"transport":"bounded_http","http":null,"query_attempted":true,"verification":"not_asserted","source":{"kind":"configured_rpc","endpoint":"http://devnet.rialo.io:4100","rpc_method":"getAccountInfo","transport":"bounded_http"},"observation_started_at_utc":"1970-01-01T00:00:01.000Z","observation_completed_at_utc":"1970-01-01T00:00:02.000Z","record_created_at_utc":"1970-01-01T00:00:09.000Z","timestamp_utc":"1970-01-01T00:00:09.000Z"})
}
fn run(paths: &[String]) -> Value {
    let p = std::process::Command::new(env!("CARGO_BIN_EXE_report_analysis"))
        .arg("--json")
        .args(paths)
        .output()
        .unwrap();
    assert!(p.status.success());
    assert!(p.stderr.is_empty());
    serde_json::from_slice(&p.stdout).unwrap()
}
#[test]
fn different_operations_never_form_a_pair() {
    let f = Inputs::new();
    let mut a = sample();
    for key in [
        "observation_started_at_utc",
        "observation_completed_at_utc",
        "record_created_at_utc",
        "source",
    ] {
        a.as_object_mut().unwrap().remove(key);
    }
    let mut b = a.clone();
    b["operation"] = json!("balance_status_snapshot");
    b["timestamp_utc"] = json!("1970-01-01T00:00:10.000Z");
    b["status"] = json!("success");
    b["data"] = json!({"address":"11111111111111111111111111111111","balance_kelvin":1});
    let r = run(&[f.put("a.json", &a), f.put("b.json", &b)]);
    assert_eq!(r["total_reports"], 2);
    assert_eq!(r["by_operation"]["account_lookup"], 1);
    assert_eq!(r["by_operation"]["balance_status_snapshot"], 1);
    assert_eq!(r["comparisons"].as_array().unwrap().len(), 2);
    assert!(
        r["comparisons"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["comparable"] == false)
    );
}
#[test]
fn arbitrary_input_text_is_never_forwarded() {
    let f = Inputs::new();
    let mut a = sample();
    a["unknown_private_text"] = json!("SYNTHETIC_DO_NOT_EMIT");
    a["data"]["provider_body"] = json!("SYNTHETIC_DO_NOT_EMIT");
    a["source"]["unknown_text"] = json!("SYNTHETIC_DO_NOT_EMIT");
    let r = run(&[f.put("a.json", &a)]);
    assert!(
        !serde_json::to_string(&r)
            .unwrap()
            .contains("SYNTHETIC_DO_NOT_EMIT")
    );
    assert_eq!(r["not_found"], 1);
}
