# Phase 7D — pre-sign checks (2026-10-15)

## Implementation

`pre_sign::pre_sign_check(transfer, reviewed_message_hash, estimated_fee_limit_kelvin)`
consumes a Phase 7B `PreparedTransfer` and returns `Result<CheckedTransfer>`.
The caller must capture SHA-256 of the exact reviewed raw message using
`signing_message_hash` before requesting checks. Recomputing a baseline from an
unreviewed replacement message is not an integrity check.

The public function creates a single CDK `HttpRpcClient` for the fixed trusted
endpoint `https://testnet.rialo.io/`. A private three-method adapter permits offline
mocks without exposing an arbitrary RPC provider as the production entry point.
The overall operation has a 30-second timeout. It performs only read-only RPCs
when explicitly invoked; neither the CLI nor tests invoke the production function.

Checks, in order:

1. Compare current message SHA-256 with the caller's reviewed-message digest.
2. Decode the exact bytes as VersionedMessage and check canonical reserialization.
3. Extract its config hash prefix; compare against `getRecentValidatorConfigHash`.
4. Base64-encode those same bytes for `getFeeForMessage`; reject error or absent fee.
5. Require the typed u64 estimate to be at most the caller's limit. A zero estimate
   is valid if the RPC returns it; negative/fractional/string/out-of-range values
   are rejected by the RPC response decoder.
6. Checked-add the fixed transfer amount and estimated fee; reject overflow.
7. Query the prepared sender's balance and require it to cover that total.
8. Query prefix again to detect configuration changes during these observations.
9. Recheck the message digest and return a success-only checked object.

The prepared object is owned and has private fields; its message cannot be changed
by a caller while these asynchronous checks are pending. The existing builder,
canonical message layout and Phase 7B immutable message accessor are reused.
There is no duplicated Ed25519 verification or signing implementation.

The returned report includes `network_check`, `config_hash_prefix_match`,
`estimated_fee_kelvin`, `estimated_fee_limit_kelvin`, `balance_sufficient`, and
`signing_message_hash` (32 raw SHA-256 bytes). `network_check=true` means that the
message prefix matched observations from the configured trusted RPC; it is not
independent cryptographic proof of chain identity.

`CheckedTransfer` fields are private and its constructor is internal. Errors yield
no checked object. Only read-only report/message accessors are exposed: no signing,
assembly or sending method is connected. Existing Phase 7B `assemble` remains a
separate offline primitive, not a globally gated execution pipeline. Any future
signer integration must accept checked state and enforce freshness, rather than
treating the old primitive as pre-authorized or accepting caller-built reports.

## Local API evidence

Paths below are within the installed 0.18.1 crates in the Cargo registry.

- `rialo-cdk/src/transaction/builder.rs:488`: signs `build()` raw message bytes.
- `rialo-cdk/src/transaction/builder.rs:663`: config hash prefix is serialized.
- `rialo-cdk/src/rpc/client.rs:1005`: get_config_hash_prefix calls
  getRecentValidatorConfigHash.
- `rialo-cdk/src/rpc/client.rs:766`: get_fee_for_message sends the supplied Base64
  message and returns the optional fee; RPC response context is not retained.
- `rialo-api-types/src/messages/get_fee_for_message.rs:11`, `:24`: request is a
  Base64 versioned message and response fee is Option<u64>.
- `rialo-cdk/src/rpc/client.rs:272`, `:289`: the non-localhost HTTPS client uses
  certificate validation. Host/TLS/DNS/proxy trust follows CDK/HTTP client defaults;
  this change does not add certificate pinning or custom redirect policy.

## Limits

The fee is a **quote**, not a signed fee amount or on-chain fee cap. Quote, balance,
and prefix are separate non-atomic observations; values can change after checking.
There is no quote expiry, preserved RPC context slot, reservation of funds, or
independent genesis/chain-ID authentication. Two prefix observations do not remove
all race conditions. Actual fee bounds still require additional official guarantees.

No actual RPC execution was performed in this phase. Production endpoint behavior,
TLS/network failure handling against a real server, and quote freshness remain
unverified. Errors and overall timeout fail closed by construction. Future use
needs checks immediately before a separately authorized signing operation.

## Offline validation

Ten new tests cover normal matching prefixes and exact fee-request bytes, mismatched
prefixes, errors at prefix/fee/balance stages, missing fee, estimate-limit excess,
insufficient balance, message mutation before RPC, malformed RPC fee data, arithmetic
overflow, and prefix changes during queries. The malformed-data test exercises the
same API response type used by CDK with both invalid and valid JSON fixtures.
Mocks do not create clients, read wallets or call the network. New tests use only a
public Ed25519 basepoint; existing Phase 7B synthetic signing tests remain unchanged.

`cargo fmt`, `cargo fmt --check`, `cargo check --offline --locked`,
`cargo test --offline --locked`: passed. 24 tests passed, 0 failed (17 library,
7 executable). The existing proc-macro-error2 future-Rust warning remains.
`sha2 0.10.9` and test-only `serde_json 1.0.151` were already cached transitive
dependencies; only direct dependency edges were added to Cargo.lock offline.

Next task: design freshness/expiry rules for consuming `CheckedTransfer` in the
external-signature request interface, with explicit fee-estimate semantics.

Real-wallet signing, TX submission, Faucet, commits and pushes: zero.
