# Phase 8N — 限定検証後の監査とENOSPC実行前計画

**調査・計画作成は完了。Phase8N全体はPARTIAL PASS（未完了）。5分間負荷試験へは移行不可。**
今回は既存Journal・AGPデータ・専用ext4へ書き込まず、読み取り確認と新規のローカル報告書/解析コード作成だけを行った。sudo、mount/umount/fsck、実ENOSPC、GitHub Pushは0件。

## 1. 既存の限定6項目を再確認

元のlimited-results.jsonとpostcheck.jsonを読み取り、6項目成功・予期しない失敗0を確認した。再実行はしていない。
A 作成/読取り、B atomic rename、C 整合性、D 書込み拒否後の旧commit保持、E OSの512byte上限、F 再open/復旧が対象。
DのRust CLI exit1は期待した拒否、Eはerrno27 EFBIG。**errno28 ENOSPCやFS全体の容量境界を確認した結果ではない。**
当時のWorker5件はwait回収済みで、今回の読取りでも同PIDの存在はなかった。PIDが将来再利用された場合、それだけで旧Worker残存とは判断しない。

既知の旧Journal `/tmp/phase8c-live-f9z7xgg4/journal/observations.json` のSHA256は、試験前/試験後/今回で一致：
`06597f6fcabd37df66e9a2ecaebc90d4c6a9766640a4a6eb4f2423548ca9305c`
これは照合した既知Journalの結果であり、PC内の全Journalを走査した主張ではない。既存Journalへ書き込んでいない。

限定試験の合成Journalはversion1、round3、records1。現在の内容hashも今回の証拠export間で一致した。

## 2. AGPの現在状態と取得継続

2026-10-26T13:13:10.235732 UTC（22:13 JST）の読取り：AGP active/running、PID188/start422。
直近監査1000行から235件のTrack通信を確認。最新成功pollは13:13:08.201801 UTC、読取り開始の約2秒前。
直近通信の平均19.81ms、最大36.80ms。これはtransport時間であり、サーバー生成時刻からの情報鮮度ではない。

systemdの今回確認値：NRestarts0、Result=success、ExecMainStart/ActiveEnterは2026-10-24 03:30:40 JST。限定試験時と同じprocess identityが継続している。
固定showとimmutableな監査読取りのみで、AGPの起動/停止/再起動/設定変更やリモート照会はしていない。

## 3. 検証前後の監査ログ

試験本体は2026-10-26T12:30:56.562800 UTC開始、約0.631秒。終了時刻はelapsedから算出した12:30:57.193965 UTCで、結果JSON保存完了時刻の厳密な測定ではない。
前後各5分の窓を含む848行を取得し、期間coverageとUTC順序を確認した。

| 観測 | 試験前5分 | 試験本体0.631秒 | 試験後5分 |
|---|---:|---:|---:|
| Track通信の完全pair数 | 88 | 0 | 88 |
| 平均transport時間 | 21.28ms | UNKNOWN | 19.00ms |
| 最大transport時間 | 38.44ms | UNKNOWN | 31.49ms |
| failed poll | 0 | 0 | 0 |
| read exception / anomaly | 0 / 0 | 0 / 0 | 0 / 0 |
| session end | 0 | 0 | 0 |

設定intervalは30秒。前poll終了から次poll開始までの16遷移を、既存watch_auditの基準（interval-0.5秒〜interval+10秒）で照合し、範囲外0件。最大の予定超過は約62.8ms。
停止・再起動・異常を示す記録や、前後の輸送時間悪化を示す値は取得範囲に見当たらなかった。
ただし試験中にAGP取得処理がなく、**同時負荷影響を直接測定したものではない**。通信時間が小さくなったことを性能改善や因果的な影響なしと解釈しない。
この監査窓は試験本体を中心にしたもの。後刻の報告書保存処理まで全期間のwire timingを集計したものではない。

同じ窓のservice journalは取得0件。単に記録がないことを履歴完全性の証明にはせず、主に監査DBと現在のprocess情報を根拠にした。子processの全終了履歴も収集していないため、任意の短命process異常を完全に排除する主張はしない。

### 読取り安全性

AGP DBはmode=ro&immutable=1。ロック/SHM/backupを作らず、主キー二分探索で窓を探し、最大10000行/SQLite実行予算0.8秒。全件scanやPRAGMA integrity_checkは行っていない。
sidecarがある場合や前後metadataが変化した場合は破棄してUNKNOWNとする。今回はmetadata安定・sidecarなしを確認。ただしtransactional backupと同等とはしない。
journalは最大300件、raw message/response/session ID等は報告に出さず、必要なcountだけを保存。
解析の合成4テストは4成功/0失敗。これはログ解析のテストで、限定6項目や実ENOSPC試験へ合算しない。

