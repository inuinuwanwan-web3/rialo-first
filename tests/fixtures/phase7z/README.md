# Synthetic Phase 7Z response fixtures

`synthetic.json` contains eight fixture categories (A–H), not captured Testnet data.
The three signatures are repeated bytes encoded in base58. They are structurally
valid SDK identifiers, not signed transactions or evidence of real transactions.
No keys, seed phrases, wallets, or transaction construction are used.

- A: `success`, `detail_success` (execution evidence and detail are separate SDK responses).
- B: `failure`, `detail_failure`, `history_failure`. The history error is decoded
  into the SDK's typed TransactionError; SignatureStatus uses Option<String>,
  while detail metadata uses Option<Value>.
- C: `unexecuted`; no fabricated executed field is added to transaction detail.
- D: `not_found`: null detail is an invalid detail response, not proof of absence.
- E: `history`: three distinct IDs and heights, deliberately not height sorted.
- F: `page1`, `short_page`, `terminal`; cursor is the last raw page signature.
- G: `page2`: overlaps page1 at the boundary, retains a distinct third TX.
- H: incomplete detail/status, invalid signature/cursor, empty terminal page;
  RPC error and timeout are transport behavior injected by Mock, not SDK payloads.

Full pages supply only a candidate next cursor. An empty/short page ends this
bounded scan; neither implies globally complete retained chain history.
