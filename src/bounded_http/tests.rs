//! Loopback-only HTTP fixtures; no live RPC, credentials, signing or transactions.
use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize},
    },
    thread,
    time::Instant,
};

struct Server {
    url: Url,
    stop: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
    request: Arc<Mutex<Option<Value>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("fixture server panic");
        }
    }
}
fn serve(handler: impl FnOnce(&mut TcpStream) + Send + 'static) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let request = Arc::new(Mutex::new(None));
    let (s, c, r) = (stop.clone(), calls.clone(), request.clone());
    let worker = thread::spawn(move || {
        let start = Instant::now();
        while !s.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(3) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    c.fetch_add(1, Ordering::SeqCst);
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_millis(300)))
                        .unwrap();
                    let mut headers = vec![];
                    let mut byte = [0];
                    while !headers.ends_with(b"\r\n\r\n") {
                        assert!(headers.len() < 16384);
                        stream.read_exact(&mut byte).unwrap();
                        headers.push(byte[0]);
                    }
                    let headers = String::from_utf8(headers).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|l| {
                            l.split_once(':')
                                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    assert!(length < 16384);
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).unwrap();
                    *r.lock().unwrap() = Some(serde_json::from_slice(&body).unwrap());
                    handler(&mut stream);
                    return;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(e) => panic!("fixture accept: {e}"),
            }
        }
    });
    Server {
        url,
        stop,
        calls,
        request,
        worker: Some(worker),
    }
}
fn fixed(bytes: Vec<u8>) -> Server {
    serve(move |s| {
        let _ = s.write_all(&bytes);
    })
}
fn frame(status: u16, body: &[u8], length: Option<usize>) -> Vec<u8> {
    let mut headers = format!("HTTP/1.1 {status} Fixture\r\nConnection: close\r\n");
    if let Some(length) = length {
        headers += &format!("Content-Length: {length}\r\n");
    }
    let mut bytes = headers.into_bytes();
    bytes.extend_from_slice(b"\r\n");
    bytes.extend_from_slice(body);
    bytes
}
fn body(result: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"result":result})).unwrap()
}
fn result(present: bool) -> Value {
    json!({"context":{"slot":42,"api_version":"synthetic"},"value":if present { json!({"kelvin":100,"owner":"11111111111111111111111111111111","executable":true,"data":["bW9jaw==","base64"],"rentEpoch":0,"space":4}) } else {Value::Null}})
}
fn transport(server: &Server, limit: usize) -> BoundedHttp {
    // Private test-only builder; no runtime endpoint override is exported.
    assert_eq!(server.url.host_str(), Some("127.0.0.1"));
    BoundedHttp::build(
        server.url.clone(),
        Limits {
            body_bytes: limit,
            ..Limits::default()
        },
    )
    .unwrap()
}
async fn read(client: &BoundedHttp) -> Result<Value, AccountError> {
    client
        .rpc(
            "getAccountInfo",
            json!([{"version":0,"address":"11111111111111111111111111111111"}]),
        )
        .await
}

