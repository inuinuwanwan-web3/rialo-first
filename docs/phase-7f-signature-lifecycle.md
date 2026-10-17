# Phase 7F — external signature lifecycle (2026-10-15)

## Scope and reused APIs

The in-memory library now connects Phase 7B signature verification/assembly,
Phase 7D RPC checks and Phase 7E expiry/revalidation. It exposes Rust data structures
and local state transitions only. No wallet or external signer is contacted, no
process is spawned, no stdin/file/HTTP transport is implemented, and no TX is sent.
The CLI and dependencies are unchanged. The prior successful E2E records are intact.

Implementation lives in `src/pre_sign/lifecycle.rs`, a child of the pre-sign module.
This permits reuse of private checked-state invariants without a public unchecked
constructor or a public mock-RPC bypass.

- `src/lib.rs:77`: exact raw CDK signing-message bytes.
- `src/lib.rs:83`: existing sender comparison, strict Ed25519 verification, and wire
  assembly. The lifecycle delegates to this method; it does not duplicate crypto.
- `src/pre_sign.rs:99`: Phase 7E local clock/expiry/integrity validation.
- `src/pre_sign.rs:173`: fresh RPC revalidation, including original fee limit and
  original message, delegated from lifecycle `validate()`.
- `src/pre_sign/lifecycle.rs:220`: request fields decoded and checked against the
  exact message, including reconstructing the existing fixed transfer byte-for-byte.

## Request and response representation

`SignatureRequest` contains protocol_version (1), request_id, created_at,
expires_at, network (`testnet`), rpc_endpoint, network_config_prefix,
sender_address, recipient_address, amount_kelvin, estimated_fee_kelvin,
estimated_fee_limit_kelvin, signing_message, signing_message_hash and
expected_public_key. Addresses use canonical Base58 strings. The message is raw
bytes and the digest is 32 raw SHA-256 bytes. Timestamps are Unix milliseconds.

`SignatureResponse` contains protocol_version, request_id, public_key,
signature (raw detached 64-byte Ed25519 signature) and signing_message_hash.
The signer signs **exactly the request's raw message bytes**, not the request
metadata, hash or Base64 text. These structs are not a JSON wire protocol; they
have no serde parser/transport and do not implement the earlier proposed Base64
JSON encoding. Future transport work must explicitly define bounded encoding and
perform strict input validation.

Sender, recipient and amount are decoded from the one System Program transfer
instruction, checked against the fee payer, then compared through byte-identical
reconstruction using the existing transfer builder. Extra instructions, unexpected
instruction layout, amount changes or prefix disagreement fail. The prefix is
both in the message and equal to the fresh Phase 7D/7E observation. Network label
and RPC URL follow the fixed trusted Testnet policy; they are not signed fields or
independent chain-identity proofs. Fees come from the fresh checked report and are
**estimates**, not on-chain fee caps.

Session state and its retained request are private, not Clone/deserializable.
`begin_signing` returns a copy; callers cannot replace the session's original by
mutating that copy. Before accepting a response the session compares its retained
request against the checked message/report. This does not authenticate the display
of an arbitrary external UI: a future signer must independently decode/compare
display fields and use a trusted transport. No real-wallet safety is claimed.

## Lifecycle

| Operation | Required state | Successful next state | Failure |
| --- | --- | --- | --- |
| new(CheckedTransfer) | No session | CREATED | No session |
| validate(clock) | CREATED | VALIDATED | REJECTED, or EXPIRED if detected before RPC |
| begin_signing(clock) | VALIDATED | AWAITING_SIGNATURE | REJECTED / EXPIRED |
| accept_response(response, clock) | AWAITING_SIGNATURE | SIGNED; wire bytes returned once | REJECTED / EXPIRED; no wire bytes |
| Any repeated/invalid operation | Other state | Never advances | Terminal states cannot be reused |

Construction does not expose a signature request. `validate()` first consumes the
old checked state, sets the session to REJECTED while awaiting fresh revalidation,
then constructs a request only after success. Cancellation or RPC errors therefore
cannot leave a reusable session. VALIDATED is reached only after the fresh checks.
Expired checks cannot be renewed. Revalidation failures, including expiry reported
inside RPC revalidation, leave REJECTED; locally observed expiry sets EXPIRED.

Before exposing a request, expiry is checked again. created_at is the post-check
creation time; expires_at is inherited unchanged from Phase 7E and is never extended
while awaiting a signature. On response, the session checks current expiry, retained
request integrity, protocol version, ID, expected public key and message hash.
Phase 7B verifies the Ed25519 signature and assembles wire bytes. Clock/expiry is
checked again before returning bytes, preventing expiry during assembly from
releasing a TX. No automatic retry is performed. Missing signature bytes are rejected.

Invalid responses permanently consume the request. Duplicate responses return an
error. SIGNED stays SIGNED on duplicate calls; EXPIRED stays EXPIRED. Rejected state
cannot be revalidated or restarted. Failed/consumed checked objects are not returned.
The supplied clock must satisfy the trusted clock contract of Phase 7E.

## Offline tests and validation

Existing 42 tests are retained. 24 new lifecycle tests cover request fields,
valid response generation/cryptographic verification, exact-message wire round trip,
invalid signature, missing signature, mismatched public key/ID/version/hash, message,
recipient, amount, network and fee-display tampering, expiry and terminal-state reuse,
double response, premature request exposure, network/fee/RPC failures before request
creation, clock failures, expiry during assembly, validation cancellation and distinct
process-local IDs. Mock RPC revalidation must occur before exposing each test request.
Test keys are synthetic in-memory Ed25519 keys; keys and signatures are not logged
or written to files. No production RPC constructor is called by these tests.

`cargo fmt`, `cargo fmt --check`, `cargo check --offline --locked`,
`cargo test --offline --locked` passed: **66 tests passed, 0 failed** (59 library,
7 executable). `git diff --check` passed. Existing proc-macro-error2 future-Rust
compatibility warning remains. No dependency or Cargo.lock changes were needed.

## Limits and next task

Request IDs are unique only within the current process, using an atomic counter
that fails on exhaustion. IDs are not signed or secret. There is no durable replay
registry, cross-restart uniqueness or global rejection of the same message in a
different session. An Ed25519 signature authenticates only the message, not request
ID, expiry, fee quote or UI metadata. Recreating a session with identical message
bytes can accept the same mathematical signature under new response metadata;
this phase guarantees one response per session, not global replay protection.

Time validity cannot guarantee network state, fee or balance remains unchanged
while waiting. Response handling does not perform additional RPC queries; the
last network observations are the pre-request revalidation. Real Testnet behavior,
external signer compatibility and absolute host-clock correctness remain unverified.
Chain-level transaction age and actual fee upper bounds are not guaranteed.
Phase 7B's low-level assembly remains separately accessible.

Next task: define durable request identity/replay policy and a bounded serialization
contract before adding an external signer transport. No real-wallet use is enabled.

Real-wallet signatures, TX submissions, Faucet, commits and pushes: zero.
`docs/phase-7-signing-investigation.md` and prior success records were not changed.
