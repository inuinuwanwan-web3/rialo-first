//! Synthetic data only. No RPC/HTTP or credential access.
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rialo-temporal-{}-{}",
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
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn sample(time: &str, slot: Value) -> Value {
    json!({"schema_version":"1.0","network":"devnet","operation":"account_lookup","timestamp_utc":time,"status":"not_found","context":{"slot":slot},"data":{"address":"11111111111111111111111111111111","account":null},"error":null})
}
fn first() -> Value {
    sample("2026-11-01T00:00:00.000Z", json!(1))
}
fn later() -> Value {
    sample("2026-11-01T00:00:01.250Z", json!(2))
}
fn run(paths: &[String], detail: bool) -> (i32, Value) {
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_report_analysis"));
    c.arg("--json");
    if detail {
        c.arg("--temporal-details");
    }
    let p = c.args(paths).output().unwrap();
    assert!(p.stderr.is_empty());
    (
        p.status.code().unwrap(),
        serde_json::from_slice(&p.stdout).unwrap(),
    )
}
fn pair(a: Value, b: Value) -> Value {
    let f = Files::new();
    let (code, v) = run(&[f.put("a.json", &a), f.put("b.json", &b)], true);
    assert_eq!(code, 0);
    v
}
#[test]
fn normal_interval_and_order() {
    let v = pair(first(), later());
    let t = &v["comparisons"][0]["temporal"];
    assert_eq!(t["interval_ms"], 1250);
    assert_eq!(t["time_basis"], "timestamp_utc_report_creation");
    assert_eq!(t["inference"], "field_differences_only");
}
#[test]
fn reversed_inputs() {
    let a = pair(first(), later());
    let b = pair(later(), first());
    assert_eq!(a, b);
}
#[test]
fn equal_timestamp_incomparable() {
    let mut b = first();
    b["context"]["slot"] = json!(2);
    let v = pair(first(), b);
    assert_eq!(v["comparisons"][0]["reason"], "same_timestamp");
    assert!(v["comparisons"][0].get("temporal").is_none());
}
#[test]
fn slot_increase() {
    let v = pair(first(), later());
    assert_eq!(
        v["comparisons"][0]["temporal"]["slot_change"],
        json!({"direction":"increased","magnitude":1})
    );
}
#[test]
fn slot_decrease() {
    let mut a = first();
    a["context"]["slot"] = json!(9);
    let v = pair(a, later());
    assert_eq!(
        v["comparisons"][0]["temporal"]["slot_change"],
        json!({"direction":"decreased","magnitude":7})
    );
}
#[test]
fn slot_unknown() {
    let mut a = first();
    a["context"]["slot"] = Value::Null;
    let v = pair(a, later());
    assert_eq!(
        v["comparisons"][0]["temporal"]["slot_change"],
        json!({"direction":"unknown","magnitude":null})
    );
}
#[test]
fn slot_unchanged() {
    let mut b = later();
    b["context"]["slot"] = json!(1);
    let v = pair(first(), b);
    assert_eq!(
        v["comparisons"][0]["temporal"]["slot_change"]["direction"],
        "unchanged"
    );
}
#[test]
fn status_and_presence_change() {
    let mut b = later();
    b["status"] = json!("success");
    b["data"]["account"] =
        json!({"owner":"11111111111111111111111111111111","executable":false,"balance_kelvin":0});
    let v = pair(first(), b);
    let t = &v["comparisons"][0]["temporal"];
    assert_eq!(t["status_changed"], true);
    assert_eq!(t["account_presence_before"], "absent");
    assert_eq!(t["account_presence_after"], "present");
}
#[test]
fn not_found_does_not_mean_present() {
    let v = pair(first(), later());
    let t = &v["comparisons"][0]["temporal"];
    assert_eq!(t["status_changed"], false);
    assert_eq!(t["account_presence_after"], "absent");
    assert_eq!(t["query_outcome_after"], "successful_response");
    assert_eq!(v["success"], 0);
    assert_eq!(v["not_found"], 2);
}
fn assert_separated(v: Value) {
    assert_eq!(v["comparisons"].as_array().unwrap().len(), 2);
    assert!(
        v["comparisons"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["comparable"] == false)
    );
}
#[test]
fn network_mismatch() {
    let mut b = later();
    b["network"] = json!("testnet");
    assert_separated(pair(first(), b));
}
#[test]
fn operation_mismatch() {
    let mut b = later();
    b["operation"] = json!("balance_status_snapshot");
    b["status"] = json!("success");
    b["data"] = json!({"address":"11111111111111111111111111111111","balance_kelvin":0});
    assert_separated(pair(first(), b));
}
#[test]
fn identifier_mismatch() {
    let mut b = later();
    b["data"]["address"] = json!(rialo_cdk::rpc::types::Pubkey::from([1u8; 32]).to_string());
    assert_separated(pair(first(), b));
}
#[test]
fn duplicates_not_fake_timepoints() {
    let v = pair(first(), first());
    assert_eq!(v["duplicates_skipped"], 1);
    assert_eq!(v["comparisons"][0]["reason"], "single_observation");
}
#[test]
fn corrupt_and_missing() {
    let f = Files::new();
    let bad = f.0.join("bad.json");
    std::fs::write(&bad, b"{").unwrap();
    assert_eq!(run(&[bad.to_str().unwrap().into()], true).0, 2);
    let mut v = first();
    v.as_object_mut().unwrap().remove("timestamp_utc");
    assert_eq!(run(&[f.put("missing.json", &v)], true).0, 2);
}
#[test]
fn private_text_excluded() {
    let mut b = later();
    b["provider_message"] = json!("SYNTHETIC_DO_NOT_EMIT");
    b["data"]["unrelated"] = json!("SYNTHETIC_DO_NOT_EMIT");
    assert!(
        !pair(first(), b)
            .to_string()
            .contains("SYNTHETIC_DO_NOT_EMIT")
    );
}
#[test]
fn missing_identifier_and_empty_inputs() {
    let f = Files::new();
    let mut b = later();
    b["data"].as_object_mut().unwrap().remove("address");
    let (code, v) = run(&[f.put("b.json", &b)], true);
    assert_eq!(code, 0);
    assert_eq!(v["incomparable_reports"], 1);
    assert!(v["comparisons"].as_array().unwrap().is_empty());
    assert_eq!(run(&[], true).0, 2);
}
#[test]
fn legacy_output_unchanged_without_optin() {
    let f = Files::new();
    let paths = [f.put("a.json", &first()), f.put("b.json", &later())];
    let (code, v) = run(&paths, false);
    assert_eq!(code, 0);
    assert!(v.get("temporal_details_version").is_none());
    assert!(v["comparisons"][0].get("temporal").is_none());
    let mut detailed = run(&paths, true).1;
    detailed
        .as_object_mut()
        .unwrap()
        .remove("temporal_details_version");
    for c in detailed["comparisons"].as_array_mut().unwrap() {
        c.as_object_mut().unwrap().remove("temporal");
    }
    assert_eq!(v, detailed);
}
#[test]
fn leap_day_and_year_interval() {
    let a = sample("2024-02-28T23:59:59.999Z", json!(1));
    let b = sample("2024-03-01T00:00:00.000Z", json!(1));
    assert_eq!(
        pair(a, b)["comparisons"][0]["temporal"]["interval_ms"],
        86400001u64
    );
    let a = sample("1999-12-31T23:59:59.999Z", json!(1));
    let b = sample("2000-01-01T00:00:00.000Z", json!(1));
    assert_eq!(pair(a, b)["comparisons"][0]["temporal"]["interval_ms"], 1);
}
#[test]
fn maximum_slot_no_signed_overflow() {
    let mut a = first();
    let mut b = later();
    a["context"]["slot"] = json!(0);
    b["context"]["slot"] = json!(u64::MAX);
    assert_eq!(
        pair(a, b)["comparisons"][0]["temporal"]["slot_change"]["magnitude"],
        json!(u64::MAX)
    );
}
#[test]
fn observation_completion_axis() {
    let a: Value = json!({"schema_version":"1.0","network":"devnet","operation":"account_lookup","status":"not_found","context":{"slot":1},"data":{"address":"11111111111111111111111111111111","account":null},"error":null,"transport":"bounded_http","http":null,"query_attempted":true,"verification":"not_asserted","source":{"kind":"configured_rpc","endpoint":"http://devnet.rialo.io:4100","rpc_method":"getAccountInfo","transport":"bounded_http"},"observation_started_at_utc":"1970-01-01T00:00:01.000Z","observation_completed_at_utc":"1970-01-01T00:00:02.000Z","record_created_at_utc":"1970-01-01T00:00:09.000Z","timestamp_utc":"1970-01-01T00:00:09.000Z"});
    let b: Value = json!({"schema_version":"1.0","network":"devnet","operation":"account_lookup","status":"success","context":{"slot":2},"data":{"address":"11111111111111111111111111111111","account":{"owner":"11111111111111111111111111111111","executable":false,"balance_kelvin":7,"rent_epoch":0,"space":0,"data":{"encoding":"base64","returned_bytes":0,"requested_offset":0,"requested_max_bytes":4096}}},"error":null,"transport":"bounded_http","http":null,"query_attempted":true,"verification":"not_asserted","source":{"kind":"configured_rpc","endpoint":"http://devnet.rialo.io:4100","rpc_method":"getAccountInfo","transport":"bounded_http"},"observation_started_at_utc":"1970-01-01T00:00:03.000Z","observation_completed_at_utc":"1970-01-01T00:00:04.000Z","record_created_at_utc":"1970-01-01T00:00:05.000Z","timestamp_utc":"1970-01-01T00:00:05.000Z"});
    let v = pair(a, b);
    assert_eq!(
        v["comparisons"][0]["temporal"]["time_basis"],
        "observation_completed_at_utc"
    );
    assert_eq!(v["comparisons"][0]["temporal"]["interval_ms"], 2000);
}
#[test]
fn failed_rpc_not_attributed_to_target() {
    let mut b = later();
    b["status"] = json!("timeout");
    b["data"] = Value::Null;
    b["error"] = json!({"kind":"timeout"});
    let v = pair(first(), b);
    assert_eq!(v["errors"], 1);
    assert_eq!(v["incomparable_reports"], 1);
    assert_eq!(v["comparisons"][0]["reason"], "single_observation");
}
