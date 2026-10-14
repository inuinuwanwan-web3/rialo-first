# DevNet preparation only

This page describes the CLI executable. The separate [Phase 7B offline library](docs/phase-7b-external-signatures.md)
can verify external signatures and assemble Testnet transaction bytes, without
loading keys, signing, or sending. It is not invoked by the CLI.

Run `cargo run -- --from PUBLIC_SOURCE --to PUBLIC_DESTINATION` only after selecting
both public addresses. No destination is provided by default. Amount is fixed at
0.001 RLO (1,000,000 kelvin). `--rpc-check` preserves the original read-only
connection/config-hash check and adds the current block height.

The executable never loads a keyring, signs, or sends a transaction. Missing,
invalid, duplicate or unknown arguments stop before RPC access. Identical source
and destination are rejected. RPC errors, a 30-second overall timeout, unknown
fees, arithmetic overflow and insufficient balances cause nonzero exit.

Preparation uses only getBlockHeight, getRecentValidatorConfigHash, getBalance
and getFeeForMessage against the existing CDK URL_DEVNET. Displayed heights bracket
the observations; they are not an atomic balance/fee snapshot. Block bodies are
not downloaded. The sender/signer match and recipient ownership/account eligibility
remain unverified. All quotes must be refreshed before any separately authorized
future transaction. No approval or execution path is implemented here.

## Encoding evidence (installed 0.18.1 sources)

- rialo-api-types/src/messages/get_fee_for_message.rs:16 specifies a base64,
  bincode-serialized VersionedMessage; response value is optional at line 29.
- rialo-s-message/src/versions/mod.rs:153 uses bincode serialization; line 198
  serializes Legacy without an enum/version prefix.
- rialo-cdk/src/transaction/builder.rs:272 builds only the message. Its wire
  serializer begins at line 645.

The application decodes CDK output into VersionedMessage and checks byte-for-byte
canonical reserialization before Base64 encoding. Offline tests additionally check
payer, recipient, system program, instruction amount and balance/error boundaries.
This proves local 0.18.1 format compatibility, not live server fee acceptance.

Dependencies are pinned; base64, bincode and rialo-s-message were already present
transitively and are now direct dependencies. No secret input is accepted.

## Separate Testnet execution record

The statement above describes what offline format tests prove; it does not mean
that a live server rejects fee queries. Separate Testnet read-only queries returned
5,000 kelvin for a 0.001 RLO transfer on 2026-10-11. The application remains fixed
to DevNet; the successful Testnet transfer used the existing CLI, not this code.
See [the Testnet E2E record](docs/2026-10-11-testnet-transfer-e2e.md).
Playground UI balance reflection remains unverified.
