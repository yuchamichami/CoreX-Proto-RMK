# 改善の優先順位と参考資料

2026-10-09に、CoreXの実装とキーボードファーム7系統、他ジャンルのREADME 11件を比較しました。利用者の操作は[使い方](usage.md)、開発時の検査は[BUILDING.md](../BUILDING.md)にまとめています。このページは、何を改善し、何をまだ追加しないかを残す開発用の記録です。

## v0.9.6で直したところ

| 問題 | v0.9.6での変更 | 確認方法 |
| --- | --- | --- |
| Vialの3つの接続先が同じ省略名に見える | `BLE 1`・`BLE 2`・`BLE 3`に区別 | 表示名だけの差分で、キーコードと配列は一致 |
| `Reserved`という名前で左右登録を解除できてしまう | `Reset split pairing (hold 5s)`と説明を表示 | User7の番号と5秒長押し処理を維持 |
| 依存ライブラリがBLEの接続用秘密情報をログへ出す | 対象ログを文字列化前に除外。登録情報全体を出すログも削除 | 秘密情報の整形処理が呼ばれず、通常の接続・センサーログが残るテスト |
| 電池表示の回帰テストが通常の検査から外れている | 既存テスト、電池、ログ、配布UF2を一つの入口とCIで検査 | `./tools/test.sh`。実機検査は別に記録 |
| 文書の旧画像・更新履歴・再登録の説明が食い違う | 現行の配布定義から図を作り、使用・更新・純正左導入の入口を分ける | 配布物との照合、リンクと画面表示の確認 |

感度の選択肢、クリック操作、AML、キー配列、Bluetoothの登録形式は増やしていません。設定名やログの修正で新しい操作を覚える必要がない範囲に絞っています。

## v0.10.0で対応する項目

- 通常の左右再接続で登録相手を変えず、変更は60秒のペアリング受付中に限定。
- 左右の登録解除を常時動く接続処理へ移し、相手不在でもローカルで操作可能にする。
- 再接続後に左の押下状態を同期し、最初の短い入力は上限付きで保持。
- BLEからUSBへ切り替える際、古い送信処理を打ち切り、キー・マウス・メディア・システム操作を解除。
- 右のVial物理ロックを有効化し、重要操作の各経路とキーテスターを検査。
- 接続・左右の電池低下を短いLED表示で通知。普段とスリープ中は消灯。
- 右親指の接続レイヤーを追加。接続先の長押しでは登録を消さず、消去は専用キーの5秒長押しに分離。
- 配布する左右を同じソースのv0.10.0に揃え、起動ログとVial応答から版を確認可能にする。

コードの検査と実機の確認は別に記録します。電池のみでの待機・復帰、PCスリープとの組み合わせ、複数セットでの試験、実消費電流と電池持ちは[検証状況](validation.md)を参照してください。自動テストの成功を電池寿命の測定結果には置き換えません。

## 今は追加しないもの

専用設定アプリ、独自ドキュメントサイト、センサーの多機種対応、ボールによる音量・ズームなどの追加モードは保留です。現在のVialとMarkdownで、設定・更新・復旧の説明は足ります。WatchdogやRMK全体の更新も、再現する問題と移行試験が揃ってから判断します。

## キーボードファームの比較

公式文書の最新版に書かれた機能が、そのままCoreXの固定したRMK版で使えるとは扱っていません。

