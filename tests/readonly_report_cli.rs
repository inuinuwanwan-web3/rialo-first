//! Actual CLI formatting only; never passes --execute or a valid monitor RPC invocation.
use serde_json::{Value,json};
use std::process::{Command,Output};
fn run(binary:&str,args:&[&str])->Output {Command::new(binary).args(args).env_remove("RUST_LOG").output().unwrap()}
fn value(output:&Output)->Value {serde_json::from_slice(&output.stdout).unwrap()}
const ADDRESS:&str="11111111111111111111111111111111";
#[test]
fn account_legacy_sdk_and_bounded_dry_outputs_are_preserved() {
    let p=run(env!("CARGO_BIN_EXE_devnet_account"),&["--address",ADDRESS]);assert!(p.status.success());assert_eq!(value(&p),json!({"network":"devnet","rpc":"http://devnet.rialo.io:4100","executed":false}));
    let p=run(env!("CARGO_BIN_EXE_devnet_account"),&["--address",ADDRESS,"--bounded-http"]);assert_eq!(value(&p)["transport"],"bounded_http");assert!(value(&p).get("schema_version").is_none());
}
#[test]
fn account_json_mode_never_claims_dry_run_success() {
    for mode in [vec!["--address",ADDRESS,"--json"],vec!["--address",ADDRESS,"--bounded-http","--json"]] {let p=run(env!("CARGO_BIN_EXE_devnet_account"),&mode);assert!(p.status.success());let v=value(&p);assert_eq!(v["schema_version"],"1.0");assert_eq!(v["status"],"unverified");assert_eq!(v["query_attempted"],false);}
}
#[test]
fn account_invalid_inputs_and_duplicate_json_are_sanitized_json() {
    for args in [vec!["--json"],vec!["--address","sk-secret-input","--json"],vec!["--json","--json"]] {let p=run(env!("CARGO_BIN_EXE_devnet_account"),&args);assert_eq!(p.status.code(),Some(2));assert!(p.stderr.is_empty());assert_eq!(value(&p)["status"],"blocked");assert!(!String::from_utf8_lossy(&p.stdout).contains("sk-secret"));}
}
#[test]
fn status_json_and_legacy_outputs_are_separate() {
    let f:Value=serde_json::from_str(include_str!("fixtures/phase7z/synthetic.json")).unwrap();let signature=f["signatures"][0].as_str().unwrap();
    let old=run(env!("CARGO_BIN_EXE_devnet_status"),&["--signature",signature]);assert!(old.status.success());assert_eq!(value(&old)["executed"],false);assert!(value(&old).get("schema_version").is_none());
    let new=run(env!("CARGO_BIN_EXE_devnet_status"),&["--signature",signature,"--json"]);assert!(new.status.success());assert_eq!(value(&new)["operation"],"transaction_status");assert_eq!(value(&new)["status"],"unverified");
}
#[test]
fn status_and_monitor_parse_errors_are_json_without_rpc() {
    let p=run(env!("CARGO_BIN_EXE_devnet_status"),&["--json"]);assert_eq!(p.status.code(),Some(2));assert_eq!(value(&p)["status"],"blocked");assert!(p.stderr.is_empty());
    let p=run(env!("CARGO_BIN_EXE_rialo-first"),&["--testnet-monitor","--address","bad","--json"]);assert_eq!(p.status.code(),Some(2));assert_eq!(value(&p)["network"],"testnet");assert_eq!(value(&p)["status"],"blocked");assert!(p.stderr.is_empty());
}
