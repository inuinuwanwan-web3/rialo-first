//! Explicit manual live verification; never run by cargo test.
use rialo_cdk::{
    RpcClient,
    rpc::{
        HttpRpcClient,
        types::{Pubkey, Signature, SignatureStatus},
    },
};
use rialo_first::{
    Result, TESTNET_RPC,
    monitor::{self, MonitorRpc},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
const ADDRESS: &str = "7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa";
const TX: &str =
    "5h5JFQgEdyS6fJzVCugGxdbfcg7Wj6auUGmPuLPMV76u1vFENhAhWeuUK2UnrFGvW1nKTb9EXit7QsJqL8RushTs";
fn stamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
// Reuse the height already read in the requested order, avoiding another RPC.
struct ObservedHeight {
    client: HttpRpcClient,
    height: u64,
}
impl MonitorRpc for ObservedHeight {
    async fn balance(&self, address: &Pubkey) -> Result<u64> {
        let balance = MonitorRpc::balance(&self.client, address).await?;
        println!("time_unix={} balance_kelvin={balance}", stamp());
        Ok(balance)
    }
    async fn height(&self) -> Result<u64> {
        Ok(self.height)
    }
    async fn statuses(&self, ids: &[Signature]) -> Result<Vec<Option<SignatureStatus>>> {
        MonitorRpc::statuses(&self.client, ids).await
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--live-readonly"] {
        return Err("explicit --live-readonly required".into());
    }
    println!(
        "time_unix={} endpoint={TESTNET_RPC} address={ADDRESS} tx={TX}",
        stamp()
    );
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let client = HttpRpcClient::new(TESTNET_RPC.to_owned());
        let prefix = client.get_config_hash_prefix().await?;
        println!(
            "time_unix={} connection=PASS config_prefix={prefix:?}",
            stamp()
        );
        let height = client.get_block_height().await?;
        println!("time_unix={} block_height={height}", stamp());
        let rpc = ObservedHeight { client, height };
        let result =
            monitor::snapshot(&rpc, ADDRESS, &[TX.to_owned()], Duration::from_secs(20)).await;
        println!("time_unix={} snapshot_complete", stamp());
        match result {
            Ok(snapshot) => {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
                Ok(())
            }
            Err(error) => {
                eprintln!("monitor_error={error}");
                Err(error)
            }
        }
    })
    .await
    .map_err(|_| "live verification timeout (30 seconds)")?;
    result
}
