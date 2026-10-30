# 2026-10-30 Rialo CDK DevNet状態照会：開発・オンライン検証完了

**PASS：今回追加したDevNet読み取り専用状態照会の範囲で完了。**
公式API型で個別TX slotとcontext.slotを分けて保持し、既存の状態分類を再利用する。
履歴・Journalの再構築、署名・送金、長時間監視の資格化は含めない。

## 確認した実装

| ファイル | 内容 |
|---|---|
| src/status_readonly.rs | getSignatureStatuses専用adapter、公式応答型の保持、入力順の署名対応、既存observe_statusによる分類 |
| src/bin/devnet_status.rs | DevNet専用CLI。既定は入力確認のみ、--executeで初めて接続 |
| src/lib.rs | status_readonly module公開の1行追加 |
| Cargo.toml | 新bin追加後も従来cargo runの対象を保つdefault-run。既存ローカルの同じ1行を必要な互換設定として公開対象へ再利用 |
| tests/status_readonly.rs | 新規9件の合成データによるオフラインテスト |

公開署名1〜256件、重複拒否、timeout1〜30秒（既定10秒）、1要求のみ。
書き込みAPI・秘密鍵・ウォレット・自動再試行・ポーリング・状態保存なし。
不正/null/件数不一致/未対応versionを正常結果へ変換しない。
未実行の観測はUnknownを維持し、absenceやfinalityを推測しない。

## オフライン検証：既存結果を再利用

| 区分 | 成功 | 失敗 |
|---|---:|---:|
| 新規状態照会 | 9 | 0 |
| 既存Monitor本体 | 14 | 0 |
| 既存query | 15 | 0 |
| 既存CLI | 12 | 0 |
| 既存7Z結合 | 11 | 0 |
| 登録テスト合計 | 61 | 0 |
| 別枠CLI確認 | 3条件 | 0 |

既存回帰は52件であり、依頼文の525件を裏付けるログはないため525とは記録しない。
新規9＋既存52＝61件。CLI3条件は登録テスト数に加算しない。
Linux seccompによるネットワーク遮断、専用HOME/TMPDIR、cargo --offline --locked、
並列2で実行した既存ログのhash・個別件数・対象source hashを照合した。
今回の完了記録のために既存テストやオンライン照会を再実行していない。
全suite・署名系テストの成功を主張しない。
公開用の分離ソースはネットワーク遮断下のcargo test --offline --locked
--test status_readonly --no-runでコンパイル成功。テスト実行は0件。

## 実DevNetオンライン検証

検証開始：2026-10-30 11:44:58.952794 UTC（20:44:58.952794 JST）。
以下は同セッションのtool実行出力からの選択転記。独立した生HTTP応答ファイルは保存していない。

| 項目 | 確認結果 |
|---|---|
| SDK / API型 | rialo-cdk 0.18.1 / rialo-api-types 0.18.1 |
| RPC | http://devnet.rialo.io:4100（固定版公式SDKのURL_DEVNET） |
| getTransactions | version=0、config.limit=1、1要求、HTTP 200、1件取得 |
| getSignatureStatuses | 取得した同じ公開署名1件を新CLIで1要求、処理成功、終了コード0 |
| 合計要求 / 自動再試行 | 2 / 0 |
| 一覧のcontext.slot | 5,539,655 |
| 一覧のTx slot / blockHeight | 5,539,656 / 5,539,656 |
| 状態応答の個別slot | 5,539,656 |
| 状態応答のcontext.slot | 5,539,660 |
| API version | 0.23.0-alpha.0 |
| executed / err | true / null |
| 分類 | Succeeded / ExecutedWithoutError |
| 照合 | 公開署名対応・一覧slotと個別slot・接続先の全一致 |

公開Tx署名：

```text
32JWJXgTm9rKeL4sLThAVR8yaBQ3K562mXv87TSgKo3zY545Ki9xzDQuFEPb3ohC6pY4EmwiXbFMCZVucdaZDZa8
```

依頼文のAPI version「0.23.6-alpha.0」は実行出力と異なる。
ログにある「0.23.0-alpha.0」を採用した。SDK版とサーバーAPI版は別項目である。
一覧contextよりTx slotが1大きい値も元の観測として保持し、原子的snapshotとは解釈しない。
状態応答は個別slotとcontext.slotが異なるため、SDKのcontextへの置換を介さず
新機能が個別slotを保持できたことを実応答で確認できる。

## 完了境界と引き継ぎ

- 今回のDevNet状態照会の正常系・1件・固定SDKの使用経路はPASS。必須残作業なし。
- 全RPC、実サーバーでの異常応答・障害復旧、複数署名のlive互換性は未検証。
- archive保持期間、過去と現在のledger継続性、finality・確認数は証明していない。
- Phase 7Zの旧Testnet wire状態全体の未確認事項を、今回のDevNet結果で遡及解消しない。
- Phase 8Nの実ENOSPC、容量枯渇時復旧、長時間運用資格化は別の未完了事項。
- 同じ署名探索・オンライン照会・オフライン回帰を繰り返す必要はない。
- 次候補は公開アカウントのowner/executable/data長等を調べる読み取り機能。
  実装前に公式getAccountInfo/getMultipleAccounts型と実用要件を照合する。

## 公開・保全範囲

元の作業ツリーから今回のsource/testと必要な記録だけを分離チェックアウトへコピーする。
旧Journal module、Phase 8A〜8Nの未commit実装・実行環境、AGP/Telegram関連変更は含めない。
新binに必要なdefault-runだけを既存の同一設定から再利用し、依存版・Cargo.lockを変更しない。
既存ローカル変更・未追跡成果物・AGPコード/設定/serviceはhash照合して保全する。
個人情報・秘密情報・実認証・生応答本文・ホスト固有log/pathは公開しない。
この記録作業によるAGP操作、Telegram送信、新規RPC照会、署名・送金は0件。

詳細は[開発時報告](cdk-devnet-status-2026-10-30.md)、
[オフライン結果](cdk-devnet-status-results-2026-10-30.json)、
[オンライン選択結果](cdk-devnet-status-online-2026-10-30.json)を参照。