| 一次資料 | 取り入れる点 | CoreXへの判断 |
| --- | --- | --- |
| RMK：[無線](https://rmk.rs/docs/features/wireless)・[保存](https://rmk.rs/docs/features/storage)・[Vial](https://rmk.rs/docs/features/vial_support) | PCの登録と左右の登録を区別。保存形式の条件を明示 | 現行の保存形式を維持し、解除操作は実際の意味で表示 |
| ZMK：[接続の問題](https://zmk.dev/docs/troubleshooting/connection-issues)・[設定](https://zmk.dev/docs/config/settings)・[電池](https://zmk.dev/docs/config/battery) | 接続できない場合と入力先が違う場合を分ける | 症状から案内し、全設定消去を最初の対処にしない |
| QMK：[Pointing Device](https://docs.qmk.fm/features/pointing_device)・[テスト](https://docs.qmk.fm/unit_testing) | AMLの解除・無効化条件と、実機不要の試験 | 操作モードの増設より、既存操作の回帰確認を優先 |
| Vial：[基本操作](https://get.vial.today/manual/first-use.html)・[Custom Keycode](https://get.vial.today/docs/custom_keycode.html)・[Security](https://get.vial.today/docs/security.html) | キーを選ぶ→値を割り当てる操作と、重要操作のロック | 感度欄も同じ説明に統一。物理ロックは移行込みで検討 |
| [Cornix純正マニュアル](https://docs.channel.io/jezailfunderjp/ja/articles/Cornix-%E6%97%A5%E6%9C%AC%E8%AA%9E%E3%83%9E%E3%83%8B%E3%83%A5%E3%82%A2%E3%83%AB-c1160246) | 接続・充電・登録・更新を分ける | 配列は参考にするが、CoreXの長押し時間や電源回路には実装値を使う |
| Charybdis：[機能](https://docs.bastardkb.com/fw/charybdis-features.html)・[書き込み](https://docs.bastardkb.com/fw/flashing.html) | 普段の操作と設定、通常更新と復旧を分ける | 多数のモードは移植せず、説明の分担を参考にする |
| Ploopy Adept：[README](https://github.com/ploopyco/adept-trackball/blob/master/README.md)・[Programming](https://ploopyco.github.io/adept-trackball/appendices/programming/) | 配布ファームを入口にし、開発手順を分ける | UF2配布を継続。別MCU用の全消去手順は取り入れない |

## 他ジャンルのREADMEの比較

| 一次資料 | 参考にした構成 | 今回の使い方 |
| --- | --- | --- |
| [uv](https://github.com/astral-sh/uv/blob/main/README.md) — Python環境 | 用途別の短い例と詳細へのリンク | 感度を少し下げる設定例 |
| [restic](https://github.com/restic/restic/blob/master/README.md) — バックアップ | 操作に続いて成功時の状態を示す | コピー後の再起動・Vial認識まで案内 |
| [Syncthing](https://github.com/syncthing/syncthing/blob/main/README.md) — 同期 | 初回利用・支援・開発の分離 | 完成セットを使う人と改造する人の入口を分ける |
| [LocalSend](https://github.com/localsend/localsend/blob/main/README.md) — 転送 | 入手経路付近の互換性と症状別対処 | ダウンロード場所で左右の対応を示す |
| [Immich](https://github.com/immich-app/immich/blob/main/README.md) — 写真管理 | 画面で機能を示して導入へ案内 | 現行設定と一致する図を使う |
| [PrusaSlicer](https://github.com/prusa3d/PrusaSlicer/blob/master/README.md) — 3Dプリント | 配布物と自ビルドの導線を分ける | 利用者へCargoの準備を要求しない |
| [Home Assistant](https://github.com/home-assistant/core/blob/dev/README.rst) — 家の自動化 | 少数の目的別リンク | README冒頭を使用・更新・純正左導入へ分岐 |
| [Zed](https://github.com/zed-industries/zed/blob/main/README.md) — エディタ | 対象・導入・開発の短い区分 | 対応機材を具体名で示す |
| [Jellyfin](https://github.com/jellyfin/jellyfin/blob/master/README.md) — メディア配信 | 導入・支援・開発を用事で案内 | 解決しないときの報告先を置く |
| [ripgrep](https://github.com/BurntSushi/ripgrep/blob/master/README.md) — 検索 | 初期動作と例外の明示 | AMLでJ・K・Lが変わることを早い段階で説明 |
| [RustDesk](https://github.com/rustdesk/rustdesk/blob/master/README.md) — 遠隔操作 | 配布物と開発の区別 | 長いビルド本文はREADMEへ移さない |

文の整理には[GitHubのREADMEガイド](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-readmes)、[Googleの手順ガイド](https://developers.google.com/style/procedures)、[SmartHRのライティング方針](https://smarthr.design/products/contents/writing-style/)も参照しました。各プロジェクトの長さや口調を模倣せず、CoreXを受け取った人の作業に必要な部分だけを採用しています。
