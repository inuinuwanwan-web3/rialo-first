//! Unix local-filesystem journal. Never restores live signing sessions after restart.
use super::*;
pub mod diagnostics;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, DirBuilder, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

const HEADER: &[u8] = b"RIALO-SIGNATURE-STATE-1\n";
const MAX_JOURNAL: u64 = 16 * 1024 * 1024;
const MAX_RECORD: usize = 4096;
const FILE_NAME: &str = "requests.journal";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoredState {
    Created,
    Validating,
    Validated,
    AwaitingSignature,
    ProcessingResponse,
    Signed,
    Rejected,
    Expired,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub request_id: String,
    pub protocol_version: u16,
    pub signing_message_hash: [u8; 32],
    pub network_config_prefix: u64,
    pub created_at: i64,
    pub expires_at: i64,
    pub state: StoredState,
    pub updated_at: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    record: Record,
    previous: [u8; 32],
    checksum: [u8; 32],
}
fn digest(record: &Record, previous: [u8; 32]) -> Result<[u8; 32]> {
    let mut bytes = previous.to_vec();
    bytes.extend(serde_json::to_vec(record)?);
    Ok(signing_message_hash(&bytes))
}
fn validate_record(r: &Record, old: Option<&Record>) -> Result<()> {
    if r.request_id.is_empty()
        || r.request_id.len() > 128
        || !r
            .request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        || r.protocol_version != PROTOCOL_VERSION
        || r.created_at <= 0
        || r.expires_at <= r.created_at
        || r.updated_at < r.created_at
    {
        return Err("不正な永続レコード".into());
    }
    match old {
        None if r.state == StoredState::Created => Ok(()),
        Some(o) => {
            if r.request_id != o.request_id
                || r.protocol_version != o.protocol_version
                || r.signing_message_hash != o.signing_message_hash
                || r.network_config_prefix != o.network_config_prefix
                || r.updated_at < o.updated_at
            {
                return Err("永続レコードの識別情報変更".into());
            }
            let valid = matches!(
                (o.state, r.state),
                (
                    StoredState::Created,
                    StoredState::Validating | StoredState::Rejected | StoredState::Expired
                ) | (
                    StoredState::Validating,
                    StoredState::Validated | StoredState::Rejected | StoredState::Expired
                ) | (
                    StoredState::Validated,
                    StoredState::AwaitingSignature | StoredState::Rejected | StoredState::Expired
                ) | (
                    StoredState::AwaitingSignature,
                    StoredState::ProcessingResponse | StoredState::Rejected | StoredState::Expired
                ) | (
                    StoredState::ProcessingResponse,
                    StoredState::Signed | StoredState::Rejected | StoredState::Expired
                )
            );
            let renewed = o.state == StoredState::Validating && r.state == StoredState::Validated;
            if !valid
                || (!renewed && (r.created_at != o.created_at || r.expires_at != o.expires_at))
                || (renewed && (r.created_at < o.created_at || r.expires_at < o.expires_at))
            {
                return Err("不正な永続状態遷移".into());
            }
            Ok(())
        }
        _ => Err("初期状態が欠損".into()),
    }
}
/// Holds a non-blocking exclusive OS lock for its entire lifetime. No Clone or recovery reset.
pub struct StateStore {
    file: File,
    records: BTreeMap<String, Record>,
    tail: [u8; 32],
    poisoned: bool,
    #[cfg(test)]
    fail_state: Option<StoredState>,
}
impl StateStore {
    /// Explicit first-time initialization only. Existing directory is never overwritten.
    pub fn initialize(directory: &Path) -> Result<Self> {
        DirBuilder::new().mode(0o700).create(directory)?;
        let path = directory.join(FILE_NAME);
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.try_lock()?;
        file.write_all(HEADER)?;
        file.sync_all()?;
        File::open(directory)?.sync_all()?;
        if let Some(parent) = directory.parent() {
            File::open(parent)?.sync_all()?;
        }
        Ok(Self {
            file,
            records: BTreeMap::new(),
            tail: [0; 32],
            poisoned: false,
            #[cfg(test)]
            fail_state: None,
        })
    }
    /// Missing, empty, corrupt or inaccessible files are errors, never a new empty store.
    pub fn open(directory: &Path) -> Result<Self> {
        Self::open_diagnosed(directory).map_err(|e| Box::new(e) as _)
    }
    pub fn record(&self, request_id: &str) -> Option<&Record> {
        self.records.get(request_id)
    }
    fn append(&mut self, record: Record) -> Result<()> {
        if self.poisoned {
            return Err("保存失敗後のストアは再利用禁止".into());
        }
        validate_record(&record, self.records.get(&record.request_id))?;
        let checksum = digest(&record, self.tail)?;
        let mut bytes = serde_json::to_vec(&Entry {
            record: record.clone(),
            previous: self.tail,
            checksum,
        })?;
        bytes.push(b'\n');
        if bytes.len() > MAX_RECORD
            || self
                .file
                .metadata()?
                .len()
                .checked_add(bytes.len() as u64)
                .ok_or("容量overflow")?
                > MAX_JOURNAL
        {
            return Err("ジャーナル容量超過".into());
        }
        // Poison before any write; failure or unwind cannot authorize a retry.
        self.poisoned = true;
        #[cfg(test)]
        if self.fail_state == Some(record.state) {
            return Err("injected persistence failure".into());
        }
        #[cfg(test)]
        tests::crash::checkpoint(record.state, "before_write");
        #[cfg(test)]
        if tests::crash::selected(record.state, "partial_json") {
            self.file.write_all(&bytes[..bytes.len() / 2])?;
            tests::crash::checkpoint(record.state, "partial_json");
            unreachable!("crash child must not resume");
        }
        self.file.write_all(&bytes)?;
        #[cfg(test)]
        tests::crash::checkpoint(record.state, "before_sync");
        self.file.sync_all()?;
        #[cfg(test)]
        tests::crash::checkpoint(record.state, "after_sync");
        self.tail = checksum;
        self.records.insert(record.request_id.clone(), record);
        self.poisoned = false;
        Ok(())
    }
    /// Caller supplies the ID. ANY historical ID is refused, including unfinished requests.
    pub fn register<'a>(
        &'a mut self,
        request_id: &str,
        checked: CheckedTransfer,
        clock: &impl Clock,
    ) -> Result<PersistentLifecycle<'a>> {
        if self.poisoned || self.records.contains_key(request_id) {
            return Err("要求IDは登録済み、またはストア使用不可".into());
        }
        let now = clock.now()?;
        checked.validate_at(now)?;
        let r = checked.report();
        self.append(Record {
            request_id: request_id.into(),
            protocol_version: PROTOCOL_VERSION,
            signing_message_hash: r.signing_message_hash,
            network_config_prefix: r.network_config_prefix.into(),
            created_at: now.unix_ms,
            expires_at: r.expires_at,
            state: StoredState::Created,
            updated_at: now.unix_ms,
        })?;
        let inner = SignatureLifecycle {
            state: State::Created,
            checked: Some(checked),
            request: None,
            id: request_id.into(),
        };
        Ok(PersistentLifecycle {
            store: self,
            inner,
            blocked: false,
        })
    }
}

