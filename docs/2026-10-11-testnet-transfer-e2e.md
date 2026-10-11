# Rialo Testnet Transfer E2E — 2026-10-11

## Goal

Verify a normal RLO transfer on Rialo Testnet from an existing CLI wallet to the
user-designated Playground custodial wallet address.

Result: **SUCCESS for on-chain execution and RPC balance verification.**
Playground UI verification is separate and remains unverified.
CLI: Rialo 0.18.1-2e1fdefe31cc. CDK/API reference: Rialo CDK 0.18.1.

This record summarizes RPC/CLI observations from the execution session. The TX IDs
below allow independent on-chain lookup; no raw response files are included.

## Network

Testnet RPC: https://testnet.rialo.io/

The user observed Playground browser `getBalance` requests to this endpoint,
returning 9,000,000,000 kelvin and displaying 9 $RIALO before the transfer.
The same destination address previously had 2,999,985,000 kelvin on DevNet at
http://devnet.rialo.io:4100. These balance observations were not simultaneous.

Read-only comparison on 2026-10-11:

| Observation | Testnet | DevNet |
| --- | --- | --- |
| configHashPrefix | 11774740490753875893 | 18190376740211973295 |
| Block height (10:11:51–52 UTC) | 31296108 | 29209981 |
| blockTime at height 29209900 (10:12:53–54 UTC) | 1791420752 | 1791713470 |

The endpoints were treated operationally as separate ledger states. Genesis hash
and chain ID were not obtained: strict chain-identity proof remains incomplete.
The config prefix is not presented as an immutable genesis hash.

## Faucet preparation

The sender's initial Testnet balance was 0 RLO. Requests used the Testnet
`requestAirdrop` RPC with explicit public address and integer kelvin amounts.
No recipient-side user signature was required. Successful requests produced
funding transactions. This was Testnet working capital.

| Request | Before | Requested | Result | After |
| --- | --- | --- | --- | --- |
| Initial | 0 RLO | 1,000,000 kelvin = 0.001 RLO | executed=true, err=null | 0.001 RLO |
| Additional | 0.001 RLO | 10,000,000 kelvin = 0.01 RLO | executed=true, err=null | 0.011 RLO |

Each request was made once, without retry.

Initial funding TX (execution slot 31312642):

`5TiohMeL3ryJJPQKqJ7Dxr7oUAj2ZNnu9qRv1m3Sws6omMQDT6x9vvUtNbthVubd1Nvdeb6dkGdMJ6uGPHC8myz7`

Additional funding TX (execution slot 31317728):

`5o9vrT8LrU3U6Gith4FxCRzHTrHNCm5xJ9AWQCQB8A9yxaXgu1oWP4xdVJhmPD2P8oRB4oouY1StAjSnWkQKmFG8`

Final preparation balance: 11,000,000 kelvin = 0.011 RLO.
Current Faucet rate limits and cooldown were not established by these successes.

## Fee verification

`getFeeForMessage` returned 5,000 kelvin = 0.000005 RLO for a transfer of
1,000,000 kelvin = 0.001 RLO. Required total: 1,005,000 kelvin = 0.001005 RLO.
The initial 0.001 RLO balance alone was short by 5,000 kelvin.

The pre-transfer fee query on 2026-10-11 at 11:04:34 UTC again returned 5,000
kelvin (context slot 31319400). This is an observed quote, not a permanent fee.
The Testnet message was constructed in memory using the CDK 0.18.1 wire layout;
the repository application itself remained DevNet-only.

## Transfer

Sender / fee payer (existing CLI alias `default`):

`7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa`

Destination / user-designated Playground custodial wallet:

`FAyTs54yLpZ4KRHot8dVpJaoJJAibwkfEi9ck3NjHVc6`

- Amount: 1,000,000 kelvin = 0.001 RLO
- Actual fee: 5,000 kelvin = 0.000005 RLO
- CLI transfer invocation: 1
- Resend: 0
- Sender signature: 1
- executed: true
- err: null

TX ID:

`5h5JFQgEdyS6fJzVCugGxdbfcg7Wj6auUGmPuLPMV76u1vFENhAhWeuUK2UnrFGvW1nKTb9EXit7QsJqL8RushTs`

`getSignatureStatuses` confirmed success. `getTransaction` returned block height
31319418, the specified sender and destination, one signature, `meta.err=null`,
`meta.fee=5000`, and a successful System Program invocation
(`11111111111111111111111111111111`). Confirmation occurred at 11:04:39–41 UTC.

## Balance result

| Account | Before (kelvin / RLO) | After (kelvin / RLO) |
| --- | --- | --- |
| Sender | 11,000,000 / 0.011 | 9,995,000 / 0.009995 |
| Destination | 9,000,000,000 / 9 | 9,001,000,000 / 9.001 |

After-balances were read at 11:04:42–43 UTC (20:04:42–43 JST), context slots
31319453 and 31319463 respectively. Each matched the expected arithmetic.
The balance queries were separate observations, not an atomic snapshot.

## UI verification

Whether the Playground Account UI changed from 9 to 9.001 $RIALO is **unverified
at the time this record was created**. The confirmed 9.001 RLO is the destination's
RPC balance. RPC success and UI display success are separate findings.

## Important scope

This E2E transfer was signed and sent using the existing Rialo CLI.
It does not prove a transfer implemented and executed directly by `rialo-first`.
The application's Rust code still has no signing, `send_transaction`, Faucet
execution or transaction confirmation implementation.

No conclusions about incentives, evaluations or eligibility are made.
