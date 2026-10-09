# 改善の優先順位と参考資料

2026-10-09に、CoreXの実装とキーボードファーム7系統、他ジャンルのREADME 11件を比較しました。利用者の操作は[使い方](usage.md)、開発時の検査は[BUILDING.md](../BUILDING.md)にまとめています。このページは、何を改善し、何をまだ追加しないかを残す開発用の記録です。

## 今回直すところ

| 問題 | v0.9.6での変更 | 確認方法 |
| --- | --- | --- |
| Vialの3つの接続先が同じ省略名に見える | `BLE 1`・`BLE 2`・`BLE 3`に区別 | 表示名だけの差分で、キーコードと配列は一致 |
| `Reserved`という名前で左右登録を解除できてしまう | `Reset split pairing (hold 5s)`と説明を表示 | User7の番号と5秒長押し処理を維持 |
| 依存ライブラリがBLEの接続用秘密情報をログへ出す | 対象ログを文字列化前に除外。登録情報全体を出すログも削除 | 秘密情報の整形処理が呼ばれず、通常の接続・センサーログが残るテスト |
| 電池表示の回帰テストが通常の検査から外れている | 既存テスト、電池、ログ、配布UF2を一つの入口とCIで検査 | `./tools/test.sh`。実機検査は別に記録 |
| 文書の旧画像・更新履歴・再登録の説明が食い違う | 現行の配布定義から図を作り、使用・更新・純正左導入の入口を分ける | 配布物との照合、リンクと画面表示の確認 |

感度の選択肢、クリック操作、AML、キー配列、Bluetoothの登録形式は増やしていません。設定名やログの修正で新しい操作を覚える必要がない範囲に絞っています。

## 次に確かめるところ

### 1. 別の左右セットへ接続しないこと

現在の実装には、既知の相手への接続がタイムアウトすると再探索へ戻る経路があります。右側の[接続処理](../source/corex-rmk-upstream/rmk/src/split/ble/central.rs)と左側の[広告処理](../source/corex-rmk-upstream/rmk/src/split/ble/peripheral.rs)で確認しました。別のセットへつながる現象を実機で再現したわけではありません。

販売前に、2セットを近くに置き、一方の片側を切った状態から元の相手だけへ戻るか確認します。必要な要件は「通常の再接続では登録相手を変えない」「相手の変更は明示した操作で行う」の2点です。修正する場合は、初回登録と故障した片側の交換手順も一緒に検証します。

### 2. Vialの重要な設定操作に物理確認を使うこと

`unlock_keys`はありますが、現在の[右のビルド設定](../source/corex-rmk-pair/right/Cargo.toml)には`host_lock`がありません。[Vial処理](../source/corex-rmk-upstream/rmk/src/host/via/vial.rs)では、その場合に常時Unlockedを返します。

第三者へ配布する構成では、[Vialの物理ロック](https://get.vial.today/docs/security.html)を次の候補にします。ただし、RMKのfeature追加は設定保存形式に影響します。有効化だけで済ませず、旧版の配列・マクロ・登録情報を保つ更新と、解除できないときの復旧を検証してから採用します。

### 3. 待機時の電池消費と最初の入力

RMKのスリープ時間は未指定で、既定値は0です。PAW3222は15 ms周期で状態を確認し、センサー自身の省電力設定を使っています。電池残量が表示できることと、電池持ちがよいことは別です。

先にUSBなしでの待機時・操作時の消費と、長時間放置後の最初のキー・ボール操作を測ります。必要な要件は「待機消費を下げる」「復帰時に最初の入力を失わない」。実測前に深いスリープや電源遮断を追加しません。

### 4. 接続待ちが見て分かること

状態LEDは未対応です。純正の全パターンを移植する前に、接続待ち・接続成功・センサー認識失敗のうち、利用者の判断に必要なものを選びます。実装には基板の極性と点灯確認が必要です。充電回路のLED表示と、ファームの表示を混同しないようにします。

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
