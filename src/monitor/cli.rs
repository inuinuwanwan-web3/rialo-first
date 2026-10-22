//! Testnet-only CLI boundary. Errors never print provider response bodies or input values.
use super::{MonitorRpc, snapshot, validate_request};
use crate::{Result, TESTNET_RPC};
use rialo_cdk::{RpcClient, rpc::HttpRpcClient};
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub struct Options {
    address: String,
    tx_ids: Vec<String>,
    timeout: Duration,
}
impl Options {
    pub fn parse(args: &[String]) -> Result<Self> {
        let (mut address, mut timeout, mut rpc) = (None, None, None);
        let mut tx_ids = Vec::new();
        let mut iter = args.iter();
        while let Some(flag) = iter.next() {
            let value = iter.next().ok_or("monitor option requires a value")?;
            match flag.as_str() {
                "--address" if address.is_none() => address = Some(value.clone()),
                "--tx-hash" => tx_ids.push(value.clone()),
                "--rpc" if rpc.is_none() => {
                    // Exact allowlist also rejects credentials, query strings and other networks.
                    if value != TESTNET_RPC {
                        return Err(
                            "monitor RPC must be the configured official Testnet URL".into()
                        );
                    }
                    rpc = Some(());
                }
                "--timeout-secs" if timeout.is_none() => {
                    let seconds: u64 = value.parse().map_err(|_| "invalid monitor timeout")?;
                    if !(1..=30).contains(&seconds) {
                        return Err("monitor timeout must be 1..30 seconds".into());
                    }
                    timeout = Some(Duration::from_secs(seconds));
                }
                _ => return Err("unknown or duplicate monitor option".into()),
            }
        }
        let address = address.ok_or("monitor requires --address")?;
        validate_request(&address, &tx_ids)?;
        Ok(Self {
            address,
            tx_ids,
            timeout: timeout.unwrap_or(Duration::from_secs(30)),
        })
    }
}
#[allow(async_fn_in_trait)]
pub trait ConfigRpc: MonitorRpc {
    async fn config_prefix(&self) -> Result<u64>;
}
impl ConfigRpc for HttpRpcClient {
    async fn config_prefix(&self) -> Result<u64> {
        Ok(self.get_config_hash_prefix().await?.value())
    }
}
/// JSON on stdout, sanitized error on stderr, and an exit code used by the actual CLI.
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}
fn failure(message: &str) -> Output {
    Output {
        code: 1,
        stdout: serde_json::json!({"error":message}).to_string(),
        stderr: message.to_owned(),
    }
}
pub async fn execute(options: &Options, rpc: &impl ConfigRpc) -> Output {
    let result = tokio::time::timeout(options.timeout, async {
        let prefix = rpc.config_prefix().await?;
        let snapshot = snapshot(rpc, &options.address, &options.tx_ids, options.timeout).await?;
        let mut value = serde_json::to_value(snapshot)?;
        value["network_config_prefix"] = serde_json::json!(prefix);
        value["rpc"] = serde_json::json!(TESTNET_RPC);
        // Succeeded means executed=true/err absent, not a finality assertion.
        for tx in value["transactions"].as_array_mut().unwrap() {
            if tx["execution"] == "Succeeded" {
                tx["execution"] = serde_json::json!("Confirmed");
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(serde_json::to_string_pretty(&value)?)
    })
    .await;
    match result {
        Ok(Ok(stdout)) => Output {
            code: 0,
            stdout,
            stderr: String::new(),
        },
        Err(_) => failure("monitor RPC timeout"),
        Ok(Err(_)) => failure("monitor RPC or response error"),
    }
}
pub async fn run(options: &Options) -> Output {
    execute(options, &HttpRpcClient::new(TESTNET_RPC.to_owned())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rialo_cdk::rpc::types::{Pubkey, Signature, SignatureStatus};
    struct Mock {
        balance: u64,
        mode: u8,
    }
    impl MonitorRpc for Mock {
        async fn balance(&self, _: &Pubkey) -> Result<u64> {
            Ok(self.balance)
        }
        async fn height(&self) -> Result<u64> {
            Ok(123)
        }
        async fn statuses(&self, ids: &[Signature]) -> Result<Vec<Option<SignatureStatus>>> {
            Ok(ids
                .iter()
                .map(|_| match self.mode {
                    3 => None,
                    _ => Some(SignatureStatus {
                        slot: 1,
                        executed: self.mode != 4,
                        err: (self.mode == 2).then(|| "fixture".into()),
                    }),
                })
                .collect())
        }
    }
    impl ConfigRpc for Mock {
        async fn config_prefix(&self) -> Result<u64> {
            match self.mode {
                5 => Err("secret-provider-body".into()),
                6 => std::future::pending().await,
                _ => Ok(42),
            }
        }
    }
    fn args(tail: &[&str]) -> Vec<String> {
        let mut a = vec!["--address".into(), Pubkey::from([1; 32]).to_string()];
        a.extend(tail.iter().map(|s| s.to_string()));
        a
    }
    async fn output(balance: u64, mode: u8) -> Output {
        let mut options =
            Options::parse(&args(&["--tx-hash", &Signature::from([1; 64]).to_string()])).unwrap();
        options.timeout = Duration::from_millis(5);
        execute(&options, &Mock { balance, mode }).await
    }
    async fn json(balance: u64, mode: u8) -> serde_json::Value {
        let out = output(balance, mode).await;
        assert_eq!(out.code, 0);
        assert!(out.stderr.is_empty());
        serde_json::from_str(&out.stdout).unwrap()
    }
    #[tokio::test]
    async fn normal_balance() {
        assert_eq!(json(42, 0).await["balance_kelvin"], 42);
    }
    #[tokio::test]
    async fn zero_balance() {
        assert_eq!(json(0, 0).await["balance_kelvin"], 0);
    }
    #[tokio::test]
    async fn height_and_config_json() {
        let j = json(42, 0).await;
        assert_eq!(j["block_height"], 123);
        assert_eq!(j["network_config_prefix"], 42);
        assert!(j.get("confirmation_depth").is_none());
        assert!(j.get("finality").is_none());
    }
    #[tokio::test]
    async fn tx_confirmed() {
        assert_eq!(
            json(42, 0).await["transactions"][0]["execution"],
            "Confirmed"
        );
    }
    #[tokio::test]
    async fn tx_failed() {
        assert_eq!(json(42, 2).await["transactions"][0]["execution"], "Failed");
    }
    #[tokio::test]
    async fn tx_absent_unknown() {
        assert_eq!(json(42, 3).await["transactions"][0]["execution"], "Unknown");
    }
    #[tokio::test]
    async fn tx_unexecuted_unknown() {
        assert_eq!(json(42, 4).await["transactions"][0]["execution"], "Unknown");
    }
    #[tokio::test]
    async fn timeout_exit_code() {
        let o = output(0, 6).await;
        assert_eq!(o.code, 1);
        assert!(o.stderr.contains("timeout"));
    }
    #[tokio::test]
    async fn error_exit_code_redacted_json() {
        let o = output(0, 5).await;
        assert_eq!(o.code, 1);
        assert!(!o.stdout.contains("secret-provider"));
        assert!(!o.stderr.contains("secret-provider"));
        assert!(
            serde_json::from_str::<serde_json::Value>(&o.stdout)
                .unwrap()
                .get("error")
                .is_some()
        );
    }
    #[test]
    fn invalid_address() {
        assert!(Options::parse(&["--address".into(), "invalid".into()]).is_err());
    }
    #[test]
    fn invalid_tx() {
        assert!(Options::parse(&args(&["--tx-hash", "invalid"])).is_err());
    }
    #[test]
    fn rpc_and_options_validation() {
        assert!(Options::parse(&args(&["--rpc", TESTNET_RPC, "--timeout-secs", "1"])).is_ok());
        for tail in [
            vec!["--rpc", "http://testnet.rialo.io/"],
            vec!["--rpc", "https://user:secret@testnet.rialo.io/"],
            vec!["--send", "yes"],
            vec!["--timeout-secs", "0"],
            vec!["--timeout-secs", "31"],
            vec!["--timeout-secs", "bad"],
            vec!["--address", "duplicate"],
            vec!["--rpc", TESTNET_RPC, "--rpc", TESTNET_RPC],
            vec!["--tx-hash"],
        ] {
            assert!(Options::parse(&args(&tail)).is_err());
        }
    }
}
