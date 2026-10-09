# ファームウェアをビルドする

使うだけならビルドは不要です。[READMEのダウンロード](README.md#1-ファームをダウンロードする)と[書き込み手順](docs/flashing.md)を使ってください。右は CoreX 用、左は純正 Cornix 左用です。

利用案内の対象は左右v0.10.1です。`main`には右v0.10.2の比較試験が残っているため、利用版を再ビルドするときは[v0.10.1タグのソース](https://github.com/yuchamichami/CoreX-Proto-RMK/tree/v0.10.1)を取得してください。配布物と試験記録の一覧は[開発資料](docs/development.md)にあります。

## 必要なもの

- Rust **1.95.0** と `thumbv7em-none-eabihf` ターゲット
- `llvm-tools-preview`、`flip-link` **0.1.12**
- Python 3.11以降、Git、C/C++ ビルド環境と libclang（Nordic のバインディング生成に使用）
- 初回の Cargo 依存関係取得用ネット接続

この配布版は macOS / `x86_64-apple-darwin` ホストでビルド確認しています。スクリプトはホスト名を Rust から取得します。Windows では WSL 等の POSIX シェル環境を使用してください。

```sh
rustup toolchain install 1.95.0 --profile minimal
rustup target add thumbv7em-none-eabihf --toolchain 1.95.0
rustup component add llvm-tools-preview --toolchain 1.95.0
cargo +1.95.0 install flip-link --version 0.1.12 --locked
```

macOS の C/C++ 環境は Xcode Command Line Tools、Linux ではディストリビューションの `clang` / `libclang-dev` 等が必要です。環境によっては `LIBCLANG_PATH` の設定が必要になります。

## ビルド

リポジトリのルートで実行します。

```sh
./build.sh right  # CoreX 右・PAW3222
./build.sh left   # 純正 Cornix 左・peripheral
./build.sh both   # 両方。引数省略時も両方
```

出力先は `build/firmware/`。ファイル名は `CoreX-Right-v<版>.uf2`／`CoreX-Left-v<版>.uf2` とし、[firmware/manifest.json](firmware/manifest.json) から取得します。既存の配布用 `firmware/` は上書きしません。ELF と Cargo キャッシュは `build/target/` に残り、Git 対象外です。キャッシュを別の場所へ置く場合は `CARGO_TARGET_DIR` を指定できます。

```sh
python3 tools/verify_release.py --rebuilt --side right  # 右だけビルドした場合
python3 tools/verify_release.py --rebuilt  # 両方ビルドした場合
./tools/test.sh              # 通信・設定・復帰動作と、配布UF2をまとめて検査
./tools/test_ble.sh          # 電池・USB・スリープ・入力のホストテスト
```

`test.sh` は実機を操作しません。BLEのテストもPC上で実行し、ホストの種類を自動判定します。RMKのモック時計と通信キューがテスト間で干渉しないよう、各テストを別プロセスで実行します。GitHub Actionsでは同じコマンドをUbuntuで実行します。初回はテスト用の依存ライブラリを取得するため、ネット接続が必要です。実機でのキー入力・電池駆動・再接続は[検証状況](docs/validation.md)に別途記録します。

`verify_release.py` は同梱 UF2 の SHA-256 と、再ビルドした UF2 のターゲット・アプリ領域・ベクタテーブル・ローカルパスの混入を検査します。Cargo メタデータがあれば設定スキーマも検査します。キャッシュを外部に置いた場合は `--target-dir <CARGO_TARGET_DIRの場所>` を指定できます。`--rebuilt` を省くと同梱 UF2 だけを検査します。

配布版とのハッシュ一致は参考情報として表示し、不一致だけでは失敗にしません。配布を作成した場所で厳密に照合する場合は `--require-identical` を付けます。この指定では、保存形式を確認するビルドメタデータが見つからない場合も失敗にします。対象を片側に絞る場合は `--side right` または `--side left` を指定します。

左右は同じ版・同じソースからビルドします。リリースタグでその版のソースを固定し、左右それぞれのファイル名・ハッシュ・書き込み範囲を `firmware/manifest.json` に記録します。起動ログに左右とCoreXのバージョンが出ます。右はVialのファームウェア版照会にも同じ版を返します。

**別のチェックアウト場所からのビルドは確認していますが、バイナリの完全一致は保証しません。** 同一ホスト・同一ソースでもチェックアウト場所が変わると Cargo / コンパイラのメタデータ等が変化し、生成物のハッシュが異なることを確認しています。異なるホスト、C ライブラリ、追加フラグでも変化します。公開用 UF2 の照合には同梱 `SHA256SUMS` を使い、手元でビルドした UF2 のハッシュとは区別してください。

## 初期キーマップ

右の初期配列は、メーカー配布のCornix設定ファイルから生成します。原本と差分は[keymaps/README.md](keymaps/README.md)を参照してください。v0.10.0では左右の接続処理も変わるため、両側を更新します。

```sh
python3 tools/default_keymap.py          # 右TOMLの配列とVial用プリセットを生成
python3 tools/default_keymap.py --check  # 生成物の一致を確認
```

## ソース構成

- `source/corex-rmk-pair/right/`：右の配列、Vial定義、PAW3222、感度・スクロール・AML。
- `source/corex-rmk-pair/left/`：純正Cornix左のファームウェア。
- `source/corex-rmk-upstream/`：RMKの4クレートとCoreX用の変更。
- `tools/git-metadata/git`：設定の保存形式を維持するためのビルドメタデータ固定。
- `tools/make_uf2.py`：左右のアプリ領域を検査し、UF2を生成。

左右の `Cargo.lock` を同梱し、ビルドは `--locked` で行います。キーボードのマニフェストから RMK への参照はリポジトリ内の相対パスです。公開 RMK の最新版へ自動追従しません。

### BLE送信出力

v0.10.1では、左右それぞれの `keyboard.toml` の `[ble]` に `default_tx_power = 8` を設定します。[nRF52840の最大送信出力](https://www.nordicsemi.com/Products/nRF52840)である `+8 dBm` を、[コントローラの送信全体の既定値](https://nrfconnectdocs.nordicsemi.com/ncs/latest/nrfxlib/doxygen/html/group__sdc_gae72e34ed7d6ecc33442223c9d8ffb56b.html)として指定します。PCとの通信と左右間の通信が対象です。

2M PHY対応を有効にしない既存設定 `use_2m_phy = false` は維持します。以前の送信出力はコントローラ既定値を使っており、数値は確認していません。ケース内での通信と消費電流の実測状況は[検証状況](docs/validation.md#v0101)を参照してください。

### 電池残量の取得

右の `keyboard.toml` の `[split.central]` で、電圧の入力を `P0_04`、分圧比を `2000 / 3000` に設定しています。`main.rs` で `P0_31` をHighにし、分圧回路が安定するまで350 ms待ってから測定を始めます。電圧から推定した残量を、BLE Battery Serviceの主バッテリーとして送信します。

接続時、残量の読み出し時、通知の購読開始時にも保存済みの測定値を反映します。PCが後から通知を購読した場合も、残量が次に変わるまで待つ必要がないようにしています。

標準のBattery Serviceは右の1つだけを公開し、Battery Levelの読み出しと通知、CCCDで構成しています。左からの残量受信は維持し、PCへはCoreX独自UUIDのサービスで提供します。左の値はPCの標準残量表示には使いません。

Battery Serviceの読み出しと通知登録は暗号化前にも受け付けます。HIDとVialのアクセス条件は変更していません。

v0.9.5でGATTの構成とハンドルを変更しました。v0.9.4以前から更新したPCは一度登録をやり直します。v0.9.5以降の更新ではGATTを変更していないため、再登録は不要です。[利用者向けの更新手順](docs/flashing.md#corex-を更新する)

### スリープとPAW3222

右の `keyboard.toml` の `[rmk] split_central_sleep_timeout_seconds = 300` で、無操作5分の待機を有効にしています。これはCoreXの初期値です。純正Cornixの待機時間を測定した値ではありません。BLEホストのサスペンド指示や広告終了でも待機に入り、押下中のキーがあれば延期します。

PAW3222はMOTIONのLOWレベル割り込みで読み取りを始め、v0.10.1では連続操作中の間隔を最短15 msに制限します。通常5秒・スリープ中30秒の通信確認も残し、IRQが戻らない場合でも再確認できます。設定確認の50 msタイマーは廃止し、Vialの変更通知で更新します。センサーの休止モードは維持し、正常な復帰ではリセットや最初のデータの読み捨てをしません。ID異常時だけ再初期化します。

左右が接続済みなら、その接続を保持して省電力の接続パラメータを使います。切れている既知の左は、スリープ中も2秒の接続試行と4秒の休止を繰り返して再接続を待ちます。未登録の相手の探索はスリープ中に止めます。左の切断時には、その左で押下中だったキーだけを解放します。

USBサスペンド中の最初のレポートは復帰を待って送信します。ホストが5秒以内に復帰・受信しない場合は入力を破棄し、復帰時に押下状態を解除します。PCが起きない間の入力を無期限に貯める機能ではありません。リセット・抜き差し・出力先切替をまたぐ古い入力も破棄します。

実際の消費電流・電池持ちは未測定です。割り込み待ちへの変更と、電池寿命の実測値は区別してください。

### 診断ログ

右をUSB接続すると、CDCシリアルポートからセンサーの認識・BLE接続・電池の通知状態を読めます。`PAW3222 J4 ready`はセンサー認識成功、`expected 30`はセンサーID不一致、`lost valid ID`は通信を失って再初期化している状態です。ログの正常表示だけで入力や残量表示の実機確認を代用しません。

右v0.9.6以降では、BLEの接続用秘密情報を含む依存ライブラリのログを、文字列化する前に除外しています。v0.10.0の左右にはこの修正が入っています。旧版の右と左v0.9.0はこの修正を含まないため、公開のIssueへ未編集のログを添付しないでください。通常の問い合わせには、[報告する情報](docs/usage.md#解決しない場合)だけで十分です。

## 既存設定との互換性

RMK はバージョン・コミット情報・feature・設定容量等からストレージスキーマを計算します。変更するとキーマップやペアリング情報の再初期化が起きます。この版は元の RMK コミット `8a6889854fb996be592c55075b385234133e1772` と既存 feature を保持し、スキーマを **`0xA805CEFB`** に揃えています。

**直接 `cargo build` を使わず、ルートの `build.sh` を使ってください。** リポジトリを新しく commit しただけで RMK のスキーマが変わらないよう、専用の Git ラッパーを有効にします。固定するのは RMK の `git log -1 --format=%H -- .` 問い合わせだけです。それ以外の Git 操作は通常の Git に渡します。

### フラッシュの割り当て

範囲の末尾は含みません。

- **右**：アプリは `0x26000..0xB0000`、設定は `0xB0000..0xD0000`。
- **左**：アプリは `0x1000..0xA0000`、CoreXの設定は `0xC0000..0xE0000`。
- 確認した左右のブートローダー開始位置は `0xF4000`。

左のCoreX設定領域は、確認した純正の設定領域 `0xA0000..0xC0000` と重ならない位置に置いています。純正へ戻した後も元の設定を読めるかは未確認です。復元には、別途保存した純正用のVial設定を使ってください。

UF2 はアプリのみで、ブートローダーや設定領域を含みません。右用を左へ、左用を右へ書かないでください。スキーマ互換性の維持は同じ構成を更新するためのもので、純正ファームと CoreX ファームの左右混在を保証するものではありません。

## 公開バイナリの再現性

配布用UF2は、Rustの `--remap-path-prefix` で開発PCのローカルパスを除いてビルドしています。Bluetooth名は `Cornix TB` です。最新の確認範囲は[検証状況](docs/validation.md)を参照してください。配布物の正しいハッシュは [firmware/SHA256SUMS](firmware/SHA256SUMS) を使用してください。

ライセンスと派生元は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) に記載しています。

次に検証する要件と、他のファーム・READMEから採用した考え方は[改善の優先順位](docs/design-review.md)にまとめています。

## Vialの保護と保存形式

右では `host_lock` を有効にし、Y＋Backspaceの物理長押しで重要操作を許可します。通常のキー割り当てと感度変更はロック中も使えます。ロックはRAM上の状態だけで、保存データの列挙型やフィールドを変えません。このため `host_lock` だけを保存スキーマのfeature列から除外し、`RMK_ENABLED_FEATURES` には実際の全featureを残します。保存スキーマは従来の `0xA805CEFB` を維持します。
