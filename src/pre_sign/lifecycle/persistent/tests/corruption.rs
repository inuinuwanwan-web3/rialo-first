use super::super::diagnostics::Issue;
use super::*;
async fn fixture() -> (Temp, Vec<u8>) {
    let dir = Temp::new();
    let clock = Time(Cell::new(10000));
    let mut store = StateStore::initialize(&dir.0).unwrap();
    drop(
        store
            .register("id", checked(&clock, 2).await, &clock)
            .unwrap(),
    );
    release_test_store(store);
    let bytes = fs::read(dir.0.join(FILE_NAME)).unwrap();
    (dir, bytes)
}
fn rejected(dir: &Temp, expected: Issue) {
    let path = dir.0.join(FILE_NAME);
    let before = fs::read(&path).ok();
    let diagnostic = StateStore::open_diagnosed(&dir.0).err().unwrap();
    assert_eq!(diagnostic.issue, expected);
    assert!(!format!("{diagnostic:?} {diagnostic}").contains("SENSITIVE_SENTINEL"));
    assert!(StateStore::open(&dir.0).is_err());
    assert_eq!(fs::read(path).ok(), before);
}
fn append_record(bytes: &mut Vec<u8>, mutator: impl FnOnce(&mut Record)) {
    let entry: Entry = serde_json::from_slice(&bytes[HEADER.len()..]).unwrap();
    let mut record = entry.record;
    mutator(&mut record);
    let previous = entry.checksum;
    let checksum = digest(&record, previous).unwrap();
    bytes.extend(
        serde_json::to_vec(&Entry {
            record,
            previous,
            checksum,
        })
        .unwrap(),
    );
    bytes.push(b'\n');
}
#[tokio::test]
async fn malformed_json_redacted() {
    let (d, _) = fixture().await;
    fs::write(
        d.0.join(FILE_NAME),
        [HEADER, b"{invalid SENSITIVE_SENTINEL}\n"].concat(),
    )
    .unwrap();
    rejected(&d, Issue::JsonSyntax);
}
#[tokio::test]
async fn truncated_json_preserved() {
    let (d, mut b) = fixture().await;
    b.truncate(b.len() - 10);
    fs::write(d.0.join(FILE_NAME), b).unwrap();
    rejected(&d, Issue::Truncated);
}
#[tokio::test]
async fn missing_fields_rejected() {
    let (d, _) = fixture().await;
    fs::write(d.0.join(FILE_NAME), [HEADER, b"{}\n"].concat()).unwrap();
    rejected(&d, Issue::Fields);
}
#[tokio::test]
async fn unknown_field_never_disclosed() {
    let (d, _) = fixture().await;
    fs::write(
        d.0.join(FILE_NAME),
        [HEADER, b"{\"SENSITIVE_SENTINEL\":1}\n"].concat(),
    )
    .unwrap();
    rejected(&d, Issue::Fields);
}
#[tokio::test]
async fn invalid_transition_with_valid_checksum() {
    let (d, mut b) = fixture().await;
    append_record(&mut b, |r| r.state = StoredState::Signed);
    fs::write(d.0.join(FILE_NAME), b).unwrap();
    rejected(&d, Issue::InvalidState);
}
#[tokio::test]
async fn duplicate_registration_with_valid_checksum() {
    let (d, mut b) = fixture().await;
    append_record(&mut b, |_| {});
    fs::write(d.0.join(FILE_NAME), b).unwrap();
    rejected(&d, Issue::DuplicateRegistration);
}
#[tokio::test]
async fn unknown_schema_rejected() {
    let (d, _) = fixture().await;
    fs::write(d.0.join(FILE_NAME), b"RIALO-SIGNATURE-STATE-999\n").unwrap();
    rejected(&d, Issue::HeaderOrSchema);
}
#[tokio::test]
async fn unknown_protocol_rejected() {
    let (d, mut b) = fixture().await;
    append_record(&mut b, |r| r.protocol_version = 99);
    fs::write(d.0.join(FILE_NAME), b).unwrap();
    rejected(&d, Issue::ProtocolVersion);
}
#[tokio::test]
async fn file_permissions_rejected() {
    let (d, _) = fixture().await;
    fs::set_permissions(d.0.join(FILE_NAME), fs::Permissions::from_mode(0o644)).unwrap();
    rejected(&d, Issue::PermissionsOrType);
}
#[tokio::test]
async fn directory_permissions_rejected() {
    let (d, _) = fixture().await;
    fs::set_permissions(&d.0, fs::Permissions::from_mode(0o755)).unwrap();
    rejected(&d, Issue::PermissionsOrType);
}
#[tokio::test]
async fn missing_file_not_initialized() {
    let (d, b) = fixture().await;
    let backup = d.0.join("preserved");
    fs::rename(d.0.join(FILE_NAME), &backup).unwrap();
    rejected(&d, Issue::Missing);
    assert_eq!(fs::read(backup).unwrap(), b);
}
#[tokio::test]
async fn matching_temp_file_stops() {
    let (d, b) = fixture().await;
    fs::write(d.0.join("requests.tmp"), &b).unwrap();
    rejected(&d, Issue::UnexpectedArtifact);
    assert_eq!(fs::read(d.0.join("requests.tmp")).unwrap(), b);
}
#[tokio::test]
async fn conflicting_temp_file_stops() {
    let (d, _) = fixture().await;
    fs::write(d.0.join("requests.tmp"), b"SENSITIVE_SENTINEL").unwrap();
    rejected(&d, Issue::UnexpectedArtifact);
    assert_eq!(
        fs::read(d.0.join("requests.tmp")).unwrap(),
        b"SENSITIVE_SENTINEL"
    );
}
#[tokio::test]
async fn normal_state_preserved_and_id_blocked() {
    let (d, b) = fixture().await;
    let clock = Time(Cell::new(10000));
    let mut s = StateStore::open_diagnosed(&d.0).unwrap();
    assert!(s.register("id", checked(&clock, 3).await, &clock).is_err());
    assert_eq!(fs::read(d.0.join(FILE_NAME)).unwrap(), b);
}
#[test]
fn absent_directory_not_auto_initialized() {
    let d = Temp::new();
    rejected(&d, Issue::Missing);
    assert!(!d.0.exists());
}