## 4. 現在のマウント・データ保全

専用mountは現在も/dev/loop0、ext4、rw,nosuid,nodev,noexec,noatime、UUID49035b22-d7cf-46a4-b089-c63e06922dee。
場所：`/home/pc/rialo-first/.phase8n-xkyabase/volume`
観測時のユーザー利用可能量28495872bytes（約27.18MiB）。raw image32MiBと、metadata/予約等を引いた利用可能量は別。
今回はmount/umountしていない。計画対象の新scope `phase8n-enospc-check-01` は未作成。

解除後も証拠を参照できるよう、新規reportとして以下をexportした。元fileは変更していない。
- phase-8n-limited-evidence-export.json：限定結果・postcheck・レポート・検証sourceとhash、26421bytes。
- phase-8n-synthetic-journal-export.json：今回の合成Journalと合成input、hash、3251bytes。
実wallet/private key/認証情報やAGPのraw payloadをコピーしたものではない。既存の実Journalを復元/更新する操作も行っていない。

## 5. 次回ENOSPC試験の対象と許可範囲

**現在は設計のみ。実行wrapperは未作成・未資格で、ENOSPC実行の承認もない。**
対象は同じ専用ext4内の、新規directoryだけ：
`/home/pc/rialo-first/.phase8n-xkyabase/volume/phase8n-enospc-check-01/`
既存のlimited検証scope、旧8C Journal、lost+foundは変更/削除しない。lost+foundには内容アクセスせずmetadataのみ扱う。

新scopeのjournal/inputs/logs/results/reserveを分離し、実Rust Journal・common lease・既存Landlock/seccomp・独立Guardianを再利用する。新たなMonitorやJournal保存形式を作り直さない。

次回承認に含めるべき内容：
- このvolume全体の非root利用可能量を一時的に0へ近づけること。
- 新規filler/reserveだけの削除/truncateによる復旧（後述の所有照合必須）。
- 満杯FSに依存しない診断の第二経路。新規のvolume外結果JSONを使うなら最大64KiB/O_EXCLとし、具体的なpath/commandを別途提示する。
- sudoや解除操作はデータ試験の承認に含めない。解除は別の個別承認。

### 開始条件

1. mountpoint/loop/backing file/UUID/32MiBを再照合。mounted-state用の確認を使い、旧「未mount・初期hash一致」preflightをそのまま再使用しない。image全体hashは正当な試験書込みでも変わるため、既存Journal不変の比較に使わない。
2. AGP activeで、直近成功・新規errorなし・実際のintervalを確認。監査coverage不足は開始拒否。
3. common leaseを取得し、新scopeは未存在。既存volumeは他のwriterが使っていないことを取得可能な範囲で確認。非協調processまでlockが禁止する保証はない。
4. volume空き16MiB以上、free inode64以上、host backing空き2GiB以上を確認。数値はこの小試験向け暫定基準で、長期運用値ではない。
5. 旧8C/limited scopeの既存file hash一覧を取得。snapshot/schema/上限とコードをreviewし、新規の停止・reserve・結果保存テストが通ってから個別承認を得る。

### 上限と監督

- filler書込み試行は最大32MiB、64KiB固定buffer、payloadを最大約1MiB/sへpacing。これはアプリによるpacingで、OSのI/O帯域上限ではない。
- sparse seek/truncate/fallocateで満杯を装わず、実buffer writeとfsyncで確認する。
- per-file RLIMIT_FSIZEは64MiBなどvolumeより大きい値を使い、EFBIGとENOSPCを混同しない。値は次回source/commandで確定する。
- Writer実稼働は最大60秒、retryなし、後片付け猶予10秒。host障害/D-stateで絶対期限を保証しない。
- Guardian/Writer/Rust子/読取り照会は固定個数とし、未登録子は中止。pidfd/親子関係/start identity/Run IDで停止対象を限定し、AGPを停止対象に含めない。
- 5秒ごとにservice状態を読む。監視取得不能が2回連続ならRialo側だけ中止。新たなAGP失敗poll/read exception/anomaly、identity変更、cadence許容外でも中止する。
- transportがmax(1秒, fresh baseline最大値の5倍)を超えた場合も保守的に中止する案。この閾値は暫定停止基準で、Rialo原因と断定するものではない。
- 自然発生したpoll_startを観測して開始できれば重複実行を評価する。AGPに強制RPCを行わず、重複サンプルがない場合は影響UNKNOWNのまま。

### 実施順序