/// Borrow prevents concurrent use through the same store; OS lock covers other handles/processes.
pub struct PersistentLifecycle<'a> {
    store: &'a mut StateStore,
    inner: SignatureLifecycle,
    blocked: bool,
}
impl PersistentLifecycle<'_> {
    pub fn state(&self) -> State {
        self.inner.state()
    }
    fn save(&mut self, state: StoredState, clock: &impl Clock) -> Result<()> {
        let now = clock.now()?;
        let mut r = self
            .store
            .records
            .get(&self.inner.id)
            .ok_or("永続状態欠損")?
            .clone();
        r.state = state;
        r.updated_at = now.unix_ms;
        if state == StoredState::Validated {
            let request = self.inner.request.as_ref().ok_or("要求欠損")?;
            r.created_at = request.created_at;
            r.expires_at = request.expires_at;
        }
        self.store.append(r)
    }
    fn halt(&mut self) {
        self.blocked = true;
        self.inner.reject();
    }
    fn usable(&self) -> Result<()> {
        if self.blocked || self.store.poisoned {
            return Err("永続セッション再利用禁止".into());
        }
        Ok(())
    }
    pub async fn validate(&mut self, clock: &impl Clock) -> Result<()> {
        self.validate_using(clock, |c| revalidate_before_signing(c, clock))
            .await
    }
    async fn validate_using<F: std::future::Future<Output = Result<CheckedTransfer>>>(
        &mut self,
        clock: &impl Clock,
        f: impl FnOnce(CheckedTransfer) -> F,
    ) -> Result<()> {
        self.usable()?;
        if self.inner.state != State::Created {
            self.halt();
            return Err("検証状態不一致".into());
        }
        self.blocked = true;
        if let Err(e) = self.save(StoredState::Validating, clock) {
            self.halt();
            return Err(e);
        }
        let outcome = self.inner.validate_using(clock, f).await;
        let stored = match self.inner.state {
            State::Validated => StoredState::Validated,
            State::Expired => StoredState::Expired,
            _ => StoredState::Rejected,
        };
        if let Err(e) = self.save(stored, clock) {
            self.halt();
            return Err(e);
        }
        if outcome.is_err() {
            self.halt();
            return outcome;
        }
        self.blocked = false;
        Ok(())
    }
    pub fn begin_signing(&mut self, clock: &impl Clock) -> Result<SignatureRequest> {
        self.usable()?;
        self.blocked = true;
        let outcome = self.inner.begin_signing(clock);
        let stored = match self.inner.state {
            State::AwaitingSignature => StoredState::AwaitingSignature,
            State::Expired => StoredState::Expired,
            _ => StoredState::Rejected,
        };
        if let Err(e) = self.save(stored, clock) {
            self.halt();
            return Err(e);
        }
        match outcome {
            Ok(r) => {
                self.blocked = false;
                Ok(r)
            }
            Err(e) => {
                self.halt();
                Err(e)
            }
        }
    }
    pub fn accept_response(
        &mut self,
        response: SignatureResponse,
        clock: &impl Clock,
    ) -> Result<Vec<u8>> {
        self.usable()?;
        if self.inner.state != State::AwaitingSignature {
            self.halt();
            return Err("応答状態不一致".into());
        }
        self.blocked = true;
        // Durable claim BEFORE verification/assembly, so crash cannot cause another attempt.
        if let Err(e) = self.save(StoredState::ProcessingResponse, clock) {
            self.halt();
            return Err(e);
        }
        let outcome = self.inner.accept_response(response, clock);
        let stored = match self.inner.state {
            State::Signed => StoredState::Signed,
            State::Expired => StoredState::Expired,
            _ => StoredState::Rejected,
        };
        if let Err(e) = self.save(stored, clock) {
            self.halt();
            return Err(e);
        }
        if outcome.is_err() {
            self.halt();
        }
        // Remain blocked even on success; wire bytes are never returned twice.
        outcome
    }
}

