//! Opt-in DevNet status query. No wallet, transaction construction, or persistence.
use rialo_cdk::{constants::URL_DEVNET, rpc::HttpRpcClient};
use rialo_first::status_readonly;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] {
        println!(
            "DevNet read-only status: --signature BASE58 [--signature BASE58 ...] [--timeout-secs 1..30] [--execute]\nDefault: validate only, no network. At most 256 distinct signatures; one request, no retries."
        );
        return;
    }
    let options = match status_readonly::parse(&args) {
        Ok(options) => options,
        Err(_) => {
            eprintln!("Status query blocked: invalid arguments.");
            std::process::exit(2);
        }
    };
    if !options.execute {
        println!(
            "{}",
            serde_json::json!({"network":"devnet", "rpc":URL_DEVNET, "validated_signatures":options.signatures.len(), "executed":false})
        );
        return;
    }
    let client = HttpRpcClient::new(URL_DEVNET.to_string());
    match status_readonly::query(&client, &options.signatures, options.timeout).await {
        Ok(report) => println!(
            "{}",
            serde_json::json!({"network":"devnet", "rpc":URL_DEVNET, "report":report})
        ),
        Err(_) => {
            eprintln!("Status query failed: no successful report; no retry performed.");
            std::process::exit(1);
        }
    }
}