1. 新規dummy Journalを正常作成・更新し、atomic renameとchecksum/identityを確認。次段階の入力は満杯前に全て準備する。
2. 新規reserve256KiBを実割当/fsyncしclose。作成時のdev/inode/owner/typeをメモリに記録する。疎fileをreserveと呼ばない。
3. 新fillerだけへpacingして書込み、errno28で即停止。上限到達せずbudget/timeを使い切った場合はUNKNOWN。空きinodeが残っていることも確認し、inode枯渇と区別する。
4. 満杯時の追加書込みは小さいcontrol probe1回と新dummy Journal更新1回に限定。Raw error28を採取し、Journal拒否後の旧active bytes/hash不変を確認。診断はRAMにも保持する。
5. Guardianが作成時identityと照合した新reserveだけを解放し、診断を保存。open FDを残すとblockが解放されないため、所有FDのcloseも確認する。
6. 同様に所有照合した新fillerだけをtruncate/unlinkしfsync。旧Journal/log/証拠を削除して空きを作らない。
7. 既存Rustのopen/検証を通して新dummyを再開・1回更新し、旧commitの状態履歴を確認。activeが不正なら自動修復/推測復旧せずFAILで停止。
8. 旧hash不変、残存0、空き回復（新成果物込みで開始時との差2MiB以内を候補）、AGPの少なくとも次の2成功pollを確認して報告する。

### 中止・復旧条件

EIO/EROFS、source不一致、old hash変化、監督喪失、予期しない子、時間/byte超過、AGP異常/観測不能で新規書込みを中止。
停止対象は新しく起動した所有確認済みRialo processだけ。reserve/fillerのpath/inodeが変わっていたら削除しない。許可外の操作が必要なら、その状態で停止して報告する。
reserve解放後も結果保存できない場合、診断喪失を隠してPASSにしない。新scope以外を掃除したり、rootでfsck/remountしたりしない。
停止後にもdirty pageのwritebackが残る可能性があり、共有PCでAGPへの影響ゼロを事前に保証はできない。それが許容できない場合は別の物理資源環境で行う。

## 6. アンマウントの必要性と条件

現在は書込みJobがなく、継続利用する明示計画がなければ、証拠export後に解除する方が誤操作防止になる。image自体は保持し、削除しない。
ただし**今回は禁止されており実行しない**。次の通常解除だけを別途個別承認の対象とする。

```bash
sudo --non-interactive /usr/bin/umount -- /home/pc/rialo-first/.phase8n-xkyabase/volume
```

解除前：結果export/hash、全owned Worker終了、取得可能な範囲のopen FD/cwd/使用process、対象mount/UUID/backing fileの一致を確認。端末が対象内をcwdにしている場合も先に対象外へ移す。
EBUSYなら強制/lazy解除や他processのkillをせず原因確認。通常解除後は元のmountpoint device/inodeへの復帰と対象loop関連0を読む。残るloopのdetachは別承認。
/tmp配下のbacking imageは再起動・自動清掃後の保持を保証しない。今のexportはそれに備えた証拠保全で、実運用backup方針を確立したものではない。
一旦解除した場合、次回の再mountも新しい個別承認が必要。

## 7. 5分間負荷試験の移行条件

- 実FS ENOSPC＋atomic rename＋保持/復旧を実証。
- process残存0、正常解除とloop解放を実証。
- 既存data hash不変、診断・abort処理の実効性を確認。
- AGPの自然な取得処理との重複を含む監査で失敗/遅延/空白を評価。情報不足はUNKNOWN。
- 本番起動経路でWorker全体のCPU/memory上限・独立leasekeeperを資格化。8I native backendは現在も停止中で、今回のstorage計画だけで解除しない。
- 5分間試験自体への明示承認。上記を満たす前に自動移行しない。

Phase8Nは限定6項目成功だが、総量ENOSPC、実FS full復旧、解除、同時負荷影響が未検証なので**PARTIAL PASSは合格を意味しない**。
次の見積り：実行wrapper/合成abort試験1〜2時間、承認後の短時間実測・観測・証拠保存0.5〜1時間。権限待ち・環境不具合対応や5分間向けresource統合は別。

## 8. 成果物・実行していないこと

- docs/phase-8n-enospc-readiness-observations.json：歴史窓848行・service journal・hash確認
- docs/phase-8n-agp-latest-readonly.json：直近取得継続確認
- docs/phase-8n-enospc-execution-plan.json：未実行計画、上限、条件、承認範囲
- docs/phase-8n-enospc-readiness-summary.json：集計とevidence hash
- 上記2つの限定証拠export、本書
- scripts/phase8n_log_review.py / test_phase8n_log_review.py：bounded解析、合成4テスト

新scopeは作らず、既存reportを上書きしない。今回行ったのは読取り調査と新しいローカル報告書作成だけ。実ENOSPC、sudo、mount/umount/fsck、AGP操作、既存Journalへの書込み、GitHub Pushは行っていない。
ここで停止する。
