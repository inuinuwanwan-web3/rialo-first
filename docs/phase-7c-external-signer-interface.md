# Phase 7C — external signer interface (2026-10-15)

## Status: safety blocker identified; interface not implemented

The task requires proving that the displayed network, recipient, amount and fee
match the signing target before proceeding to signing. The existing fixed transfer
message commits to the sender, recipient, amount, timestamp and config hash prefix.
It does not encode an RPC URL or a fee amount. The offline inputs currently available
cannot establish the hash prefix's Testnet provenance or an authenticated fee quote.
We therefore do not emit signing requests, accept protocol responses, or enable a
transport. Phase 7B remains unchanged; no new claims about its network guarantees
are made. This is a blocker for these requirements, not a claim that all external
signer architectures are impossible.

## Evidence from installed 0.18.1 sources

Crates are under `/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

| Evidence | Source |
| --- | --- |
| Legacy message fields: header, account keys, valid_from, config_hash_prefix, OCC, instructions | `rialo-s-message-0.18.1/src/legacy.rs:168` |
| Exact message serialization; no separate fee or RPC URL field | `rialo-cdk-0.18.1/src/transaction/builder.rs:645` |
| Fixed transfer instruction uses from/to and kelvin | `rialo-cdk-0.18.1/src/transaction/builder.rs:221` |
| Transfer instruction data encodes operation index and transferred amount, not transaction fee | `rialo-cdk-0.18.1/src/transaction/instruction.rs:100`, `:123` |
| Fee request contains the Base64-encoded versioned message; response contains context and optional fee value | `rialo-api-types-0.18.1/src/messages/get_fee_for_message.rs:11`, `:24` |
| Existing endpoint allowlist is only an offline policy | `src/lib.rs:29`, `:44` |
| Existing canonical encoding and required-signer checks | `src/lib.rs:57` |
| Existing raw-message accessor and strict Ed25519 verification/assembly | `src/lib.rs:76`, `:82` |

Recipient and transferred amount can be compared by decoding the exact message,
checking the System Program and transfer operation, resolving instruction account
indices, and comparing the little-endian u64 amount. Existing unsigned tests check
this (`src/main.rs:215`). A signed config hash prefix supplies replay-protection
material, but its mapping to Testnet is not established by an arbitrary URL label.

A fee quote is external evidence about a message, not a fee field signed by this
transaction. Copying 5,000 kelvin from the prior CLI E2E, hashing a JSON request,
or comparing user-provided fee fields would not prove the requested consistency.
Even a fresh quote is not by itself a signed maximum-fee constraint.

## Proposed protocol (design only)

The following schema is a proposal, not a supported input/output format.
Strict JSON parsing would reject unknown or duplicate fields and oversized inputs.

| Request field | Proposed representation / rule |
| --- | --- |
| protocol_version | Integer 1; exact supported-version check |
| network | Literal `testnet`; requires independent provenance evidence |
| rpc_endpoint | Exact `https://testnet.rialo.io/`; policy check alone is insufficient |
| sender_address | Canonical Base58 public key equal to fee payer and required signer |
| recipient_address | Canonical Base58 public key equal to transfer destination |
| amount_kelvin | JSON integer u64, fixed 1,000,000, equal to decoded instruction amount |
| fee_kelvin | JSON integer u64; BLOCKED pending agreed quote/evidence semantics |
| signing_message | Canonical Base64 of exact CDK raw message bytes; signer decodes before signing |
| signing_message_hash | Lowercase hexadecimal SHA-256 of decoded raw bytes; not the signing input |
| expected_public_key | Equal to sender and required signer |
| request_id | Unique correlation ID, bound to immutable pending request state |
| expires_at_unix_ms | Integer deadline, locally enforced using a trusted clock |

| Response field | Proposed representation / rule |
| --- | --- |
| protocol_version | Must match supported version and pending request |
| request_id | Must match one unconsumed, unexpired pending request |
| public_key | Must match expected sender public key |
| signature | Canonical Base64 decoding to exactly 64 detached Ed25519 signature bytes |

The signer must sign the raw CDK message, never JSON, Base64 text or the hash.
Request metadata is not automatically authenticated by that transaction signature.
Expiry/request_id are local session controls, not on-chain replay protection.
The same valid signature may match multiple requests with identical message bytes;
IDs alone do not prevent this. A future implementation must define replay state
across restarts and prohibit reissuing consumed messages or explicitly document its
session-only guarantee. An invalid response must not release transaction bytes.

Once the evidence blocker is resolved, the response handler should reuse
`PreparedTransfer::assemble` for cryptographic verification and wire serialization,
instead of implementing Ed25519 verification again. Pending-request state should
be consumed only once following successful validation, with expiry rechecked before
returning wire bytes. No secret-key field or wallet loading belongs in this protocol.

## Transport comparison

| Option | Safety / complexity | Existing CLI compatibility |
| --- | --- | --- |
| stdin/stdout | No listener or persistent files required; simple bounded JSON framing. Requires trusted process/pipe routing and stderr-only diagnostics. | Unverified |
| local files | Simple manual handoff; needs permissions, atomic writes, stale-file/replay handling and explicit path controls. | Unverified |
| local HTTP | Requires listener lifecycle, authentication, request limits and origin/access controls; unnecessary complexity here. | Unverified |

Design selection: stdin/stdout with one bounded JSON request/response per session.
No process spawning, pipe communication or HTTP server is implemented or executed.
This selection is not evidence that the existing CLI accepts the proposed protocol.

## Implementation and validation scope

Only this design/blocker record and the README status are changed. Source code,
dependencies, prior success records and `docs/phase-7-signing-investigation.md`
are preserved. There are no new protocol tests because no protocol implementation
was enabled. The requested normal request/response, ID/version mismatch, expiry,
display-field tampering and response-replay tests remain outstanding; Phase 7B
signature tests must not be presented as covering those cases.

Validation on 2026-10-15: `cargo fmt`, `cargo fmt --check`,
`cargo check --offline --locked`, `cargo test --offline --locked` and
`git diff --check` passed. All 14 existing tests passed (7 library, 7 executable);
0 new Phase 7C protocol tests. Dependency `proc-macro-error2 2.0.1` retains its
future Rust compatibility warning. Only synthetic in-memory test signing occurred.
Real-wallet signatures, TX submissions, Faucet calls, commits and pushes: all zero.

Next task: agree on a verifiable evidence contract for Testnet provenance and fee
quotes, explicitly distinguishing external quote validation from a signed fee limit.
No requirements are silently relaxed in this phase.
