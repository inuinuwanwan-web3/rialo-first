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

## Separate Testnet E2E evidence

The successful 2026-10-11 Testnet transfer was signed and sent using the existing
Rialo CLI, **not this application's Rust code**. RPC results confirmed the transfer
and balance changes; the Playground UI balance update remains unverified.

See [Rialo Testnet Transfer E2E — 2026-10-11](docs/2026-10-11-testnet-transfer-e2e.md).
