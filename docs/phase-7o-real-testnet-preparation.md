# Phase 7O — Real Testnet Transfer Preparation Validation

## 承認と実行範囲

ユーザーが接続先・公開アドレス・検証金額1 kelvin・手数料上限6000 kelvin、
4種類の読み取りRPCを明示承認した後に実行した。
値は変更せず、次の2コマンドを各1回実行。追加照会・自動再送なし。
ネットワーク制限外の実行許可を得て実行した。

- ./target/debug/rialo-first --testnet-readonly --address 7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa
- ./target/debug/rialo-first --testnet-prepare-transfer --from 7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa --to FAyTs54yLpZ4KRHot8dVpJaoJJAibwkfEi9ck3NjHVc6 --amount-kelvin 1 --fee-limit-kelvin 6000

接続先：https://testnet.rialo.io/
From：過去CLI送金元、Phase 7Lの残高照会対象。
To：過去Playground受取先。Fromとは別アドレス。
根拠：docs/2026-10-11-testnet-transfer-e2e.md:74/78。
1 RLO=1,000,000,000 kelvin（ローカルCDK constants.rs:54）。

実行メソッド：getRecentValidatorConfigHash、getBlockHeight、
getBalance、getFeeForMessage。
手数料照会には指定公開アドレス・金額・valid_from・configを含む
未署名messageを使用した。実署名・TX送信は行っていない。

## 実Testnetの結果

| 分類 | 結果 |
| --- | --- |
| RPC接続 | 成功、CDK HttpRpcClientによる実接続 |
| config hash prefix | 3882139547858051996 |
| ブロック高 | 716181 |
| From残高 | 0 kelvin |
| 検証用金額 | 1 kelvin |
| 手数料上限 | 6000 kelvin |
| 手数料見積もり | 0 kelvin |
| Validation Result | PREPARATION BLOCKED |
| Blocking Reasons | 残高不足 |
| Signature Status | NOT EXECUTED |
| Broadcast Status | NOT EXECUTED |

読み取りコマンドはexit 0。準備コマンドは安全停止のexit 1。
既存pre_sign_checkはbalance < amount + estimate、すなわち0 < 1で停止した。
後続のamount+limit=6001 kelvin条件には到達していないが、
観測残高ではこの条件も満たさない。
残高不足によるBLOCKEDは、準備判定が安全側に動作した結果である。
十分な残高による実TestnetのPREPARATION PASSEDは今回未検証。

0 kelvinの見積もりは今回のmessageに対するRPC応答であり、
将来の見積もりや実徴収額が0である保証ではない。
過去の5000 kelvin見積もりとの差の原因は追加照会していない。
config prefixは独立したgenesis/chain IDの証明ではない。
各読み取りは同一ブロックの原子的スナップショットではない。
残高不足で停止するため、準備処理末尾の再prefix照合には到達していない。

## オフライン検証・変更

承認待ち段階でcargo fmt --check、cargo check --offline --locked、
cargo test --offline --locked、git diff --checkを実行して成功。
実測129件成功、失敗0（library119、binary10）。新規テスト0。
今回の実RPC実行前にcargo build --offline --lockedも成功。
これらのMock結果と上記実Testnet観測は区別する。
既存proc-macro-error2の将来互換性警告1種類が残る。

Phase 7Oでの変更は本書のみ。既存未コミットコードを保持。
既存のRPC失敗・prefix不一致・残高不足・上限不足・無効アドレス・
不正金額・overflow・fee欠損のMock試験は実行済み。
準備コマンド専用timeoutとfee RPCエラー注入の追加試験は未実施。
模擬TX結果timeoutのテストを準備timeoutの検証と混同しない。

## 未解決事項と次の最小作業

優先順位：
1. CLI/CDKの正式な実署名者接続と互換性を確認する（秘密鍵を取り出さない）。
2. 実送金を別途承認する際に資金・最新見積もり・金額・宛先を再確認する。
   今回は資金を補充しない。検証値1/6000を実送金条件と扱わない。
3. 本番send_transactionとstatus/transaction照会を既存ライフサイクルへ
   接続する変更案を確定する。結果不明なら再送せず停止する。
4. TX識別情報と署名状態の対応を保持する。履歴喪失/巻き戻しの限界は残る。

読み取り実検証と残高不足の安全停止確認を完了。
実ウォレット操作・秘密鍵/seed読取・実署名・TX送信・Faucet・
資産移動・Git commit/push・Notion記録は0回。
追加の実ネットワーク操作や次Phaseは開始せず停止する。

## GitHub記録前の並列テスト修正

再読み込み時のLockUnavailableは、並列Command生成がロック済みopen file
descriptionを一時的に子プロセスへ継承し、親のcloseだけでは直ちに
ロックが解放されない競合だった。
同じRust File::try_lock/Commandを使うローカル再現試験で、
close/reopen失敗2回と、同じファイルを指す子プロセスFD1件を/procで観測した。
一時ディレクトリはPIDとAtomicカウンタで分離され、共有パスが原因ではない。

cfg(test)内のrelease_test_storeで、セッション破棄後に明示unlockしてから
storeをdropする。通常のテストfixture終了・再open準備だけに適用。
本番StateStore、try_lock、診断、SIGKILL対象のクラッシュ経路は変更しない。
実プロセス強制終了によるロック解放・競合拒否のテストも維持した。
これは本番の同様の競合を解消したとの保証ではない。
本番プロセスが並列fork/spawnと即時reopenを行う場合は同じ制約が残る。

cargo fmt --check / cargo check --offline --locked /
cargo test --offline --locked / git diff --check成功。
さらに全体並列テスト20回すべて成功、各129件（合計2580件）、失敗0。
既存の将来互換性警告は変わらない。
Phase 7G以降の依存元で未記録だったPhase 7Fコード/文書も同梱する。
無関係なdocs/phase-7-signing-investigation.mdは保存対象に含めない。
