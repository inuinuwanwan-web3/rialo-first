//! Opt-in account read. No credentials, raw Data output, retries, or persistence.
use rialo_cdk::{constants::URL_DEVNET, rpc::HttpRpcClient};
use rialo_first::account_readonly::{self, AccountError};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] {
        println!(
            "DevNet public account: --address PUBLIC_KEY [--timeout-secs 1..30] [--execute]\nDefault: input validation only. One getAccountInfo request, no retries. Raw Data is never printed; slice request capped at 4096 bytes."
        );
        return;
    }
    let options = match account_readonly::parse(&args) {
        Ok(options) => options,
        Err(error) => fail(error, 2),
    };
    if !options.execute {
        println!(
            "{}",
            serde_json::json!({"network":"devnet","rpc":URL_DEVNET,"executed":false})
        );
        return;
    }
    let rpc = HttpRpcClient::new(URL_DEVNET.to_string());
    match account_readonly::query(&rpc, &options.address, options.timeout).await {
        Ok(observation) => println!(
            "{}",
            serde_json::json!({"network":"devnet","rpc":URL_DEVNET,"report":observation.report})
        ),
        Err(error) => fail(error, 1),
    }
}
fn fail(error: AccountError, code: i32) -> ! {
    eprintln!("{}", serde_json::json!({"error":error}));
    std::process::exit(code);
}
