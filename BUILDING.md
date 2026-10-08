# ファームウェアをビルドする

使うだけならビルドは不要です。[firmware/](firmware/) の UF2 と、[README](README.md) の書き込み手順を使ってください。右は coreX A13 用、左は純正 Cornix 左用です。

## 必要なもの

- Rust **1.95.0** と `thumbv7em-none-eabihf` ターゲット
- `llvm-tools-preview`、`flip-link` **0.1.12**
- Python 3、Git、C/C++ ビルド環境と libclang（Nordic のバインディング生成に使用）
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
./build.sh right  # coreX A13 右・PAW3222、v0.9.2
./build.sh left   # 純正 Cornix 左・peripheral、v0.9.0
./build.sh both   # 両方。引数省略時も両方
```

出力先は `build/firmware/`。既存の配布用 `firmware/` は上書きしません。ELF と Cargo キャッシュは `build/target/` に残り、Git 対象外です。キャッシュを別の場所へ置く場合は `CARGO_TARGET_DIR` を指定できます。

```sh
python3 tools/verify_release.py --rebuilt  # 同梱 UF2 と再ビルドした UF2 の検証
./tools/test.sh              # 通信・微小移動・設定のホストテスト 11 件
```

このコマンドは同梱 UF2 の SHA-256 と、再ビルドした UF2 のターゲット・アプリ領域・ベクタテーブル・ローカルパスの混入を検査します。Cargo メタデータがあれば設定スキーマも検査します。キャッシュを外部に置いた場合は `--target-dir <CARGO_TARGET_DIRの場所>` を指定できます。`--rebuilt` を省くと同梱 UF2 だけを検査します。

配布版とのハッシュ一致は参考情報として表示し、不一致だけでは失敗にしません。配布を作成した場所で厳密に照合する場合だけ `--require-identical` を付けてください。

**別のチェックアウト場所からのビルドは確認していますが、バイナリの完全一致は保証しません。** 同一ホスト・同一ソースでもチェックアウト場所が変わると Cargo / コンパイラのメタデータ等が変化し、生成物のハッシュが異なることを確認しています。異なるホスト、C ライブラリ、追加フラグでも変化します。公開用 UF2 の照合には同梱 `SHA256SUMS` を使い、手元でビルドした UF2 のハッシュとは区別してください。

## ソース構成

| 場所 | 内容 |
| --- | --- |
| `source/corex-rmk-pair/right/` | 右の GPIO、配列、Vial 定義、PAW3222、感度・スクロール・AML |
| `source/corex-rmk-pair/left/` | 純正 Cornix 左の peripheral |
| `source/corex-rmk-upstream/` | 固定した RMK 4 クレートと coreX 用差分 |
| `tools/git-metadata/git` | RMK のストレージ互換性を維持するビルドメタデータ固定 |
| `tools/make_uf2.py` | 左右別のアプリ領域を検証して UF2 を生成 |

左右の `Cargo.lock` を同梱し、ビルドは `--locked` で行います。キーボードのマニフェストから RMK への参照はリポジトリ内の相対パスです。公開 RMK の最新版へ自動追従しません。

## 既存設定との互換性

RMK はバージョン・コミット情報・feature・設定容量等からストレージスキーマを計算します。変更するとキーマップやペアリング情報の再初期化が起きます。この版は元の RMK コミット `8a6889854fb996be592c55075b385234133e1772` と既存 feature を保持し、スキーマを **`0xA805CEFB`** に揃えています。

**直接 `cargo build` を使わず、ルートの `build.sh` を使ってください。** リポジトリを新しく commit しただけで RMK のスキーマが変わらないよう、専用の Git ラッパーを有効にします。固定するのは RMK の `git log -1 --format=%H -- .` 問い合わせだけです。それ以外の Git 操作は通常の Git に渡します。

| 対象 | アプリ書き込み開始 | アプリ上限（含まない） | この版の設定領域 |
| --- | --- | --- | --- |
| 右 | `0x26000` | `0xB0000` | `0xB0000–0xD0000` |
| 左 | `0x1000` | `0xA0000` | `0xC0000–0xE0000` |

UF2 はアプリのみで、ブートローダーや設定領域を含みません。右用を左へ、左用を右へ書かないでください。スキーマ互換性の維持は同じ構成を更新するためのもので、純正ファームと coreX ファームの左右混在を保証するものではありません。

## 公開バイナリの再現性

配布用 UF2 は、実機で確認したソースから、Rust の `--remap-path-prefix` でビルドした人のローカルパスを除去して作り直したものです。ソースの処理・キーマップ初期値・ストレージスキーマは変更していませんが、元の手元用ビルドとはパス文字列と配置が変わるため SHA-256 が異なります。配布物の正しいハッシュは [firmware/SHA256SUMS](firmware/SHA256SUMS) を使用してください。

ライセンスと派生元は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) に記載しています。