#[tokio::test]
async fn normal_small_response_and_wire_request() {
    let bytes = body(result(false));
    let server = fixed(frame(200, &bytes, Some(bytes.len())));
    let client = transport(&server, 1024);
    assert_eq!(read(&client).await.unwrap(), result(false));
    let request = server.request.lock().unwrap();
    assert_eq!(request.as_ref().unwrap()["method"], "getAccountInfo");
    assert_eq!(request.as_ref().unwrap()["jsonrpc"], "2.0");
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn numeric_receipt_matches_validated_http_body_without_payload() {
    let bytes = body(result(false));
    let expected_len = bytes.len();
    let server = fixed(frame(200, &bytes, Some(bytes.len())));
    let client = transport(&server, 1024);
    assert!(client.receipt().is_none());
    assert_eq!(read(&client).await.unwrap(), result(false));
    let receipt = serde_json::to_value(client.receipt().unwrap()).unwrap();
    assert_eq!(
        receipt,
        json!({"http_status":200,"body_bytes":expected_len,"body_limit":1024,"request_id":1,"jsonrpc_version":"2.0"})
    );
    assert!(!receipt.to_string().contains("result"));
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn response_exactly_at_limit_is_accepted() {
    let mut bytes = body(result(false));
    bytes.resize(1024, b' ');
    let server = fixed(frame(200, &bytes, Some(bytes.len())));
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap(),
        result(false)
    );
}
#[tokio::test]
async fn declared_one_byte_over_limit_is_rejected_before_body_read() {
    let server = serve(|s| {
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1025\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut b = [0];
        assert!(matches!(s.read(&mut b), Ok(0) | Err(_)));
    });
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap_err(),
        AccountError::ResponseTooLarge { limit: 1024 }
    );
}
#[tokio::test]
async fn no_content_length_is_counted_not_assumed_safe() {
    let bytes = body(result(false));
    let server = fixed(frame(200, &bytes, None));
    assert!(read(&transport(&server, 1024)).await.is_ok());
    let server = fixed(frame(200, &vec![b' '; 1025], None));
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap_err(),
        AccountError::ResponseTooLarge { limit: 1024 }
    );
}
#[tokio::test]
async fn understated_length_truncating_json_is_rejected() {
    let bytes = body(result(false));
    let server = fixed(frame(200, &bytes, Some(bytes.len() - 1)));
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap_err(),
        AccountError::MalformedResponse
    );
}
#[tokio::test]
async fn valid_declared_message_does_not_parse_trailing_connection_bytes() {
    let bytes = body(result(false));
    let mut wire = frame(200, &bytes, Some(bytes.len()));
    wire.extend_from_slice(b"UNTRUSTED-TRAILING-BYTES");
    let server = fixed(wire);
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap(),
        result(false)
    );
    // Content-Length defines the HTTP message. No pool reuse or following response.
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn overstated_length_and_early_eof_is_transport_error() {
    let bytes = body(result(false));
    let server = fixed(frame(200, &bytes, Some(bytes.len() + 4)));
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap_err(),
        AccountError::Transport
    );
}
#[tokio::test]
async fn invalid_content_length_fails_closed() {
    let server = fixed(
        b"HTTP/1.1 200 OK\r\nContent-Length: not-a-number\r\nConnection: close\r\n\r\n{}".to_vec(),
    );
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap_err(),
        AccountError::Transport
    );
}
#[tokio::test]
async fn chunked_exact_limit_is_accepted() {
    let mut bytes = body(result(false));
    bytes.resize(1024, b' ');
    let server = serve(move |s| {
        s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n")
            .unwrap();
        for chunk in bytes.chunks(64) {
            write!(s, "{:x}\r\n", chunk.len()).unwrap();
            s.write_all(chunk).unwrap();
            s.write_all(b"\r\n").unwrap();
        }
        s.write_all(b"0\r\n\r\n").unwrap();
    });
    assert!(read(&transport(&server, 1024)).await.is_ok());
}
#[tokio::test]
async fn chunked_overflow_actually_closes_before_full_body_is_sent() {
    let sent = Arc::new(AtomicUsize::new(0));
    let disconnected = Arc::new(AtomicBool::new(false));
    let (count, closed) = (sent.clone(), disconnected.clone());
    let server = serve(move |s| {
        s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n")
            .unwrap();
        let chunk = [b' '; 512];
        for _ in 0..131072 {
            // 64MiB planned, generated incrementally, never allocated whole.
            if write!(s, "200\r\n")
                .and_then(|_| s.write_all(&chunk))
                .and_then(|_| s.write_all(b"\r\n"))
                .is_err()
            {
                closed.store(true, Ordering::SeqCst);
                return;
            }
            count.fetch_add(512, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(1));
        }
        panic!("client consumed entire oversized body");
    });
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap_err(),
        AccountError::ResponseTooLarge { limit: 1024 }
    );
    drop(server);
    assert!(disconnected.load(Ordering::SeqCst));
    assert!(sent.load(Ordering::SeqCst) < 64 * 1024 * 1024);
}
#[tokio::test]
async fn invalid_json_is_not_rpc_success() {
    let server = fixed(frame(200, b"not-json", Some(8)));
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap_err(),
        AccountError::MalformedResponse
    );
}
#[tokio::test]
async fn http_4xx_and_5xx_are_distinct_from_rpc_errors() {
    for status in [400, 503] {
        let bytes =
            serde_json::to_vec(&json!({"error":{"code":-32001,"message":"sensitive-fixture"}}))
                .unwrap();
        let server = fixed(frame(status, &bytes, Some(bytes.len())));
        let error = read(&transport(&server, 1024)).await.unwrap_err();
        assert_eq!(
            error,
            AccountError::Http {
                status,
                code: -32001
            }
        );
        assert!(!format!("{error:?}").contains("sensitive"));
    }
}
#[tokio::test]
async fn oversized_error_body_obeys_the_same_limit() {
    let server = fixed(frame(503, &vec![b' '; 1025], None));
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap_err(),
        AccountError::ResponseTooLarge { limit: 1024 }
    );
}
#[tokio::test]
async fn json_rpc_error_keeps_only_numeric_code() {
    let bytes=serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"sensitive-fixture","data":"sensitive"}})).unwrap();
    let server = fixed(frame(200, &bytes, Some(bytes.len())));
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap_err(),
        AccountError::Rpc { code: -32602 }
    );
}
#[tokio::test]
async fn wrong_id_or_version_and_ambiguous_envelopes_are_rejected() {
    for envelope in [
        json!({"jsonrpc":"2.0","id":2,"result":{}}),
        json!({"jsonrpc":"1.0","id":1,"result":{}}),
        json!({"jsonrpc":"2.0","id":"1","result":{}}),
        json!({"jsonrpc":"2.0","id":1}),
        json!({"jsonrpc":"2.0","id":1,"result":{},"error":null}),
        json!({"jsonrpc":"2.0","id":1,"error":{"code":"bad","message":"x"}}),
    ] {
        let bytes = serde_json::to_vec(&envelope).unwrap();
        let server = fixed(frame(200, &bytes, Some(bytes.len())));
        assert_eq!(
            read(&transport(&server, 1024)).await.unwrap_err(),
            AccountError::InvalidRpcEnvelope
        );
    }
}
#[tokio::test]
async fn connection_failure_is_not_retried() {
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/", socket.local_addr().unwrap())).unwrap();
    drop(socket);
    let client = BoundedHttp::build(url, Limits::default()).unwrap();
    assert_eq!(read(&client).await.unwrap_err(), AccountError::Transport);
}
#[tokio::test]
async fn reading_timeout_cancels_delayed_response() {
    let server = serve(|_| thread::sleep(Duration::from_millis(200)));
    let client = BoundedHttp::build(
        server.url.clone(),
        Limits {
            read: Duration::from_millis(50),
            total: Duration::from_secs(1),
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(read(&client).await.unwrap_err(), AccountError::Timeout);
}
#[tokio::test]
async fn total_timeout_stops_a_continuously_trickling_body() {
    let closed = Arc::new(AtomicBool::new(false));
    let c = closed.clone();
    let server = serve(move |s| {
        s.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
            .unwrap();
        for _ in 0..100 {
            if s.write_all(b" ").is_err() {
                c.store(true, Ordering::SeqCst);
                return;
            }
            thread::sleep(Duration::from_millis(15));
        }
    });
    let client = BoundedHttp::build(
        server.url.clone(),
        Limits {
            read: Duration::from_millis(100),
            total: Duration::from_millis(80),
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(read(&client).await.unwrap_err(), AccountError::Timeout);
    // Connection cleanup is asynchronous. Do not block the runtime in Server::drop
    // (thread join) before its HTTP driver has had an opportunity to close.
    drop(client);
    for _ in 0..50 {
        if closed.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(server);
    assert!(closed.load(Ordering::SeqCst));
}
#[tokio::test]
async fn forbidden_methods_are_rejected_before_connecting() {
    let server = serve(|_| panic!("forbidden request reached server"));
    let client = transport(&server, 1024);
    for method in [
        "sendTransaction",
        "requestAirdrop",
        "start_track",
        "getBalance",
        "",
    ] {
        assert_eq!(
            client.rpc(method, json!([])).await.unwrap_err(),
            AccountError::MethodNotAllowed
        );
    }
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn redirects_are_rejected_and_destination_is_not_contacted() {
    let target = serve(|_| panic!("redirect followed"));
    let location = target.url.to_string();
    let server = serve(move |s| {
        let _ = write!(
            s,
            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
    });
    assert_eq!(
        read(&transport(&server, 1024)).await.unwrap_err(),
        AccountError::Redirect { status: 302 }
    );
    assert_eq!(target.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn compressed_expansion_is_prevented_by_rejecting_compressed_responses() {
    let gzip = include_bytes!("../../tests/fixtures/http/oversized.json.gz");
    assert!(gzip.len() < DEFAULT_BODY_LIMIT);
    let mut bytes=format!("HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",gzip.len()).into_bytes();
    bytes.extend_from_slice(gzip);
    let server = fixed(bytes);
    assert_eq!(
        read(&transport(&server, DEFAULT_BODY_LIMIT))
            .await
            .unwrap_err(),
        AccountError::UnsupportedContentEncoding
    );
}
#[tokio::test]
async fn absent_account_reuses_existing_classification() {
    let bytes = body(result(false));
    let server = fixed(frame(200, &bytes, Some(bytes.len())));
    let observation = account_readonly::query(
        &transport(&server, 1024),
        "11111111111111111111111111111111",
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    assert_eq!(observation.report.status, "not_found");
    assert!(observation.report.account.is_none());
}
struct TypedFixture;
impl AccountRpc for TypedFixture {
    async fn account(&self, _: &Pubkey) -> Result<Value, AccountError> {
        Ok(result(true))
    }
}
#[tokio::test]
async fn present_account_report_and_data_match_existing_path() {
    let bytes = body(result(true));
    let server = fixed(frame(200, &bytes, Some(bytes.len())));
    let address = "11111111111111111111111111111111";
    let actual =
        account_readonly::query(&transport(&server, 1024), address, Duration::from_secs(2))
            .await
            .unwrap();
    let existing = account_readonly::query(&TypedFixture, address, Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&actual.report).unwrap(),
        serde_json::to_value(existing.report).unwrap()
    );
    assert_eq!(actual.data(), b"mock");
    let request = server.request.lock().unwrap();
    assert_eq!(
        request.as_ref().unwrap()["params"][0]["config"]["dataSlice"]["length"],
        4096
    );
}
#[test]
fn configuration_is_finite_and_public_endpoint_is_fixed() {
    for limit in [0, MAX_BODY_LIMIT + 1] {
        assert!(matches!(
            BoundedHttp::devnet(Limits {
                body_bytes: limit,
                ..Limits::default()
            }),
            Err(AccountError::InvalidInput)
        ));
    }
    assert!(matches!(
        BoundedHttp::devnet(Limits {
            read: Duration::ZERO,
            ..Limits::default()
        }),
        Err(AccountError::InvalidInput)
    ));
    let client = BoundedHttp::devnet(Limits::default()).unwrap();
    assert_eq!(client.endpoint.as_str(), format!("{URL_DEVNET}/"));
}
#[test]
fn cli_transport_flags_are_explicit_and_default_sdk_is_unchanged() {
    let base = vec![
        "--address".into(),
        "11111111111111111111111111111111".into(),
    ];
    let (options, selection) = parse_cli(&base).unwrap();
    assert!(!selection.bounded);
    assert!(!options.execute);
    let mut enabled = base.clone();
    enabled.extend([
        "--bounded-http".into(),
        "--max-body-bytes".into(),
        "1024".into(),
    ]);
    let (_, selection) = parse_cli(&enabled).unwrap();
    assert!(selection.bounded);
    assert_eq!(selection.body_bytes, 1024);
    for tail in [
        vec!["--max-body-bytes", "1024"],
        vec!["--bounded-http", "--bounded-http"],
        vec!["--bounded-http", "--max-body-bytes", "0"],
        vec!["--bounded-http", "--max-body-bytes", "8388609"],
        vec!["--bounded-http", "--rpc", "http://localhost"],
    ] {
        let mut args = base.clone();
        args.extend(tail.into_iter().map(str::to_string));
        assert!(parse_cli(&args).is_err());
    }
}
