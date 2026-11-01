//! Offline report analysis. No client construction, discovery, or input writes.
use rialo_cdk::rpc::types::{Pubkey, Signature};
use rialo_first::readonly_report::{MAX_JSON_BYTES, SCHEMA_VERSION};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
    str::FromStr,
};
pub const MAX_FILES: usize = 64;
pub const MAX_TOTAL: usize = 8 * MAX_JSON_BYTES;
const MAX_DEPTH: usize = 32;
const MAX_POINTS: usize = 4096;
type R<T> = Result<T, &'static str>;
#[derive(Clone)]
struct Point {
    time: String,
    fields: Value,
}
fn timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 24
        || [4, 7].iter().any(|&i| b[i] != b'-')
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'.'
        || b[23] != b'Z'
    {
        return false;
    }
    if b.iter()
        .enumerate()
        .any(|(i, c)| ![4, 7, 10, 13, 16, 19, 23].contains(&i) && !c.is_ascii_digit())
    {
        return false;
    }
    let n = |a: usize, z: usize| s[a..z].parse::<u32>().unwrap_or(99999);
    let (y, m, d) = (n(0, 4), n(5, 7), n(8, 10));
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    (1970..=9999).contains(&y)
        && d >= 1
        && d <= days
        && n(11, 13) < 24
        && n(14, 16) < 60
        && n(17, 19) < 60
}
fn depth(bytes: &[u8]) -> R<()> {
    let (mut level, mut string, mut escape) = (0usize, false, false);
    for &b in bytes {
        if string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                string = false;
            }
        } else {
            match b {
                b'"' => string = true,
                b'{' | b'[' => {
                    level += 1;
                    if level > MAX_DEPTH {
                        return Err("nesting_limit");
                    }
                }
                b'}' | b']' => level = level.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}
fn numbers(v: &Value) -> bool {
    match v {
        Value::Number(n) => n.as_u64().is_some() || n.as_i64().is_some(),
        Value::Array(a) => a.iter().all(numbers),
        Value::Object(o) => o.values().all(numbers),
        _ => true,
    }
}
fn identifier(v: &Value, signature: bool) -> R<String> {
    let s = v.as_str().ok_or("missing_identifier")?;
    let valid = if signature {
        Signature::from_str(s).is_ok()
    } else {
        Pubkey::from_str(s).is_ok()
    };
    if !valid {
        return Err("invalid_identifier");
    }
    Ok(s.to_owned())
}
fn selected(v: &Value, keys: &[&str]) -> R<Value> {
    let mut out = serde_json::Map::new();
    for &k in keys {
        if let Some(x) = v.get(k) {
            let safe = match k {
                "owner" => json!(identifier(x, false)?),
                "execution" => {
                    let text = x.as_str().ok_or("invalid_projection")?;
                    if !["Succeeded", "Failed", "Unconfirmed", "Unknown"].contains(&text) {
                        return Err("invalid_projection");
                    }
                    json!(text)
                }
                "executable" | "executed" | "error_present" => {
                    json!(x.as_bool().ok_or("invalid_projection")?)
                }
                _ if x.is_null() && ["slot", "block_height"].contains(&k) => Value::Null,
                _ => json!(x.as_u64().ok_or("invalid_projection")?),
            };
            out.insert(k.into(), safe);
        }
    }
    Ok(Value::Object(out))
}
fn comparison_time(v: &Value) -> &str {
    v.get("observation_completed_at_utc")
        .and_then(Value::as_str)
        .unwrap_or_else(|| v["timestamp_utc"].as_str().unwrap())
}
fn validate_observation(v: &Value) -> R<()> {
    let fields = [
        "observation_started_at_utc",
        "observation_completed_at_utc",
        "record_created_at_utc",
        "source",
    ];
    if !fields.iter().any(|k| v.get(*k).is_some()) {
        return Ok(());
    }
    if fields.iter().any(|k| v.get(*k).is_none()) {
        return Err("incomplete_observation_metadata");
    }
    let created = v["record_created_at_utc"]
        .as_str()
        .ok_or("invalid_observation_metadata")?;
    if !timestamp(created) || v["timestamp_utc"] != created {
        return Err("invalid_observation_metadata");
    }
    let start = &v["observation_started_at_utc"];
    let end = &v["observation_completed_at_utc"];
    for t in [start, end] {
        if !t.is_null() && !t.as_str().is_some_and(timestamp) {
            return Err("invalid_observation_metadata");
        }
    }
    if !end.is_null() && start.is_null() {
        return Err("invalid_observation_metadata");
    }
    if start.as_str().is_some_and(|t| t > created)
        || end.as_str().is_some_and(|t| t > created)
        || start.as_str().zip(end.as_str()).is_some_and(|(a, b)| a > b)
    {
        return Err("invalid_observation_metadata");
    }
    if ["success", "not_found"].contains(&v["status"].as_str().unwrap_or(""))
        && (start.is_null() || end.is_null())
    {
        return Err("incomplete_observation_metadata");
    }
    let source = &v["source"];
    if v["network"] != "devnet"
        || source["kind"] != "configured_rpc"
        || source["endpoint"] != "http://devnet.rialo.io:4100"
        || !["sdk", "bounded_http"].contains(&source["transport"].as_str().unwrap_or(""))
    {
        return Err("invalid_observation_source");
    }
    let method = match v["operation"].as_str() {
        Some("account_lookup") => "getAccountInfo",
        Some("transaction_status") => "getSignatureStatuses",
        _ => return Err("invalid_observation_source"),
    };
    if source["rpc_method"] != method {
        return Err("invalid_observation_source");
    }
    Ok(())
}
fn parse(bytes: &[u8]) -> R<Value> {
    depth(bytes)?;
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "invalid_json")?;
    if !numbers(&v) {
        return Err("invalid_number");
    }
    if v["schema_version"] != SCHEMA_VERSION {
        return Err("unsupported_schema");
    }
    if !["devnet", "testnet"].contains(&v["network"].as_str().unwrap_or("")) {
        return Err("invalid_network");
    }
    if ![
        "balance_status_snapshot",
        "transaction_status",
        "account_lookup",
        "transaction_history",
        "transaction_detail",
    ]
    .contains(&v["operation"].as_str().unwrap_or(""))
    {
        return Err("invalid_operation");
    }
    if !timestamp(v["timestamp_utc"].as_str().unwrap_or("")) {
        return Err("invalid_timestamp");
    }
    let status = v["status"].as_str().ok_or("invalid_status")?;
    if ![
        "success",
        "not_found",
        "rpc_error",
        "http_error",
        "timeout",
        "blocked",
        "unverified",
    ]
    .contains(&status)
    {
        return Err("invalid_status");
    }
    if !v.get("context").is_some_and(|x| {
        x.is_object()
            && x.get("slot")
                .is_some_and(|s| s.is_null() || s.as_u64().is_some())
    }) || v.get("data").is_none()
        || v.get("error").is_none()
    {
        return Err("missing_fields");
    }
    if ["success", "not_found"].contains(&status) {
        if !v["error"].is_null() || !v["data"].is_object() {
            return Err("inconsistent_result");
        }
    } else if status == "unverified" {
        if !v["error"].is_null() || !v["data"].is_null() {
            return Err("inconsistent_result");
        }
    } else if !v["error"].is_object() || !v["data"].is_null() {
        return Err("inconsistent_result");
    }
    if status == "not_found"
        && (v["operation"] != "account_lookup" || v["data"].get("account") != Some(&Value::Null))
    {
        return Err("inconsistent_absence");
    }
    validate_observation(&v)?;
    Ok(v)
}
fn add(
    map: &mut BTreeMap<String, Vec<Point>>,
    v: &Value,
    id: String,
    fields: Value,
    count: &mut usize,
) -> R<()> {
    *count += 1;
    if *count > MAX_POINTS {
        return Err("point_limit");
    }
    let mut key = format!(
        "{}|{}|{}",
        v["network"].as_str().unwrap(),
        v["operation"].as_str().unwrap(),
        id
    );
    if v.get("observation_completed_at_utc")
        .is_some_and(Value::is_string)
    {
        key.push_str("|observation_completed_at_utc");
    }
    map.entry(key).or_default().push(Point {
        time: comparison_time(v).into(),
        fields: json!({"status":v["status"],"context_slot":v["context"]["slot"],"observed":fields}),
    });
    Ok(())
}
pub fn analyze(paths: &[String]) -> R<Value> {
    analyze_inner(paths, false)
}
pub fn analyze_with_details(paths: &[String]) -> R<Value> {
    analyze_inner(paths, true)
}
fn analyze_inner(paths: &[String], details: bool) -> R<Value> {
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err("file_count_limit");
    }
    let (mut total, mut count, mut duplicates, mut points) = (0usize, 0usize, 0usize, 0usize);
    let mut seen = BTreeSet::new();
    let mut statuses = BTreeMap::<String, usize>::new();
    let mut networks = BTreeMap::<String, usize>::new();
    let mut operations = BTreeMap::<String, usize>::new();
    let mut times = vec![];
    let mut groups = BTreeMap::<String, Vec<Point>>::new();
    let mut unavailable = 0;
    for path in paths {
        let p = Path::new(path);
        if p.extension().and_then(|s| s.to_str()) != Some("json")
            || p.components().any(|c| {
                let s = c.as_os_str().to_string_lossy().to_lowercase();
                [
                    "wallet",
                    "secret",
                    "credential",
                    "token",
                    "keypair",
                    ".aws",
                    ".ssh",
                    ".env",
                ]
                .iter()
                .any(|x| s.contains(x))
            })
        {
            return Err("disallowed_input_path");
        }
        for ancestor in p.ancestors().filter(|a| !a.as_os_str().is_empty()) {
            if std::fs::symlink_metadata(ancestor)
                .map_err(|_| "read_failed")?
                .file_type()
                .is_symlink()
            {
                return Err("regular_file_required");
            }
        }
        let meta = std::fs::symlink_metadata(p).map_err(|_| "read_failed")?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("regular_file_required");
        }
        if meta.len() > MAX_JSON_BYTES as u64 {
            return Err("file_size_limit");
        }
        let f = File::open(p).map_err(|_| "read_failed")?;
        if !f.metadata().map_err(|_| "read_failed")?.is_file() {
            return Err("regular_file_required");
        }
        let mut bytes = vec![];
        f.take((MAX_JSON_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "read_failed")?;
        if bytes.len() > MAX_JSON_BYTES {
            return Err("file_size_limit");
        }
        total += bytes.len();
        if total > MAX_TOTAL {
            return Err("total_size_limit");
        }
        let v = parse(&bytes)?;
        drop(bytes);
        let digest = Sha256::digest(serde_json::to_vec(&v).map_err(|_| "invalid_json")?);
        if !seen.insert(digest.to_vec()) {
            duplicates += 1;
            continue;
        }
        count += 1;
        let status = v["status"].as_str().unwrap();
        *statuses.entry(status.into()).or_default() += 1;
        *networks
            .entry(v["network"].as_str().unwrap().into())
            .or_default() += 1;
        *operations
            .entry(v["operation"].as_str().unwrap().into())
            .or_default() += 1;
        times.push(comparison_time(&v).to_owned());
        if !["success", "not_found"].contains(&status) {
            unavailable += 1;
            continue;
        }
        let d = &v["data"];
        match v["operation"].as_str().unwrap() {
            "account_lookup" | "balance_status_snapshot" => {
                if d.get("address").is_none() {
                    unavailable += 1;
                    continue;
                }
                let id = identifier(&d["address"], false)?;
                let fields = if v["operation"] == "account_lookup" {
                    if status == "not_found" {
                        Value::Null
                    } else {
                        if !d["account"].is_object() {
                            return Err("invalid_projection");
                        }
                        let mut projection = selected(
                            &d["account"],
                            &[
                                "owner",
                                "executable",
                                "balance_kelvin",
                                "rent_epoch",
                                "space",
                            ],
                        )?;
                        if let Some(data) = d["account"].get("data") {
                            if data["encoding"] != "base64" {
                                return Err("invalid_projection");
                            }
                            projection["data"] = selected(
                                data,
                                &["returned_bytes", "requested_max_bytes", "requested_offset"],
                            )?;
                        }
                        projection
                    }
                } else {
                    selected(d, &["balance_kelvin"])?
                };
                add(&mut groups, &v, id, fields, &mut points)?;
            }
            "transaction_status" => {
                let Some(rows) = d["transactions"].as_array() else {
                    return Err("invalid_projection");
                };
                if rows.len() > 256 {
                    return Err("entry_limit");
                }
                if rows.is_empty() {
                    unavailable += 1;
                }
                let mut ids = BTreeSet::new();
                for row in rows {
                    let id = identifier(&row["signature"], true)?;
                    if !ids.insert(id.clone()) {
                        return Err("duplicate_target");
                    }
                    add(
                        &mut groups,
                        &v,
                        id,
                        selected(row, &["slot", "executed", "error_present", "execution"])?,
                        &mut points,
                    )?;
                }
            }
            "transaction_detail" => {
                if d.get("signature").is_none() {
                    unavailable += 1;
                } else {
                    add(
                        &mut groups,
                        &v,
                        identifier(&d["signature"], true)?,
                        selected(d, &["block_height", "error_present"])?,
                        &mut points,
                    )?;
                }
            }
            _ => unavailable += 1,
        }
    }
    times.sort();
    let mut comparisons = vec![];
    for (target, mut rows) in groups {
        rows.sort_by(|a, b| a.time.cmp(&b.time));
        let mut frequency = BTreeMap::new();
        for row in &rows {
            *frequency.entry(row.time.clone()).or_insert(0usize) += 1;
        }
        if rows.len() < 2 {
            comparisons
                .push(json!({"target":target,"comparable":false,"reason":"single_observation"}));
            continue;
        }
        for pair in rows.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if frequency[&a.time] > 1 || frequency[&b.time] > 1 {
                comparisons.push(json!({"target":target,"comparable":false,"reason":"same_timestamp","timestamp_utc":a.time}));
            } else {
                let mut comparison = json!({"target":target,"comparable":true,"from":a.time,"to":b.time,"changed":a.fields!=b.fields,"before":a.fields,"after":b.fields});
                if details {
                    comparison["temporal"] = temporal_details(
                        a,
                        b,
                        target.ends_with("|observation_completed_at_utc"),
                        target.contains("|account_lookup|"),
                    )?;
                }
                comparisons.push(comparison);
            }
        }
    }
    let mut result = json!({"analysis_schema_version":"1.0","input_files":paths.len(),"total_reports":count,"duplicates_skipped":duplicates,"success":statuses.get("success").copied().unwrap_or(0),"not_found":statuses.get("not_found").copied().unwrap_or(0),"errors":(["rpc_error","http_error","timeout"].iter().map(|s|statuses.get(*s).copied().unwrap_or(0)).sum::<usize>()),"blocked":statuses.get("blocked").copied().unwrap_or(0),"unverified":statuses.get("unverified").copied().unwrap_or(0),"by_status":statuses,"by_network":networks,"by_operation":operations,"first_observed_utc":times.first(),"last_observed_utc":times.last(),"incomparable_reports":unavailable,"incomparable_reason":"missing_target_or_non_observation_or_history_without_address","comparisons":comparisons});
    if details {
        result["temporal_details_version"] = json!("1.0");
    }
    Ok(result)
}
pub fn render(v: &Value, json_mode: bool) -> R<String> {
    let output = if json_mode {
        {
            struct Capped(Vec<u8>);
            impl std::io::Write for Capped {
                fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                    if bytes.len() > MAX_JSON_BYTES - self.0.len() {
                        return Err(std::io::Error::other("output_size_limit"));
                    }
                    self.0.extend_from_slice(bytes);
                    Ok(bytes.len())
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            let mut writer = Capped(Vec::new());
            serde_json::to_writer_pretty(&mut writer, v).map_err(|_| "output_size_limit")?;
            String::from_utf8(writer.0).map_err(|_| "output_error")?
        }
    } else {
        let mut s = format!(
            "Offline READ ONLY reports\nTotal: {} (duplicates skipped: {})\nSuccess: {}  not_found: {}  Errors: {}  blocked: {}  unverified: {}\nNetwork: {}\nOperation: {}\nFirst: {}  Last: {}\n比較不可 reports: {}\n",
            v["total_reports"],
            v["duplicates_skipped"],
            v["success"],
            v["not_found"],
            v["errors"],
            v["blocked"],
            v["unverified"],
            v["by_network"],
            v["by_operation"],
            v["first_observed_utc"],
            v["last_observed_utc"],
            v["incomparable_reports"]
        );
        for c in v["comparisons"].as_array().unwrap() {
            let line = serde_json::to_string(c).map_err(|_| "output_error")?;
            if line.len() + 1 > MAX_JSON_BYTES.saturating_sub(s.len()) {
                return Err("output_size_limit");
            }
            s.push_str(&line);
            s.push('\n');
        }
        s
    };
    if output.len() > MAX_JSON_BYTES {
        return Err("output_size_limit");
    }
    Ok(output)
}

fn timestamp_millis(s: &str) -> R<u64> {
    if !timestamp(s) {
        return Err("invalid_timestamp");
    }
    let n = |a: usize, b: usize| s[a..b].parse::<u64>().map_err(|_| "invalid_timestamp");
    let year = n(0, 4)?;
    let month = n(5, 7)?;
    let leap = |y: u64| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days_before = |y: u64| {
        let z = y - 1;
        365 * z + z / 4 - z / 100 + z / 400
    };
    let mut days = days_before(year) - days_before(1970);
    for m in 1..month {
        days += match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if leap(year) {
                    29
                } else {
                    28
                }
            }
            _ => return Err("invalid_timestamp"),
        };
    }
    days += n(8, 10)? - 1;
    Ok((((days * 24 + n(11, 13)?) * 60 + n(14, 16)?) * 60 + n(17, 19)?) * 1000 + n(20, 23)?)
}
fn temporal_details(a: &Point, b: &Point, observed: bool, account: bool) -> R<Value> {
    let interval = timestamp_millis(&b.time)?
        .checked_sub(timestamp_millis(&a.time)?)
        .ok_or("invalid_time_order")?;
    let (direction, magnitude) = match (
        a.fields["context_slot"].as_u64(),
        b.fields["context_slot"].as_u64(),
    ) {
        (Some(x), Some(y)) if y > x => ("increased", Some(y - x)),
        (Some(x), Some(y)) if y < x => ("decreased", Some(x - y)),
        (Some(_), Some(_)) => ("unchanged", Some(0)),
        _ => ("unknown", None),
    };
    let presence = |p: &Point| {
        if !account {
            "not_applicable"
        } else {
            match p.fields["status"].as_str() {
                Some("not_found") => "absent",
                Some("success") if p.fields["observed"].is_object() => "present",
                _ => "unknown",
            }
        }
    };
    let evidence = |p: &Point| match p.fields["status"].as_str() {
        Some("success" | "not_found") => "successful_response",
        _ => "unverified",
    };
    Ok(
        json!({"time_basis":if observed{"observation_completed_at_utc"}else{"timestamp_utc_report_creation"},"interval_ms":interval,"slot_change":{"direction":direction,"magnitude":magnitude},"status_changed":a.fields["status"]!=b.fields["status"],"account_presence_before":presence(a),"account_presence_after":presence(b),"query_outcome_before":evidence(a),"query_outcome_after":evidence(b),"inference":"field_differences_only"}),
    )
}