#[cfg(test)]
mod tests {
    mod corruption;
    pub(super) mod crash;
    mod integration;
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::{
        cell::Cell,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    // Test teardown only: a concurrent Command spawn can temporarily inherit
    // the open file description until exec. close alone need not release flock.
    // Call only after the session is dropped; never use in the SIGKILL path.
    fn release_test_store(store: StateStore) {
        store.file.unlock().expect("test store unlock");
        drop(store);
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "rialo-persistence-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    struct Time(Cell<i64>);
    impl Clock for Time {
        fn now(&self) -> Result<ClockReading> {
            Ok(ClockReading {
                unix_ms: self.0.get(),
                monotonic_ms: self.0.get() as u64,
            })
        }
    }
    struct Mock;
    impl ReadRpc for Mock {
        async fn prefix(&self) -> Result<ConfigHashPrefix> {
            Ok(ConfigHashPrefix::new(42))
        }
        async fn fee(&self, _: &str) -> Result<Option<u64>> {
            Ok(Some(5000))
        }
        async fn balance(&self, _: &Pubkey) -> Result<u64> {
            Ok(2_000_000)
        }
    }
    async fn checked(clock: &Time, recipient: u8) -> CheckedTransfer {
        let key = SigningKey::from_bytes(&[73; 32]);
        let transfer = PreparedTransfer::new(
            TESTNET_RPC,
            &Pubkey::from(key.verifying_key().to_bytes()).to_string(),
            &Pubkey::from([recipient; 32]).to_string(),
            1234,
            ConfigHashPrefix::new(42),
        )
        .unwrap();
        let hash = signing_message_hash(transfer.signing_message());
        check_with_rpc_and_clock(transfer, hash, 6000, &Mock, 5000, clock)
            .await
            .unwrap()
    }
    async fn ready<'a>(
        store: &'a mut StateStore,
        clock: &Time,
    ) -> (PersistentLifecycle<'a>, SignatureResponse) {
        let mut s = store
            .register("request-1", checked(clock, 2).await, clock)
            .unwrap();
        s.validate_using(clock, |c| revalidate_with_rpc(c, &Mock, clock))
            .await
            .unwrap();
        let r = s.begin_signing(clock).unwrap();
        let key = SigningKey::from_bytes(&[73; 32]);
        let response = SignatureResponse {
            protocol_version: r.protocol_version,
            request_id: r.request_id,
            public_key: r.expected_public_key,
            signature: key.sign(&r.signing_message).to_bytes().to_vec(),
            signing_message_hash: r.signing_message_hash,
        };
        (s, response)
    }
    #[tokio::test]
    async fn normal_save_reload_and_signed_replay() {
        let dir = Temp::new();
        let clock = Time(Cell::new(10000));
        let mut store = StateStore::initialize(&dir.0).unwrap();
        {
            let (mut s, response) = ready(&mut store, &clock).await;
            assert!(
                !s.accept_response(response.clone(), &clock)
                    .unwrap()
                    .is_empty()
            );
            assert!(s.accept_response(response, &clock).is_err());
        }
        let record = store.record("request-1").unwrap().clone();
        assert_eq!(record.state, StoredState::Signed);
        release_test_store(store);
        let mut store = StateStore::open(&dir.0).unwrap();
        assert_eq!(store.record("request-1"), Some(&record));
        assert!(
            store
                .register("request-1", checked(&clock, 2).await, &clock)
                .is_err()
        );
    }
    #[tokio::test]
    async fn rejected_survives_restart() {
        let dir = Temp::new();
        let clock = Time(Cell::new(10000));
        let mut store = StateStore::initialize(&dir.0).unwrap();
        {
            let (mut s, mut response) = ready(&mut store, &clock).await;
            response.signature[0] ^= 1;
            assert!(s.accept_response(response.clone(), &clock).is_err());
            assert!(s.accept_response(response, &clock).is_err());
        }
        release_test_store(store);
        let mut store = StateStore::open(&dir.0).unwrap();
        assert_eq!(
            store.record("request-1").unwrap().state,
            StoredState::Rejected
        );
        assert!(
            store
                .register("request-1", checked(&clock, 2).await, &clock)
                .is_err()
        );
    }
    #[tokio::test]
    async fn expired_survives_restart() {
        let dir = Temp::new();
        let clock = Time(Cell::new(10000));
        let mut store = StateStore::initialize(&dir.0).unwrap();
        {
            let (mut s, response) = ready(&mut store, &clock).await;
            clock.0.set(15000);
            assert!(s.accept_response(response, &clock).is_err());
        }
        release_test_store(store);
        let mut store = StateStore::open(&dir.0).unwrap();
        assert_eq!(
            store.record("request-1").unwrap().state,
            StoredState::Expired
        );
        assert!(
            store
                .register("request-1", checked(&clock, 2).await, &clock)
                .is_err()
        );
    }
    #[tokio::test]
    async fn duplicate_and_different_message_id_refused() {
        let dir = Temp::new();
        let clock = Time(Cell::new(10000));
        let mut store = StateStore::initialize(&dir.0).unwrap();
        drop(
            store
                .register("same", checked(&clock, 2).await, &clock)
                .unwrap(),
        );
        assert!(
            store
                .register("same", checked(&clock, 2).await, &clock)
                .is_err()
        );
        assert!(
            store
                .register("same", checked(&clock, 3).await, &clock)
                .is_err()
        );
        release_test_store(store);
        let mut store = StateStore::open(&dir.0).unwrap();
        assert!(
            store
                .register("same", checked(&clock, 3).await, &clock)
                .is_err()
        );
    }
    #[tokio::test]
    async fn abandoned_awaiting_request_cannot_resume() {
        let dir = Temp::new();
        let clock = Time(Cell::new(10000));
        let mut store = StateStore::initialize(&dir.0).unwrap();
        let (s, _) = ready(&mut store, &clock).await;
        drop(s);
        release_test_store(store);
        clock.0.set(20000);
        let mut store = StateStore::open(&dir.0).unwrap();
        assert!(
            store
                .register("request-1", checked(&clock, 2).await, &clock)
                .is_err()
        );
    }
    #[tokio::test]
    async fn registration_io_failure_stops_and_poisons() {
        let dir = Temp::new();
        let clock = Time(Cell::new(10000));
        let mut store = StateStore::initialize(&dir.0).unwrap();
        let _locked =
            std::mem::replace(&mut store.file, File::open(dir.0.join(FILE_NAME)).unwrap());
        assert!(
            store
                .register("id", checked(&clock, 2).await, &clock)
                .is_err()
        );
        assert!(store.poisoned);
        assert!(
            store
                .register("new-id", checked(&clock, 2).await, &clock)
                .is_err()
        );
    }
    #[tokio::test]
    async fn request_not_exposed_on_save_failure() {
        let dir = Temp::new();
        let clock = Time(Cell::new(10000));
        let mut store = StateStore::initialize(&dir.0).unwrap();
        let mut s = store
            .register("id", checked(&clock, 2).await, &clock)
            .unwrap();
        s.validate_using(&clock, |c| revalidate_with_rpc(c, &Mock, &clock))
            .await
            .unwrap();
        let _locked = std::mem::replace(
            &mut s.store.file,
            File::open(dir.0.join(FILE_NAME)).unwrap(),
        );
        assert!(s.begin_signing(&clock).is_err());
        assert!(s.begin_signing(&clock).is_err());
    }
    #[tokio::test]
    async fn response_not_processed_on_claim_save_failure() {
        let dir = Temp::new();
        let clock = Time(Cell::new(10000));
        let mut store = StateStore::initialize(&dir.0).unwrap();
        let (mut s, response) = ready(&mut store, &clock).await;
        let _locked = std::mem::replace(
            &mut s.store.file,
            File::open(dir.0.join(FILE_NAME)).unwrap(),
        );
        assert!(s.accept_response(response.clone(), &clock).is_err());
        assert!(s.accept_response(response, &clock).is_err());
    }
    #[test]
    fn concurrent_handles_refused() {
        let dir = Temp::new();
        let store = StateStore::initialize(&dir.0).unwrap();
        assert!(StateStore::open(&dir.0).is_err());
        release_test_store(store);
        assert!(StateStore::open(&dir.0).is_ok());
    }
    #[test]
    fn missing_and_corrupt_files_refused() {
        let dir = Temp::new();
        assert!(StateStore::open(&dir.0).is_err());
        release_test_store(StateStore::initialize(&dir.0).unwrap());
        for bytes in [
            b"".as_slice(),
            b"invalid\n",
            b"RIALO-SIGNATURE-STATE-1\n{}\n",
            b"RIALO-SIGNATURE-STATE-1\npartial",
        ] {
            fs::write(dir.0.join(FILE_NAME), bytes).unwrap();
            assert!(StateStore::open(&dir.0).is_err());
        }
        assert!(StateStore::initialize(&dir.0).is_err());
    }
    #[test]
    fn permissions_restricted_and_insecure_file_refused() {
        let dir = Temp::new();
        release_test_store(StateStore::initialize(&dir.0).unwrap());
        assert_eq!(
            fs::metadata(&dir.0).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let path = dir.0.join(FILE_NAME);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(StateStore::open(&dir.0).is_err());
    }
    #[tokio::test]
    async fn checksum_corruption_refused() {
        let dir = Temp::new();
        let clock = Time(Cell::new(10000));
        let mut store = StateStore::initialize(&dir.0).unwrap();
        drop(
            store
                .register("request-1", checked(&clock, 2).await, &clock)
                .unwrap(),
        );
        release_test_store(store);
        let path = dir.0.join(FILE_NAME);
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("request-1", "request-2");
        fs::write(path, text).unwrap();
        assert!(StateStore::open(&dir.0).is_err());
    }
    #[tokio::test]
    async fn final_save_failure_withholds_wire_and_restart_blocks_retry() {
        let dir = Temp::new();
        let clock = Time(Cell::new(10000));
        let mut store = StateStore::initialize(&dir.0).unwrap();
        {
            let (mut s, response) = ready(&mut store, &clock).await;
            s.store.fail_state = Some(StoredState::Signed);
            assert!(s.accept_response(response.clone(), &clock).is_err());
            assert!(s.accept_response(response, &clock).is_err());
        }
        release_test_store(store);
        let mut store = StateStore::open(&dir.0).unwrap();
        assert_eq!(
            store.record("request-1").unwrap().state,
            StoredState::ProcessingResponse
        );
        assert!(
            store
                .register("request-1", checked(&clock, 2).await, &clock)
                .is_err()
        );
    }
    #[test]
    fn cross_process_lock_refuses_second_user() {
        const ENV: &str = "RIALO_PERSISTENCE_LOCK_TEST";
        if let Some(path) = std::env::var_os(ENV) {
            assert!(StateStore::open(Path::new(&path)).is_err());
            return;
        }
        let dir = Temp::new();
        let _store = StateStore::initialize(&dir.0).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "pre_sign::lifecycle::persistent::tests::cross_process_lock_refuses_second_user",
            ])
            .env(ENV, &dir.0)
            .output()
            .unwrap();
        assert!(output.status.success());
    }
}
