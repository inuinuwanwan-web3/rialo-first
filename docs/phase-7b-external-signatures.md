# Phase 7B: offline external-signature assembly

## Scope

The Rust library accepts detached Ed25519 signatures for a locally constructed,
fixed 1,000,000 kelvin transfer. It never loads a wallet, signs, reads files,
connects to RPC, or sends a transaction. The existing DevNet read-only executable
retains its arguments and behavior. Both paths share `transfer_builder`.
The prior CLI E2E record is unchanged and is not evidence of this library's E2E.

## Local source evidence

Paths below are relative to the installed `rialo-cdk-0.18.1` crate under
`/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

| API / behavior | Source |
| --- | --- |
| `build_transaction() -> Result<Transaction>` builds a message and zero-filled signature slots | `src/transaction/builder.rs:529`, `:30` |
| Exact signing input is `build()` message bytes, without transaction envelope or encoding | `src/transaction/builder.rs:488` |
| `add_signature_for_pubkey(&mut self, &Pubkey, Vec<u8>) -> Result<()>` locates the key among the first `num_required_signatures` accounts | `src/transaction/builder.rs:66` |
| `add_signature` checks 64-byte length and index, then overwrites the slot; no cryptographic verification or duplicate rejection | `src/transaction/builder.rs:48` |
| Actual method is `serialize_wired() -> Result<Vec<u8>>`, NOT `serialize_wire()` | `src/transaction/builder.rs:38` |
| Wire representation: compact signature count, 64-byte signatures, raw message | `src/transaction/builder.rs:705` |
| Missing slots are all-zero; `is_fully_signed()` only tests nonzero bytes, and serialization does not validate completeness | `src/transaction/builder.rs:30`, `:100`, `:705` |

Verification uses installed `ed25519-dalek-2.2.0/src/verifying.rs:357`,
`VerifyingKey::verify_strict(message, signature)`. It rejects weak keys/points and
invalid signatures. This dependency already existed transitively; it is now a pinned
direct dependency. Cargo.lock only adds the direct dependency edge.

## Library usage contract

1. Construct `PreparedTransfer::new(TESTNET_RPC, from, to, valid_from, hash)` with
   public Base58 addresses and a config hash prefix. No key material is an input.
2. Give the external signer **exactly** `signing_message()` bytes. This is plain
   Ed25519, not Ed25519ph, not a hash, not Base64 text, and not a transaction envelope.
3. Pass exactly one `ExternalSignature { public_address, signature }` to `assemble`.
   `signature` is a raw 64-byte detached signature. The address must equal the
   message's fee payer / required signer and the requested sender.
4. `assemble` verifies against the immutable message and returns wire bytes in memory.
   It consumes the prepared object; there is no signature-overwrite operation.

The constructor rejects any endpoint other than `https://testnet.rialo.io/`, invalid
addresses, weak sender keys, and identical sender/recipient. It verifies canonical
message serialization and the single-signer layout. The assembler rejects missing,
duplicate/extra, malformed, mismatched and cryptographically invalid signatures.
No unverified arbitrary transaction or replacement message can enter this interface.

The URL check is an **offline policy**, not independent chain-identity proof.
The caller-supplied config hash and timestamp are not authenticated or refreshed.
Network identity is not an added signed field. Before any future actual use, a
separate integration must establish Testnet RPC provenance and fresh hash/time,
show the recipient/amount/fee/network to the user, and check balance and fee.
This module is not a send authorization or a complete transaction execution workflow.

## Tests and remaining work

Tests instantiate synthetic Ed25519 keys only in memory and create test signatures.
No actual wallet is accessed, no keys/signatures are printed or saved, and no network
is used. Tests cover valid wire assembly, invalid signatures/lengths, wrong public
key, wrong signing key, missing/duplicate signatures, invalid addresses, identical
addresses, non-Testnet URLs and signatures over a different message or Base64 text.
Existing executable tests continue to check unsigned transfer contents and read-only
argument behavior.

CLI alias `default` integration and any external signer transport remain unverified
and unimplemented. No actual Testnet execution, balance/fee integration, TX confirmation
or user-facing external-signature import/export command is provided.

Next task: specify and verify an external signer's public-key / raw-message signing
interface, without extracting existing CLI wallet secrets.

## Validation for GitHub recording (2026-10-14)

- `cargo fmt --check`: passed.
- `cargo check --offline --locked`: passed.
- `cargo test --offline --locked`: 14 passed, 0 failed (7 library + 7 executable).
- `git diff --check`: passed.
- Existing dependency `proc-macro-error2 2.0.1` emits a future Rust compatibility
  warning; current build and tests pass.
- No real wallet access/signing, transaction submission, or Faucet execution.
