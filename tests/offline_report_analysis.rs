#[path = "../src/offline_reports/mod.rs"]
mod analysis;
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
            "rialo-analysis-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn put(&self, n: &str, b: &[u8]) -> String {
        let p = self.0.join(n);
        std::fs::write(&p, b).unwrap();
        p.to_str().unwrap().into()
    }
    fn report(&self, n: &str, v: &Value) -> String {
        self.put(n, &serde_json::to_vec(v).unwrap())
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn sample() -> Value {
    json!({"schema_version":"1.0","network":"devnet","operation":"account_lookup","timestamp_utc":"2026-11-01T00:00:00.000Z","status":"not_found","context":{"slot":1},"data":{"address":"11111111111111111111111111111111","account":null},"error":null})
}
#[test]
fn single() {
    let f = Files::new();
    let v = analysis::analyze(&[f.report("a.json", &sample())]).unwrap();
    assert_eq!(v["total_reports"], 1);
    assert_eq!(v["not_found"], 1);
    assert_eq!(v["comparisons"][0]["comparable"], false);
}
#[test]
fn order_and_slot() {
    let f = Files::new();
    let a = sample();
    let mut b = a.clone();
    b["timestamp_utc"] = json!("2026-11-02T00:00:00.000Z");
    b["context"]["slot"] = json!(9);
    let v = analysis::analyze(&[f.report("b.json", &b), f.report("a.json", &a)]).unwrap();
    assert_eq!(v["comparisons"][0]["after"]["context_slot"], 9);
    assert_eq!(v["comparisons"][0]["changed"], true);
}
#[test]
fn mixed_network_not_compared() {
    let f = Files::new();
    let a = sample();
    let mut b = a.clone();
    b["network"] = json!("testnet");
    let v = analysis::analyze(&[f.report("a.json", &a), f.report("b.json", &b)]).unwrap();
    assert_eq!(v["by_network"]["devnet"], 1);
    assert!(
        v["comparisons"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["comparable"] == false)
    );
}
#[test]
fn statuses() {
    let f = Files::new();
    let mut paths = vec![];
    for (i, s) in [
        "success",
        "not_found",
        "rpc_error",
        "http_error",
        "timeout",
        "blocked",
        "unverified",
    ]
    .iter()
    .enumerate()
    {
        let mut v = sample();
        v["status"] = json!(s);
        if *s == "success" {
            v["operation"] = json!("balance_status_snapshot");
            v["data"] = json!({"address":"11111111111111111111111111111111","balance_kelvin":0});
        } else if *s != "not_found" {
            v["data"] = Value::Null;
            if *s != "unverified" {
                v["error"] = json!({"kind":s,"reason":"private message never emitted"});
            }
        }
        paths.push(f.report(&format!("{i}.json"), &v));
    }
    let r = analysis::analyze(&paths).unwrap();
    assert_eq!(r["errors"], 3);
    assert_eq!(r["blocked"], 1);
    assert_eq!(r["unverified"], 1);
    assert_eq!(r["by_operation"]["balance_status_snapshot"], 1);
    assert!(
        !analysis::render(&r, true)
            .unwrap()
            .contains("private message")
    );
}
#[test]
fn duplicates() {
    let f = Files::new();
    let p = f.report("a.json", &sample());
    let r = analysis::analyze(&[p.clone(), p]).unwrap();
    assert_eq!(r["total_reports"], 1);
    assert_eq!(r["duplicates_skipped"], 1);
}
#[test]
fn same_time() {
    let f = Files::new();
    let a = sample();
    let mut b = a.clone();
    b["context"]["slot"] = json!(2);
    let r = analysis::analyze(&[f.report("a.json", &a), f.report("b.json", &b)]).unwrap();
    assert_eq!(r["comparisons"][0]["reason"], "same_timestamp");
}
fn bad(v: Value) {
    let f = Files::new();
    assert!(analysis::analyze(&[f.report("a.json", &v)]).is_err());
}
#[test]
fn schema() {
    let mut v = sample();
    v["schema_version"] = json!("2.0");
    bad(v);
}
#[test]
fn missing() {
    let mut v = sample();
    v.as_object_mut().unwrap().remove("error");
    bad(v);
}
#[test]
fn invalid_date() {
    let mut v = sample();
    v["timestamp_utc"] = json!("2026-02-30T00:00:00.000Z");
    bad(v);
}
#[test]
fn invalid_network() {
    let mut v = sample();
    v["network"] = json!("other");
    bad(v);
}
#[test]
fn invalid_status() {
    let mut v = sample();
    v["status"] = json!("PASS");
    bad(v);
}
#[test]
fn invalid_slot() {
    let mut v = sample();
    v["context"]["slot"] = json!(-1);
    bad(v);
}
#[test]
fn invalid_absence() {
    let mut v = sample();
    v["data"]["account"] = json!({});
    bad(v);
}
#[test]
fn empty_and_malformed() {
    let f = Files::new();
    for b in [b"".as_slice(), b"{", b"null"] {
        assert!(analysis::analyze(&[f.put("a.json", b)]).is_err());
    }
}
#[test]
fn integers_and_float() {
    let f = Files::new();
    for n in ["18446744073709551616", "1e999", "1.5"] {
        let t = serde_json::to_string(&sample())
            .unwrap()
            .replace("\"slot\":1", &format!("\"slot\":{n}"));
        assert!(analysis::analyze(&[f.put("a.json", t.as_bytes())]).is_err());
    }
}
#[test]
fn nesting() {
    let f = Files::new();
    let s = format!("{}0{}", "[".repeat(33), "]".repeat(33));
    assert_eq!(
        analysis::analyze(&[f.put("a.json", s.as_bytes())]).unwrap_err(),
        "nesting_limit"
    );
}
#[test]
fn file_size() {
    let f = Files::new();
    assert_eq!(
        analysis::analyze(&[f.put("a.json", &vec![b' '; 1048577])]).unwrap_err(),
        "file_size_limit"
    );
}
#[test]
fn file_count() {
    assert_eq!(
        analysis::analyze(&vec!["no.json".into(); 65]).unwrap_err(),
        "file_count_limit"
    );
}
#[test]
fn total_size() {
    let f = Files::new();
    let mut b = serde_json::to_vec(&sample()).unwrap();
    b.resize(1048576, b' ');
    let p = f.put("a.json", &b);
    assert_eq!(
        analysis::analyze(&vec![p; 9]).unwrap_err(),
        "total_size_limit"
    );
}
#[test]
fn read_failure() {
    assert_eq!(
        analysis::analyze(&["/tmp/nonexistent-rialo-analysis.json".into()]).unwrap_err(),
        "read_failed"
    );
}
#[test]
fn sensitive_path_refused_before_open() {
    assert_eq!(
        analysis::analyze(&["/tmp/wallet.json".into()]).unwrap_err(),
        "disallowed_input_path"
    );
}
#[test]
fn output_modes_and_limit() {
    let f = Files::new();
    let r = analysis::analyze(&[f.report("a.json", &sample())]).unwrap();
    assert!(serde_json::from_str::<Value>(&analysis::render(&r, true).unwrap()).is_ok());
    assert!(analysis::render(&r, false).unwrap().contains("比較不可"));
    assert_eq!(
        analysis::render(&json!({"x":"x".repeat(1048576)}), true).unwrap_err(),
        "output_size_limit"
    );
}
#[test]
fn missing_identifier() {
    let f = Files::new();
    let mut v = sample();
    v["data"].as_object_mut().unwrap().remove("address");
    let r = analysis::analyze(&[f.report("a.json", &v)]).unwrap();
    assert_eq!(r["incomparable_reports"], 1);
}
#[test]
fn balance_change() {
    let f = Files::new();
    let mut a = sample();
    a["operation"] = json!("balance_status_snapshot");
    a["status"] = json!("success");
    a["data"] = json!({"address":"11111111111111111111111111111111","balance_kelvin":1});
    let mut b = a.clone();
    b["timestamp_utc"] = json!("2026-11-02T00:00:00.000Z");
    b["data"]["balance_kelvin"] = json!(0);
    let r = analysis::analyze(&[f.report("a.json", &a), f.report("b.json", &b)]).unwrap();
    assert_eq!(
        r["comparisons"][0]["before"]["observed"]["balance_kelvin"],
        1
    );
    assert_eq!(
        r["comparisons"][0]["after"]["observed"]["balance_kelvin"],
        0
    );
}
#[test]
fn actual_cli() {
    let f = Files::new();
    let p = f.report("a.json", &sample());
    for args in [vec![p.as_str()], vec!["--json", p.as_str()]] {
        let r = std::process::Command::new(env!("CARGO_BIN_EXE_report_analysis"))
            .args(args.clone())
            .output()
            .unwrap();
        assert!(r.status.success());
        assert!(r.stderr.is_empty());
        if args.len() == 2 {
            assert!(serde_json::from_slice::<Value>(&r.stdout).is_ok());
        }
    }
}
#[test]
fn tx_state_change() {
    let f = Files::new();
    let signature = rialo_cdk::rpc::types::Signature::from([1u8; 64]).to_string();
    let mut a = sample();
    a["status"] = json!("success");
    a["operation"] = json!("transaction_status");
    a["data"] = json!({"transactions":[{"signature":signature,"slot":1,"executed":false,"execution":"Unconfirmed","error_present":false}]});
    let mut b = a.clone();
    b["timestamp_utc"] = json!("2026-11-02T00:00:00.000Z");
    b["data"]["transactions"][0]["execution"] = json!("Succeeded");
    b["data"]["transactions"][0]["executed"] = json!(true);
    let r = analysis::analyze(&[f.report("a.json", &a), f.report("b.json", &b)]).unwrap();
    assert_eq!(r["comparisons"][0]["changed"], true);
}
#[test]
fn account_state_change() {
    let f = Files::new();
    let a = sample();
    let mut b = a.clone();
    b["timestamp_utc"] = json!("2026-11-02T00:00:00.000Z");
    b["status"] = json!("success");
    b["data"]["account"] = json!({"owner":"11111111111111111111111111111111","executable":false,"balance_kelvin":2,"space":0,"rent_epoch":0,"data":{"encoding":"base64","returned_bytes":0,"requested_max_bytes":4096,"requested_offset":0},"secret":"never expose this"});
    let r = analysis::analyze(&[f.report("a.json", &a), f.report("b.json", &b)]).unwrap();
    assert_eq!(
        r["comparisons"][0]["after"]["observed"]["data"]["returned_bytes"],
        0
    );
    assert!(!analysis::render(&r, true).unwrap().contains("never expose"));
}
#[test]
fn symlink_refused() {
    let f = Files::new();
    let p = f.report("a.json", &sample());
    let link = f.0.join("b.json");
    std::os::unix::fs::symlink(p, &link).unwrap();
    assert_eq!(
        analysis::analyze(&[link.to_str().unwrap().into()]).unwrap_err(),
        "regular_file_required"
    );
}
#[test]
fn history_not_guessed() {
    let f = Files::new();
    let mut v = sample();
    v["operation"] = json!("transaction_history");
    v["status"] = json!("success");
    v["data"] = json!({"entries":[]});
    let r = analysis::analyze(&[f.report("a.json", &v)]).unwrap();
    assert_eq!(r["incomparable_reports"], 1);
}
#[test]
fn same_time_ambiguity_not_chained() {
    let f = Files::new();
    let a = sample();
    let mut b = a.clone();
    b["context"]["slot"] = json!(2);
    let mut c = a.clone();
    c["timestamp_utc"] = json!("2026-11-02T00:00:00.000Z");
    let r = analysis::analyze(&[
        f.report("a.json", &a),
        f.report("b.json", &b),
        f.report("c.json", &c),
    ])
    .unwrap();
    assert!(
        r["comparisons"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["comparable"] == false)
    );
}
#[test]
fn wrong_numeric_type() {
    let mut v = sample();
    v["status"] = json!("success");
    v["operation"] = json!("balance_status_snapshot");
    v["data"] = json!({"address":"11111111111111111111111111111111","balance_kelvin":true});
    bad(v);
}
