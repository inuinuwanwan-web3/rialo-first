//! Opt-in DevNet status query. No wallet, transaction construction, or persistence.
use rialo_cdk::{constants::URL_DEVNET, rpc::HttpRpcClient};
use rialo_first::readonly_report::{
    self as common, Failure, Network, Operation, Report, Transport,
};
use rialo_first::status_readonly;
use std::time::SystemTime;

#[tokio::main]
async fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let unified = raw.iter().any(|a| a == "--json");
    let args = match common::split_json(&raw) {
        Ok((args, _)) => args,
        Err(_) => {
            emit(Report::failure(
                Network::Devnet,
                Operation::TransactionStatus,
                Failure::blocked(),
                Transport::Sdk,
                false,
                SystemTime::now(),
            ));
            std::process::exit(2);
        }
    };
    if (args.is_empty() && !unified) || args == ["--help"] {
        println!(
            "DevNet read-only status: --signature BASE58 [--signature BASE58 ...] [--timeout-secs 1..30] [--execute] [--json]\nDefault: validate only, no network. At most 256 distinct signatures; one request, no retries."
        );
        return;
    }
    let options = match status_readonly::parse(&args) {
        Ok(options) => options,
        Err(_) => {
            if unified {
                emit(Report::failure(
                    Network::Devnet,
                    Operation::TransactionStatus,
                    Failure::blocked(),
                    Transport::Sdk,
                    false,
                    SystemTime::now(),
                ));
                std::process::exit(2);
            }
            eprintln!("Status query blocked: invalid arguments.");
            std::process::exit(2);
        }
    };
    if !options.execute {
        if unified {
            emit(Report::unverified(
                Network::Devnet,
                Operation::TransactionStatus,
                Transport::Sdk,
                SystemTime::now(),
            ));
            return;
        }
        println!(
            "{}",
            serde_json::json!({"network":"devnet", "rpc":URL_DEVNET, "validated_signatures":options.signatures.len(), "executed":false})
        );
        return;
    }
    let client = HttpRpcClient::new(URL_DEVNET.to_string());
    match status_readonly::query(&client, &options.signatures, options.timeout).await {
        Ok(report) if unified => {
            if !emit(Report::statuses(
                Network::Devnet,
                &report,
                Transport::Sdk,
                SystemTime::now(),
            )) {
                std::process::exit(1);
            }
        }
        Ok(report) => println!(
            "{}",
            serde_json::json!({"network":"devnet", "rpc":URL_DEVNET, "report":report})
        ),
        Err(error) => {
            if unified {
                emit(Report::failure(
                    Network::Devnet,
                    Operation::TransactionStatus,
                    Failure::error(error.as_ref()),
                    Transport::Sdk,
                    true,
                    SystemTime::now(),
                ));
                std::process::exit(1);
            }
            eprintln!("Status query failed: no successful report; no retry performed.");
            std::process::exit(1);
        }
    }
}
fn emit(report: Result<Report, common::ReportError>) -> bool {
    let output = common::finish(report, Network::Devnet, Operation::TransactionStatus);
    println!("{}", output.stdout);
    !output.failed
}
