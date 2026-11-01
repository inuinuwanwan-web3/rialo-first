//! Opt-in account read. No credentials, raw Data output, retries, or persistence.
use rialo_cdk::{constants::URL_DEVNET, rpc::HttpRpcClient};
use rialo_first::account_readonly::{self, AccountError};
use rialo_first::bounded_http::{self, BoundedHttp, Limits};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] {
        println!(
            "DevNet public account: --address PUBLIC_KEY [--timeout-secs 1..30] [--execute] [--bounded-http [--max-body-bytes 1..8388608]]\nDefault: input validation only; SDK transport. --bounded-http selects capped streaming (default 1MiB), compression/redirects/retries disabled. Raw Data is never printed; slice request capped at 4096 bytes."
        );
        return;
    }
    let (options, selection) = match bounded_http::parse_cli(&args) {
        Ok(options) => options,
        Err(error) => fail(error, 2),
    };
    if !options.execute {
        if selection.bounded {
            println!(
                "{}",
                serde_json::json!({"network":"devnet","rpc":URL_DEVNET,"executed":false,"transport":"bounded_http","max_body_bytes":selection.body_bytes})
            );
            return;
        }
        println!(
            "{}",
            serde_json::json!({"network":"devnet","rpc":URL_DEVNET,"executed":false})
        );
        return;
    }
    let (result, receipt) = if selection.bounded {
        let mut limits = Limits {
            body_bytes: selection.body_bytes,
            total: options.timeout,
            ..Limits::default()
        };
        limits.connect = limits.connect.min(options.timeout);
        limits.read = limits.read.min(options.timeout);
        let rpc = match BoundedHttp::devnet(limits) {
            Ok(rpc) => rpc,
            Err(error) => fail(error, 1),
        };
        let result = account_readonly::query(&rpc, &options.address, options.timeout).await;
        (result, rpc.receipt())
    } else {
        let rpc = HttpRpcClient::new(URL_DEVNET.to_string());
        (
            account_readonly::query(&rpc, &options.address, options.timeout).await,
            None,
        )
    };
    match result {
        Ok(observation) => {
            if let Some(receipt) = receipt {
                println!(
                    "{}",
                    serde_json::json!({"network":"devnet","rpc":URL_DEVNET,"report":observation.report,"http_receipt":receipt})
                );
            } else {
                println!(
                    "{}",
                    serde_json::json!({"network":"devnet","rpc":URL_DEVNET,"report":observation.report})
                );
            }
        }
        Err(error) => fail(error, 1),
    }
}
fn fail(error: AccountError, code: i32) -> ! {
    eprintln!("{}", serde_json::json!({"error":error}));
    std::process::exit(code);
}
