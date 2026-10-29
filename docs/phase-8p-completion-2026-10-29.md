# Phase 8P完了記録：Telegram通知改善と実配送

## 判定と記録先

**PASS（Phase 8Pの有効化・専用配送テストの範囲）。**
AGP実装とテストの記録先は既存の
[rialo-agp-latch](https://github.com/inuinuwanwan-web3/rialo-agp-latch)。
本リポジトリには成果の参照記録だけを保存し、Rustの未commit変更や他Phaseの作業を含めない。

実装・検証記録のcommit：
[`e66b905d75004ee95087e1fdd906deb918698532`](https://github.com/inuinuwanwan-web3/rialo-agp-latch/commit/e66b905d75004ee95087e1fdd906deb918698532)。
詳細は同commit内の
[Phase 8P完了報告](https://github.com/inuinuwanwan-web3/rialo-agp-latch/blob/e66b905d75004ee95087e1fdd906deb918698532/agp-track-race-agent/docs/phase-8p-completion-2026-10-29.md)、
[選択した検証結果](https://github.com/inuinuwanwan-web3/rialo-agp-latch/blob/e66b905d75004ee95087e1fdd906deb918698532/agp-track-race-agent/docs/phase-8p-results-2026-10-29.json)、
[公開前オフライン試験](https://github.com/inuinuwanwan-web3/rialo-agp-latch/blob/e66b905d75004ee95087e1fdd906deb918698532/agp-track-race-agent/docs/phase-8p-offline-tests-2026-10-29.json)を参照。

## 確定した成果

| 項目 | 結果 |
| --- | --- |
| Telegram通知改善版 | 2026-10-29 20:49:57 JSTに有効化 |
| Telegram通知サービス | 個別承認により単独で1回再起動 |
| AGP監視サービス | 再起動せず継続稼働 |
| Telegram API | HTTP 200、ok=true |
| message_id | 8 |
| 送信日時 | 2026-10-29 21:35:19 JST（12:35:19 UTC） |
| 実HTTP送信 | 1回、自動再送0回 |
| スマートフォン受信 | ユーザーが今回の記録依頼で確認済みと明示。正確な受信時刻は未提供 |
| 通知カーソル | 39→39 |
| 未処理の新Trackイベント | 0→0 |
| 有効化前の観測データ | 979件すべて保持 |
| AGP自動参加 | BLOCKEDを維持 |

初回の専用テスト処理は計測ラッパーのTypeErrorで停止し、APIリクエスト0回。
標準ライブラリのクラス参照を書き換えない方法へ修正し、外部通信なしで検証した後、
新たな個別承認・別実行IDで1回送信した。過去の一回限りのテスト記録は解除していない。
API成功時の機械記録では端末受信は未確認だったが、その後のユーザー確認により受信済みと確定した。

## 実装・試験

既存の新Track観測・保存と読み取り専用制限を再利用し、通知側に独立した永続キュー、
試行履歴、安全な失敗時だけの再送、結果不明の保持、読み取り専用statusを追加した。
接続前失敗・明示的な429拒否は待機後に再試行し、送達不明は重複回避のため自動再送しない。

公開対象は通知コード、対応テスト、隔離試験ランナー、運用文書、完了報告・選択結果、
オフライン試験結果、既存開発状況文書へのPhase 8P追記の8ファイル。
公開予定のコードに対して、**全499件成功・失敗0件**を再確認した。
合成HOME・MCP設定、ネットワーク拒否、実ファイル・DBへのアクセス拒否を使用し、
既存の24時間仮想時計試験も実施した。実24時間の連続運転試験を意味しない。

元の生ログ・実HTTP応答本文・通知DB・cursor・実行claim・認証ファイル・個人情報・
ホスト固有serviceは公開していない。公開する変更は既存認証値との一致検査、
秘密鍵・Bot Token・個人メール・端末名・絶対ユーザーパスのパターン検査で問題なし。
実行履歴やホスト固有情報は必要な結果フィールドだけを選択して記録した。

## 完了扱いにしていない項目

- 実際の新Track公開を起点とする自動通知の実運用E2E。今回は専用メッセージによる共通送信関数の検証。
- 実障害・実429発生後の再試行復旧、送信中断時の実配送結果。
- 改善版有効化後のPC再起動復旧。既存のWindows起動タスク・WSL/systemd設定と過去の起動実績の確認まで。
- Season・Tournament・参加ルール等の独立した告知の監視。
- 公式参加ルール公開待ちの自動参加、本番レース参加、ask/guess、署名、送金。

GitHub記録作業ではサービス操作、Telegram追加送信、本番AGPへの書き込み、
通知DB・カーソルの変更、Windows設定変更、Notion記録を行っていない。
既存作業ツリーを保全する分離チェックアウトで、関連ファイルだけをcommitし、
既存mainブランチへforceを使わずpushする。
