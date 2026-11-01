//! Opt-in account read. No credentials, raw Data output, retries, or persistence.
use rialo_cdk::{constants::URL_DEVNET, rpc::HttpRpcClient};
use rialo_first::account_readonly::{self, AccountError};
use rialo_first::bounded_http::{self, BoundedHttp, Limits};
use rialo_first::readonly_report::{
    self as common, Failure, Network, Operation, Report, Transport,
};
use std::time::SystemTime;

#[tokio::main]
async fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let unified = raw.iter().any(|a| a == "--json");
    let args = match common::split_json(&raw) {
        Ok((args, _)) => args,
        Err(_) => fail_mode(
            AccountError::InvalidInput,
            2,
            unified,
            Transport::ProvidedData,
        ),
    };
    if (args.is_empty() && !unified) || args == ["--help"] {
        println!(
            "DevNet public account: --address PUBLIC_KEY [--timeout-secs 1..30] [--execute] [--bounded-http [--max-body-bytes 1..8388608]] [--json]\nDefault: input validation only; SDK transport. --bounded-http selects capped streaming (default 1MiB), compression/redirects/retries disabled. Raw Data is never printed; slice request capped at 4096 bytes."
        );
        return;
    }
    let mode = if args.iter().any(|a| a == "--bounded-http") {
        Transport::BoundedHttp
    } else {
        Transport::Sdk
    };
    let (options, selection) = match bounded_http::parse_cli(&args) {
        Ok(options) => options,
        Err(error) => fail_mode(error, 2, unified, mode),
    };
    if !options.execute {
        if unified {
            emit(Report::unverified(
                Network::Devnet,
                Operation::AccountLookup,
                mode,
                SystemTime::now(),
            ));
            return;
        }
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
            Err(error) => fail_mode(error, 1, unified, mode),
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
            if unified {
                if !emit(Report::account(
                    Network::Devnet,
                    &observation.report,
                    receipt.as_ref(),
                    mode,
                    SystemTime::now(),
                )) {
                    std::process::exit(1);
                }
                return;
            }
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
        Err(error) => fail_mode(error, 1, unified, mode),
    }
}
fn fail_mode(error: AccountError, code: i32, unified: bool, mode: Transport) -> ! {
    if unified {
        emit(Report::failure(
            Network::Devnet,
            Operation::AccountLookup,
            Failure::account(&error),
            mode,
            code != 2,
            SystemTime::now(),
        ));
        std::process::exit(code);
    }
    eprintln!("{}", serde_json::json!({"error":error}));
    std::process::exit(code);
}
fn emit(report: Result<Report, common::ReportError>) -> bool {
    let output = common::finish(report, Network::Devnet, Operation::AccountLookup);
    println!("{}", output.stdout);
    !output.failed
}
