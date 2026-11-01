//! Offline only. Explicit local JSON files; no network or wallet discovery.
#[path = "../offline_reports/mod.rs"]
mod offline_reports;
fn main() {
    let mut json = false;
    let mut details = false;
    let mut paths = vec![];
    for arg in std::env::args().skip(1) {
        if arg == "--help" {
            println!(
                "report_analysis [--json] [--temporal-details] REPORT.json ... (offline only; max 64 files, 1MiB/file, 8MiB total)"
            );
            return;
        } else if arg == "--json" && !json {
            json = true;
        } else if arg == "--temporal-details" && !details {
            details = true;
        } else if arg.starts_with('-') {
            fail("invalid_arguments");
        } else {
            paths.push(arg);
        }
    }
    let result = if details {
        offline_reports::analyze_with_details(&paths)
    } else {
        offline_reports::analyze(&paths)
    };
    match result.and_then(|v| offline_reports::render(&v, json)) {
        Ok(s) => println!("{s}"),
        Err(e) => fail(e),
    }
}
fn fail(reason: &str) -> ! {
    println!(
        "{}",
        serde_json::json!({"status":"blocked","error":{"reason":reason}})
    );
    std::process::exit(2)
}
