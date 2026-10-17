//! Library integration only: transport and signer are test doubles, not the CLI.
use super::*;
use std::cell::RefCell;
#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    Connect,
    Balance,
    FeeMissing,
    FeeLimit,
    Build,
    Sign,
    Save,
    Send,
    Unknown,
    Timeout,
    Execution,
}
struct App {
    fault: Fault,
    sends: Cell<u32>,
    events: RefCell<Vec<&'static str>>,
    message: RefCell<Vec<u8>>,
}
impl App {
    fn new(fault: Fault) -> Self {
        Self {
            fault,
            sends: Cell::new(0),
            events: RefCell::new(vec!["init"]),
            message: RefCell::new(Vec::new()),
        }
    }
    fn event(&self, name: &'static str) {
        self.events.borrow_mut().push(name);
    }
    async fn connect(&self) -> Result<()> {
        self.event("connect");
        if self.fault == Fault::Connect {
            return Err("mock connection failure".into());
        }
        Ok(())
    }
    async fn send(&self, wire: &[u8]) -> Result<&'static str> {
        self.event("send");
        self.sends.set(self.sends.get() + 1);
        let tx = rialo_cdk::transaction::builder::Transaction::deserialize_wired(wire)?;
        assert_eq!(tx.message(), self.message.borrow().as_slice());
        assert_eq!(tx.signatures.len(), 1);
        SigningKey::from_bytes(&[73; 32])
            .verifying_key()
            .verify_strict(
                tx.message(),
                &ed25519_dalek::Signature::from_slice(&tx.signatures[0])?,
            )?;
        if self.fault == Fault::Send {
            return Err("mock send failed; outcome uncertain".into());
        }
        Ok("mock-tx-id")
    }
    async fn outcome(&self, id: &str) -> Result<()> {
        assert_eq!(id, "mock-tx-id");
        self.event("result");
        match self.fault {
            Fault::Unknown => Err("mock result unknown; no resend".into()),
            Fault::Execution => Err("mock execution failed".into()),
            Fault::Timeout => std::future::pending().await,
            _ => Ok(()),
        }
    }
}
impl ReadRpc for App {
    async fn prefix(&self) -> Result<ConfigHashPrefix> {
        self.event("prefix");
        Ok(ConfigHashPrefix::new(42))
    }
    async fn fee(&self, encoded: &str) -> Result<Option<u64>> {
        self.event("fee");
        assert_eq!(STANDARD.decode(encoded)?, *self.message.borrow());
        Ok(match self.fault {
            Fault::FeeMissing => None,
            Fault::FeeLimit => Some(6001),
            _ => Some(5000),
        })
    }
    async fn balance(&self, from: &Pubkey) -> Result<u64> {
        self.event("balance");
        assert_eq!(
            from.to_bytes(),
            SigningKey::from_bytes(&[73; 32]).verifying_key().to_bytes()
        );
        Ok(if self.fault == Fault::Balance {
            TRANSFER_AMOUNT
        } else {
            2_000_000
        })
    }
}
async fn run(app: &App, store: &mut StateStore, clock: &Time, journal: &Path) -> Result<()> {
    app.connect().await?;
    let key = SigningKey::from_bytes(&[73; 32]); // Memory-only synthetic test key.
    let from = Pubkey::from(key.verifying_key().to_bytes()).to_string();
    app.event("public_key");
    let to = if app.fault == Fault::Build {
        "invalid".into()
    } else {
        Pubkey::from([2; 32]).to_string()
    };
    let transfer =
        PreparedTransfer::new(TESTNET_RPC, &from, &to, 10000, ConfigHashPrefix::new(42))?;
    app.event("build");
    *app.message.borrow_mut() = transfer.signing_message().to_vec();
    let hash = signing_message_hash(transfer.signing_message());
    let checked = check_with_rpc_and_clock(transfer, hash, 6000, app, 5000, clock).await?;
    let mut session = store.register("integrated-request", checked, clock)?;
    session
        .validate_using(clock, |c| revalidate_with_rpc(c, app, clock))
        .await?;
    let request = session.begin_signing(clock)?;
    app.event("request");
    if app.fault == Fault::Sign {
        return Err("mock signer unavailable".into());
    }
    let response = SignatureResponse {
        protocol_version: request.protocol_version,
        request_id: request.request_id,
        public_key: request.expected_public_key,
        signing_message_hash: request.signing_message_hash,
        signature: key.sign(&request.signing_message).to_bytes().to_vec(),
    };
    app.event("dummy_sign");
    // Actual write failure, not a successful dummy persistence implementation.
    let _held_lock = if app.fault == Fault::Save {
        Some(std::mem::replace(
            &mut session.store.file,
            File::open(journal)?,
        ))
    } else {
        None
    };
    let wire = session.accept_response(response, clock)?;
    assert_eq!(
        session.store.record("integrated-request").unwrap().state,
        StoredState::Signed
    );
    app.event("persisted_signed");
    let id = app.send(&wire).await?;
    tokio::time::timeout(std::time::Duration::from_millis(20), app.outcome(id))
        .await
        .map_err(|_| "mock result timeout; no resend")??;
    Ok(())
}
async fn scenario(fault: Fault, expected: Option<StoredState>, sends: u32) {
    let dir = Temp::new();
    let clock = Time(Cell::new(10000));
    let mut store = StateStore::initialize(&dir.0).unwrap();
    let app = App::new(fault);
    let result = run(&app, &mut store, &clock, &dir.0.join(FILE_NAME)).await;
    assert_eq!(result.is_ok(), fault == Fault::None);
    assert_eq!(app.sends.get(), sends);
    let before = fs::read(dir.0.join(FILE_NAME)).unwrap();
    release_test_store(store);
    let mut restored = StateStore::open(&dir.0).unwrap();
    assert_eq!(
        restored.record("integrated-request").map(|r| r.state),
        expected
    );
    if expected.is_some() {
        assert!(
            restored
                .register("integrated-request", checked(&clock, 2).await, &clock)
                .is_err()
        );
    }
    assert_eq!(fs::read(dir.0.join(FILE_NAME)).unwrap(), before);
    if fault == Fault::None {
        let events = app.events.borrow();
        assert_eq!(events.iter().filter(|s| **s == "balance").count(), 2);
        assert_eq!(events.iter().filter(|s| **s == "fee").count(), 2);
        let position = |s| events.iter().position(|v| *v == s).unwrap();
        assert!(position("request") < position("dummy_sign"));
        assert!(position("dummy_sign") < position("persisted_signed"));
        assert!(position("persisted_signed") < position("send"));
        assert!(position("send") < position("result"));
    }
}
macro_rules! case {
    ($name:ident,$fault:ident,$state:expr,$sends:expr) => {
        #[tokio::test]
        async fn $name() {
            scenario(Fault::$fault, $state, $sends).await;
        }
    };
}
case!(offline_pipeline_success, None, Some(StoredState::Signed), 1);
case!(connection_failure, Connect, None, 0);
case!(insufficient_balance_including_fee, Balance, None, 0);
case!(missing_fee, FeeMissing, None, 0);
case!(fee_exceeds_limit, FeeLimit, None, 0);
case!(construction_failure, Build, None, 0);
case!(
    dummy_signer_failure,
    Sign,
    Some(StoredState::AwaitingSignature),
    0
);
case!(
    save_failure_blocks_send,
    Save,
    Some(StoredState::AwaitingSignature),
    0
);
case!(
    mock_send_failure_no_retry,
    Send,
    Some(StoredState::Signed),
    1
);
case!(
    unknown_result_no_retry,
    Unknown,
    Some(StoredState::Signed),
    1
);
case!(timeout_no_retry, Timeout, Some(StoredState::Signed), 1);
case!(execution_failure, Execution, Some(StoredState::Signed), 1);
