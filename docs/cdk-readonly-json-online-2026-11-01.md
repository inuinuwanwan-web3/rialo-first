# 2026-11-01 共通JSONレポート DevNetオンライン検証 PASS

## 目的・位置付け
既存READ ONLY照会の実DevNet応答をschema_version 1.0へ正常変換できるか確認した。追加開発はない。オフライン開発報告は `cdk-readonly-json-2026-11-01.md`、一意テスト68件成功（新規23・既存45）、失敗0件。以前の「オンライン未実施」はその記録時点の事実として保持し、本書を後続の確定結果とする。

## 実施結果
- 検証日：2026-11-01。レポート日時UTC：2026-11-01T05:59:48.109Z。
- 既存RPC：http://devnet.rialo.io:4100、getAccountInfoのみ1回、自動リトライ0回。
- CLI：devnet_accountの明示的な --bounded-http --json --execute 経路。
- 引数：既存成功記録の公開アドレス。新規探索・推測なし。version=0、dataSlice offset=0・length=4096、minContextSlot=null。不要なアドレス再掲載は省略。
- 全体timeout 10秒、接続・読み取り各3秒、HTTP本文上限1MiB（1,048,576バイト）。redirect・retry・圧縮は禁止した既存経路。
- HTTP 200、本文106バイト。JSON-RPC 2.0、request ID=1。Transportでversion・ID・result/error排他性を検証してからreceiptを生成した。ID整合性PASS。
- RPC正常応答、対象アカウントは不存在。HTTP成功・RPC成功・対象データなし・共通JSON変換成功を別々に確認。
- schema_version=1.0、network=devnet、operation=account_lookup、status=not_found。
- context.slot=6,568,878、data.account=null、error=null。
- JSON構文と必須項目・UTC形式・データ整合性の検証はすべて成功。
- transport=bounded_http、query_attempted=true。verification=not_assertedはレポート仕様通りで、外部の検証判定PASSとは別のフィールド。
- 終了コード0、stderr 0バイト。既存CLIの明示オプション経路で正常処理した。既存オフライン互換性確認を維持し、今回legacy全経路をオンライン再検証したとは主張しない。

## 判定根拠と公開証跡
**PASS**：実DevNet応答を共通JSONへ正常変換し、必須項目の整合性を確認した。not_foundを通信失敗とは扱わない。実在アカウント取得のPASSではない。

`cdk-readonly-json-online-2026-11-01.json` に必要な結果のみ抽出した。元の一時証跡をそのまま公開せず、不要な公開アドレスや一時環境情報を省略した。秘密鍵・認証情報・生Account Dataは含まない。

## 未検証事項・追加検証条件
実在アカウントのOwner・Executable・Data、他照会種別のオンライン変換、実環境の巨大応答は未検証。受信上限はHTTP本文に対する制限であり、JSON解析時・プロセス全体のメモリ上限ではない。

今回の範囲について追加照会は不要。別の照会種別を実利用へ展開するとき、レスポンス型・SDK・Transport仕様を変更するとき、または独立した存在根拠のある公開アカウントが別作業で利用可能になったときに限り、別途承認した限定検証を検討する。同じ対象探索・RPC照会は反復しない。

## 記録作業の安全性・終了
今回の正式記録では追加RPC・テスト再実行・コード変更を一切行っていない。AGP監視・Telegram通知・サービス・署名・送金への操作なし。無関係な未コミット変更を保持するため、分離した記録用作業コピーから本書と抽出JSONの2ファイルだけを公開する。
GitHub・Notion反映照合後に今回のフェーズを正式終了する。
