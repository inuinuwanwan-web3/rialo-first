//! Bounded manual verification. Only three reviewed read-only RPC methods.
use rialo_cdk::rpc::{
    HttpRpcClient,
    types::{GetSignaturesForAddressConfig, Signature},
};
use rialo_first::{
    Result, TESTNET_RPC,
    monitor::{
        self, MonitorRpc,
        query::{self, Query, QueryRpc},
    },
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const ADDRESS: &str = "7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa";
const TX: &str =
    "5h5JFQgEdyS6fJzVCugGxdbfcg7Wj6auUGmPuLPMV76u1vFENhAhWeuUK2UnrFGvW1nKTb9EXit7QsJqL8RushTs";
const TIMEOUT: Duration = Duration::from_secs(10);
fn time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
struct Recorder {
    client: HttpRpcClient,
    calls: RefCell<Vec<Value>>,
}
impl QueryRpc for Recorder {
    async fn query(&self, request: Query) -> Result<Value> {
        let method = match &request {
            Query::History(_) => "getSignaturesForAddress",
            Query::Detail(_) => "getTransaction",
        };
        let result = QueryRpc::query(&self.client, request).await;
        self.calls.borrow_mut().push(match &result {
            Ok(value) => json!({"time_unix":time(),"method":method,"rpc_result":value}),
            Err(e) => json!({"time_unix":time(),"method":method,"rpc_error":e.to_string()}),
        });
        result
    }
}
fn inputs(args: &[String]) -> Result<(&'static str, Vec<&'static str>, bool)> {
    let historical = args == ["--live-readonly", "--historical-candidates"];
    if args != ["--live-readonly"] && !historical {
        return Err("explicit --live-readonly [--historical-candidates] required".into());
    }
    let address = if historical {
        "FAyTs54yLpZ4KRHot8dVpJaoJJAibwkfEi9ck3NjHVc6"
    } else {
        ADDRESS
    };
    let ids = if historical {
        vec![
            "5TiohMeL3ryJJPQKqJ7Dxr7oUAj2ZNnu9qRv1m3Sws6omMQDT6x9vvUtNbthVubd1Nvdeb6dkGdMJ6uGPHC8myz7",
            "5o9vrT8LrU3U6Gith4FxCRzHTrHNCm5xJ9AWQCQB8A9yxaXgu1oWP4xdVJhmPD2P8oRB4oouY1StAjSnWkQKmFG8",
        ]
    } else {
        vec![TX]
    };
    Ok((address, ids, historical))
}
#[tokio::main]
async fn main() -> Result<()> {
    let (address, ids, historical) = inputs(&std::env::args().skip(1).collect::<Vec<_>>())?;
    let recorder = Recorder {
        client: HttpRpcClient::new(TESTNET_RPC.to_owned()),
        calls: RefCell::new(vec![]),
    };
    let mut output =
        json!({"started_unix":time(),"rpc":TESTNET_RPC,"address":address,"tx_ids":ids});
    let run = tokio::time::timeout(
        Duration::from_secs(if historical { 55 } else { 45 }),
        async {
            let mut details = Vec::new();
            for id in &ids {
                let value = match query::transaction_detail(&recorder, id, TIMEOUT).await {
                    Ok(d) => json!({"signature":id,"result":d}),
                    Err(e) => json!({"signature":id,"error":e.to_string()}),
                };
                details.push(value);
            }
            output["details"] = json!(details);
            let signatures: Vec<Signature> = ids
                .iter()
                .map(|id| Signature::from_str(id))
                .collect::<std::result::Result<_, _>>()?;
            output["status"] = match tokio::time::timeout(
                TIMEOUT,
                MonitorRpc::statuses(&recorder.client, &signatures),
            )
            .await
            {
                Ok(Ok(statuses)) if statuses.len() == signatures.len() => {
                    let sdk = serde_json::to_value(&statuses)?;
                    let observations: Vec<_> = signatures
                        .iter()
                        .zip(statuses)
                        .map(|(id, status)| monitor::observe_status(id, status))
                        .collect();
                    json!({"time_unix":time(),"sdk_statuses":sdk,"observations":observations})
                }
                Ok(Ok(_)) => json!({"error":"status response length mismatch"}),
                Ok(Err(e)) => json!({"error":e.to_string()}),
                Err(_) => json!({"error":"status timeout"}),
            };
            let mut pages = vec![];
            let mut before = None;
            let mut seen = std::collections::HashSet::new();
            let mut cross_page_duplicates = 0;
            for _ in 0..2 {
                let config = GetSignaturesForAddressConfig {
                    limit: Some(2),
                    before,
                    until: None,
                };
                match query::history_page(&recorder, address, config, TIMEOUT).await {
                    Ok(page) => {
                        for entry in &page.entries {
                            if !seen.insert(entry.signature.clone()) {
                                cross_page_duplicates += 1;
                            }
                        }
                        before = page.next_before.clone();
                        pages.push(serde_json::to_value(page)?);
                        if before.is_none() {
                            break;
                        }
                    }
                    Err(e) => {
                        pages.push(json!({"error":e.to_string()}));
                        break;
                    }
                }
            }
            output["history_pages"] = json!(pages);
            output["unique_history_tx_count"] = json!(seen.len());
            output["cross_page_duplicates"] = json!(cross_page_duplicates);
            Ok::<_, Box<dyn std::error::Error>>(())
        },
    )
    .await;
    output["run_error"] = match run {
        Ok(Ok(())) => Value::Null,
        Ok(Err(e)) => json!(e.to_string()),
        Err(_) => json!("overall timeout"),
    };
    output["query_calls"] = json!(recorder.calls.borrow().as_slice());
    output["finished_unix"] = json!(time());
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_selection_is_bounded_and_explicit() {
        let (a, ids, historical) =
            inputs(&["--live-readonly".into(), "--historical-candidates".into()]).unwrap();
        assert!(historical);
        assert_eq!(a, "FAyTs54yLpZ4KRHot8dVpJaoJJAibwkfEi9ck3NjHVc6");
        assert_eq!(ids.len(), 2);
        assert!(!ids.contains(&TX));
        for id in ids {
            assert!(Signature::from_str(id).is_ok());
        }
        let (a, ids, historical) = inputs(&["--live-readonly".into()]).unwrap();
        assert_eq!(a, ADDRESS);
        assert_eq!(ids, vec![TX]);
        assert!(!historical);
        for args in [
            vec![],
            vec!["--historical-candidates".into()],
            vec!["--live-readonly".into(), "--other".into()],
        ] {
            assert!(inputs(&args).is_err());
        }
    }
}
