//! Actual SIGKILL tests. Hooks are compiled only into the unit-test executable.
use super::*;
use std::{
    os::unix::process::ExitStatusExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const CHILD_TEST: &str = "pre_sign::lifecycle::persistent::tests::crash::child_process";
const ROOT: &str = "RIALO_7H_TEST_ROOT";
const MODE: &str = "RIALO_7H_TEST_MODE";
const STATE: &str = "RIALO_7H_TEST_STATE";
const POINT: &str = "RIALO_7H_TEST_POINT";

pub(crate) fn selected(state: StoredState, point: &str) -> bool {
    std::env::var(MODE).as_deref() == Ok("crash")
        && std::env::var(STATE).ok().as_deref() == Some(format!("{state:?}").as_str())
        && std::env::var(POINT).as_deref() == Ok(point)
}
pub(crate) fn checkpoint(state: StoredState, point: &str) {
    if selected(state, point) {
        let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
        fs::write(root.join("reached"), b"ready").unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        // create_dir succeeds only for a new directory; never owns an existing path.
        loop {
            let path = std::env::temp_dir().join(format!(
                "rialo-7h-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("temporary directory creation failed: {e}"),
            }
        }
    }
    fn store(&self) -> PathBuf {
        self.0.join("store")
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn spawn(root: &Path, mode: &str, state: &str, point: &str) -> OwnedChild {
    OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST])
            .env(ROOT, root)
            .env(MODE, mode)
            .env(STATE, state)
            .env(POINT, point)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}
fn completed(root: &Path, mode: &str, expected: &str) {
    let mut child = spawn(root, mode, expected, "");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "restart child failed: {mode}/{expected}");
            return;
        }
        assert!(Instant::now() < deadline, "restart child timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[tokio::test]
async fn child_process() {
    let Some(root) = std::env::var_os(ROOT).map(PathBuf::from) else {
        return;
    };
    let clock = Time(Cell::new(10000));
    let path = root.join("store");
    match std::env::var(MODE).unwrap().as_str() {
        "crash" => {
            let mut store = StateStore::open(&path).unwrap();
            let (mut session, response) = ready(&mut store, &clock).await;
            let wire = session.accept_response(response, &clock).unwrap();
            assert!(!wire.is_empty());
            panic!("crash hook was not reached");
        }
        "reject" => {
            assert!(StateStore::open(&path).is_err());
        }
        "recover" => {
            let mut store = StateStore::open(&path).unwrap();
            let state = std::env::var(STATE).unwrap();
            assert_eq!(
                format!("{:?}", store.record("request-1").unwrap().state),
                state
            );
            assert_eq!(
                store.record("baseline").unwrap().state,
                StoredState::Created
            );
            assert!(
                store
                    .register("request-1", checked(&clock, 2).await, &clock)
                    .is_err()
            );
            assert!(
                store
                    .register("request-1", checked(&clock, 3).await, &clock)
                    .is_err()
            );
        }
        "io_failure" => {
            let mut store = StateStore::open(&path).unwrap();
            let (mut session, response) = ready(&mut store, &clock).await;
            let _locked = std::mem::replace(
                &mut session.store.file,
                File::open(path.join(FILE_NAME)).unwrap(),
            );
            assert!(session.accept_response(response.clone(), &clock).is_err());
            assert!(session.accept_response(response, &clock).is_err());
            assert!(session.store.poisoned);
        }
        _ => panic!("unknown test mode"),
    }
}
async fn baseline(w: &Workspace) -> Vec<u8> {
    let clock = Time(Cell::new(10000));
    let mut store = StateStore::initialize(&w.store()).unwrap();
    drop(
        store
            .register("baseline", checked(&clock, 2).await, &clock)
            .unwrap(),
    );
    release_test_store(store);
    fs::read(w.store().join(FILE_NAME)).unwrap()
}
async fn kill_at(state: &str, point: &str, recovered: Option<&str>) {
    let w = Workspace::new();
    let before = baseline(&w).await;
    let mut child = spawn(&w.0, "crash", state, point);
    let deadline = Instant::now() + Duration::from_secs(15);
    while !w.0.join("reached").exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "child exited before checkpoint"
        );
        assert!(Instant::now() < deadline, "checkpoint timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Only the Child handle created above is terminated.
    child.0.kill().unwrap();
    let status = child.0.wait().unwrap();
    assert_eq!(status.signal(), Some(9));
    let after = fs::read(w.store().join(FILE_NAME)).unwrap();
    assert!(
        after.starts_with(&before),
        "pre-existing journal prefix changed"
    );
    if let Some(expected) = recovered {
        completed(&w.0, "recover", expected);
    } else {
        completed(&w.0, "reject", "");
    }
    assert_eq!(
        fs::read(w.store().join(FILE_NAME)).unwrap(),
        after,
        "recovery modified journal"
    );
}
macro_rules! crash_test {
    ($name:ident,$state:literal,$point:literal,$expected:expr) => {
        #[tokio::test]
        async fn $name() {
            kill_at($state, $point, $expected).await;
        }
    };
}
crash_test!(
    kill_before_response_write,
    "ProcessingResponse",
    "before_write",
    Some("AwaitingSignature")
);
crash_test!(
    kill_partial_response_json,
    "ProcessingResponse",
    "partial_json",
    None
);
crash_test!(
    kill_before_response_sync,
    "ProcessingResponse",
    "before_sync",
    Some("ProcessingResponse")
);
crash_test!(
    kill_after_response_sync,
    "ProcessingResponse",
    "after_sync",
    Some("ProcessingResponse")
);
crash_test!(kill_partial_signed_json, "Signed", "partial_json", None);
crash_test!(
    kill_after_signed_sync,
    "Signed",
    "after_sync",
    Some("Signed")
);
crash_test!(
    kill_during_validation,
    "Validating",
    "after_sync",
    Some("Validating")
);

#[tokio::test]
async fn save_failure_then_fresh_process_rejects_reuse() {
    let w = Workspace::new();
    let before = baseline(&w).await;
    completed(&w.0, "io_failure", "");
    let after = fs::read(w.store().join(FILE_NAME)).unwrap();
    assert!(after.starts_with(&before));
    completed(&w.0, "recover", "AwaitingSignature");
    assert_eq!(fs::read(w.store().join(FILE_NAME)).unwrap(), after);
}
#[tokio::test]
async fn missing_file_rejected_without_recreation() {
    let w = Workspace::new();
    let before = baseline(&w).await;
    let path = w.store().join(FILE_NAME);
    let saved = w.0.join("preserved.journal");
    fs::rename(&path, &saved).unwrap(); // Test-only fault, not a production save stage.
    completed(&w.0, "reject", "");
    assert!(!path.exists());
    assert_eq!(fs::read(saved).unwrap(), before);
}
#[tokio::test]
async fn permissions_rejected_without_modification() {
    let w = Workspace::new();
    let before = baseline(&w).await;
    let path = w.store().join(FILE_NAME);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    completed(&w.0, "reject", "");
    assert_eq!(fs::read(path).unwrap(), before);
}
