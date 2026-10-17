# rialo-first

Learning project for Rialo CDK / RPC transfer preparation, balance queries and fee
queries. Uses Rialo CDK 0.18.1. The CLI executable is fixed to the CDK's
**DevNet** RPC; it does not provide a Testnet/network selection option.

## Implemented

- Sender and recipient public-key parsing
- Fixed 0.001 RLO unsigned transfer message construction and format verification
- RPC connection, block height and config hash queries
- Sender balance and `getFeeForMessage` fee queries
- Safe stop on insufficient balance, unknown fee, timeout or RPC error
- Offline tests
- Offline Rust library for strict verification and assembly of externally signed
  Testnet transfers; see [Phase 7B](docs/phase-7b-external-signatures.md)

## Not implemented

- Keyring signing
- CLI external-signer integration (library assembly is available)
- `send_transaction`
- Faucet execution
- Transaction confirmation

See [preparation instructions](PREPARATION.md) for usage and limitations.

Phase 7 signing/send implementation is blocked on verification of CLI alias / CDK
Keyring compatibility. See [Phase 7B scope and remaining work](docs/phase-7b-external-signatures.md).
Send flags remain unsupported. Phase 7B adds an offline external-signature library;
it does not resolve CLI wallet compatibility or enable sending.

Phase 7C is blocked at design review: the current transfer message does not encode
the fee or RPC endpoint, and offline inputs do not establish Testnet provenance.
No signer transport or request/response interface has been enabled.
See [Phase 7C design and blocker](docs/phase-7c-external-signer-interface.md).

Phase 7D adds opt-in read-only pre-sign checks for Testnet config prefix, the exact
message's fee estimate, a user-supplied estimate limit, and balance. Offline tests
cover failure paths; live RPC validation and signing integration remain unperformed.
See [Phase 7D checks and limitations](docs/phase-7d-pre-sign-checks.md).

Phase 7E adds configurable check expiry, injected wall/monotonic clocks, and fresh
RPC revalidation of the same message before any future signing request. It remains
unconnected to signing and is tested offline only.
See [Phase 7E expiry and revalidation](docs/phase-7e-check-expiration.md).

Phase 7F connects fresh revalidation, in-memory signature requests/responses,
terminal session states, and verified local transaction assembly. Offline lifecycle
tests pass; no external signer transport, real-wallet integration or sending is enabled.
See [Phase 7F lifecycle and limitations](docs/phase-7f-signature-lifecycle.md).

Phase 7G adds an opt-in Unix persistent lifecycle journal with exclusive locking,
durable state transitions, and rejection of every previously registered request ID
after restart. Offline tests pass; crash recovery and filesystem limits are documented.
See [Phase 7G persistence and replay prevention](docs/phase-7g-persistent-signature-state.md).

## Separate Testnet E2E evidence

The successful 2026-10-11 Testnet transfer was signed and sent using the existing
Rialo CLI, **not this application's Rust code**. RPC results confirmed the transfer
and balance changes; the Playground UI balance update remains unverified.

See [Rialo Testnet Transfer E2E — 2026-10-11](docs/2026-10-11-testnet-transfer-e2e.md).

## Explicit Testnet read-only connection

Run `cargo run --offline --locked -- --testnet-readonly` for Testnet config
prefix and block height. Optionally add `--address PUBLIC_KEY` for balance.
This opt-in command performs live read-only RPC; ordinary tests remain offline.
It does not load a wallet, build/sign/send a transaction, or open signature state.
The older preparation mode still uses DevNet.
See [Phase 7L observations and limitations](docs/phase-7l-testnet-readonly.md).

## Testnet transfer preparation

Use `--testnet-prepare-transfer --from PUBLIC_KEY --to PUBLIC_KEY --amount-kelvin INTEGER --fee-limit-kelvin INTEGER`.
Both numbers must be positive integers. This opt-in command uses live read-only RPC.
It requires balance to cover amount plus the entire fee limit, and checks the exact
unsigned message's fee estimate. It never signs, broadcasts, or opens signature state.
PREPARATION PASSED is not a successful transfer or a guaranteed execution fee cap.
See [Phase 7N](docs/phase-7n-transfer-preparation.md) for offline results and limitations.
