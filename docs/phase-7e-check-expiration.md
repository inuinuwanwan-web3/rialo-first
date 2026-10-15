# Phase 7E — check expiry and revalidation (2026-10-15)

## Existing design reviewed

Before this change, `src/pre_sign.rs` at commit 967c374 exposed `pre_sign_check`
(line 43), returning `CheckedTransfer` (line 28) with a read-only `CheckReport`
(line 18). The private core (line 81) checked the reviewed digest, canonical
message, RPC prefix, fee estimate, balance and prefix again. It had no expiry.
`src/lib.rs:38` constructs the immutable Phase 7B transfer; `:77` exposes exact
message bytes and `:83` assembles external signatures. The CLI remains read-only
(`src/main.rs:70`) and does not invoke the pre-sign library.

The existing RPC core is reused, not duplicated. Existing `pre_sign_check` keeps
its arguments and uses a default 30-second lifetime with `SystemClock`.
`pre_sign_check_with_clock` adds an explicit lifetime and injected clock.
`revalidate_before_signing(checked, clock)` independently revalidates the owned
checked state using the same fixed Testnet endpoint and the original fee limit.
Neither entry point is connected to the Phase 7C signer interface or invoked live
in this phase. No dependency changes were needed.

## Lifetime and clock semantics

`CheckReport` adds `checked_at`, `expires_at` (Unix epoch milliseconds) and
`network_config_prefix`. It retains the message SHA-256, fee estimate/limit and
success flags. The owned checked object also retains its private clock sample
and lifetime. It has no public constructor, deserializer, mutable accessor or Clone.

- Lifetime is configurable from 1 to 300,000 ms; default is 30,000 ms. These are
  local application policy limits, not protocol guarantees.
- `checked_at` is sampled **before** RPC observation starts; `expires_at` is
  checked_at plus the requested lifetime. Slow RPC consumes the lifetime.
- Valid interval is `[checked_at, expires_at)`: equality with the deadline fails.
- Both wall-clock elapsed time and monotonic elapsed time must be below lifetime.
- Reject clock read errors, nonpositive initial Unix time, backwards wall or
  monotonic movement, arithmetic overflow, and elapsed-time disagreement above
  1,000 ms. The tolerance permits small scheduling/clock adjustments but does not
  extend either expiration test. A backwards wall-clock sample is always rejected.
- SystemClock uses SystemTime and a process-wide OnceLock<Instant> origin. Injected
  clocks must use the same trusted monotonic origin throughout an object's life.
  Checked state is process-local and cannot be restored from serialized reports.

Clock injection is a trusted application/test dependency, not metadata supplied
by an external signer. A consistently wrong absolute clock, malicious clock that
lies coherently, or a clock jump and recovery entirely between samples cannot be
detected without independent time evidence. Absolute UTC correctness is therefore
unverified; do not describe this as authenticating host time. On observed anomalies
the operation fails closed. Runtime timeout remains 30 seconds.

## Revalidation and failure behavior

Revalidation consumes the old checked object, including on failure. It first checks
old state locally, then calls the same prefix/fee/balance core with the same raw
message digest and original fee limit. It checks expiry before and after RPC and
refuses renewal if the **old** result expired during revalidation, even when the
new observation would otherwise be fresh. Successful fresh results get a new
observation-start timestamp and the same lifetime. An already expired result must
go through new review/initial checks, not be silently renewed.

| Condition | Behavior |
| --- | --- |
| Deadline reached / invalid clock | Reject, including expiry during RPC |
| Raw message, recipient, amount, signer changed | SHA-256 mismatch rejects before RPC |
| Internal sender differs from message fee payer | Reject local state |
| RPC config differs from signed prefix | Reject; no automatic message rewrite |
| Fee rises but stays within original limit | Recompute total, recheck balance, return updated estimate |
| Fee exceeds limit / missing or invalid quote | Reject |
| Balance insufficient / total overflow | Reject |
| Any RPC error | Reject; no checked state returned |
| Invalid flags, deadline, prefix, digest, inconsistent report | Reject local state |
| Missing whole checked result | Cannot call the typed API; no optional/default report fallback |

Changes to recipient, amount and signer are covered because those fields are in
the signed raw message. Tests alter canonical decoded messages and retain the old
digest to verify rejection; production code cannot mutate these private fields.
Report consistency checks defend invariants, not hostile code with arbitrary
memory-write access. An arbitrary standalone CheckReport cannot construct checked
state. `report()` and `signing_message()` remain diagnostic accessors and may expose
expired observations; they are not authorization. `validate_current` checks local
expiry/integrity only and does not substitute for fresh RPC revalidation.

## Offline validation

Existing 24 tests retained; 18 new tests cover:
valid interval; exact expiry boundary; invalid time/TTL and overflow; clock rollback
and drift; clock read failure; changed message/recipient/amount/public key; changed
network prefix; increased fee within limit; fee-limit excess; insufficient balance;
RPC failures/missing fee; inconsistent state; initial checks expiring during RPC;
old results expiring during renewal; successful fresh revalidation.

`cargo fmt`, `cargo fmt --check`, `cargo check --offline --locked`,
`cargo test --offline --locked` passed: **42 tests passed, 0 failed** (35 library,
7 executable). `git diff --check` passed. The pre-existing proc-macro-error2 future
Rust compatibility warning remains. New tests use only synthetic public keys and
in-memory clocks/RPC mocks. Existing Phase 7B synthetic signing tests are unchanged.

## Remaining limitations and next task

Time validity does not guarantee network state or fees remain unchanged. Prefix,
fee and balance observations are non-atomic; a successful return can immediately
become stale. Fee estimates are not signed fee caps. Endpoint/TLS trust follows
the CDK's defaults; no independent chain-ID proof or live Testnet validation was
performed. Transaction valid_from is unchanged and is not the local checked_at;
chain-level transaction age is not validated here.

Phase 7C external signer transport, request generation, expiry enforcement while
waiting for a signature, and response handling remain unimplemented. Phase 7B
assembly is still a separate offline primitive, not a globally gated execution path.
Next task: design the Phase 7C request/response lifecycle so it consumes fresh
checked state and rejects responses after expiration without bypassing revalidation.

Existing success records and `docs/phase-7-signing-investigation.md` were not changed.
Real-wallet signatures, TX submissions, Faucet calls, commits and pushes: all zero.
