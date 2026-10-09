# CoreX Proto RMK

CoreX用のRMKファームウェアです。PAW3222トラックボールを接続した右基板と、純正Cornixの左キーボードを組み合わせて使えます。キー配列とトラックボールの設定はVialで変更できます。

[使い方](docs/usage.md) · [書き込み・更新](docs/flashing.md) · [ダウンロード](https://github.com/yuchamichami/CoreX-Proto-RMK/releases/latest)

## 接続

左右にCoreX用ファームウェアを書き込んだ状態で使います。純正Cornixの左を初めて使う場合は、先に[左の書き込み](docs/flashing.md)を済ませてください。

1. 左右の電源をONにします。
2. **右側をPCにUSBでつなぎます。** 左側は右へ自動で無線接続します。
3. 左右のキーを押し、ボールを回して動作を確認します。

![Cornix左からCoreX右へ無線接続し、右からPCへUSBまたはBluetoothで接続](docs/images/connection.png)

Bluetoothで使うときは、PCに `Cornix TB` を登録します。v0.9.5から右側の電池残量を表示でき、macOS 15.2で確認しています。旧版から更新した場合は、一度だけBluetoothの登録をやり直してください。[接続と残量表示](docs/usage.md#pc-と-bluetooth-接続する)

## トラックボールの操作

初期設定では、次の操作が使えます。

- **移動**：ボールを回します。
- **クリック**：ボールを動かした直後に、Jで左クリック、Kで中クリック、Lで右クリック。
- **スクロール**：Iを長押ししながらボールを回します。Iを短く押すと、文字のIを入力します。

ボールを動かすと、J・K・Lが一時的にクリックへ切り替わります。ボールを止めてから約0.7秒で文字入力に戻ります。この機能をAMLと呼び、VialでON／OFFを選べます。

右エンコーダは反時計回りで上スクロール、時計回りで下スクロール、押し込みで中クリック。左エンコーダは回転で音量調整、押し込みでミュートです。

## 初期配列

v0.9.3から、通常のキー、数字・記号のFn配列、エンコーダを純正Cornixの位置に揃えました。Iの長押し、J・K・Lのクリック、AMLはCoreX用の操作として残しています。

左の親指キーは左からFn、Bluetooth、Spaceです。右はSpaceと予備レイヤーのキーです。純正の右MO(2)の位置にはトラックボールがあります。[初期配列の図](docs/usage.md#初期配列)

v0.9.2以前からの更新では、保存した変更と新しい初期値が混在する場合があります。配列全体を揃えるには、[初期配列ファイル](keymaps/CoreX-Cornix-default.vil)をVialで読み込みます。[バックアップと適用手順](docs/flashing.md#corex-を-v095-に更新する)

## Vialで設定する

右側をUSB接続し、[Vial](https://get.vial.today/)を開きます。キーの割り当ては、配列の中でキーを選び、下の一覧から変更します。

トラックボールの設定は、Layer 0にある「感度」「Scroll」「AML」の欄を選び、**User** タブで値を指定します。初期値は感度2倍、Scroll標準、AML ONです。設定は本体に保存されます。

画面上の位置と各設定の説明は、[画像付きの操作ガイド](docs/usage.md#vial-で変更する)を参照してください。

## 純正Cornixとの違い

- PCにつなぐのは、純正Cornixでは左側、CoreXでは右側です。
- 左側にも、このリポジトリの左用ファームウェアが必要です。
- 右側のPAW3222トラックボールと、感度・スクロール・AMLの設定が加わります。
- 通常キーは純正配列を基にし、トラックボール用の操作とレイヤーを加えています。Vial保存ファイルは純正Cornix用と分けて管理してください。
- **接続状態や電池残量を示す純正のLED表示には対応していません。** 接続できたかどうかは、キー入力で確認します。

この版の対象は、右のJ4に接続したPAW3222です。トラックポイントとタッチパッドは使えません。純正の無線ドングルとの接続や、電池持ちは未確認です。

純正の操作は[メーカーの日本語マニュアル](https://docs.channel.io/jezailfunderjp/ja/articles/Cornix-%E6%97%A5%E6%9C%AC%E8%AA%9E%E3%83%9E%E3%83%8B%E3%83%A5%E3%82%A2%E3%83%AB-c1160246)を参照してください。

## ファームウェア

- [右用 v0.9.5 — CoreX・PAW3222](firmware/CoreX-Right-Central-PAW3222-RMK-v0.9.5.uf2)
- [左用 v0.9.0 — 純正Cornix左](firmware/coreX-Cornix-StockLeft-Peripheral-RMK-v0.9.0.uf2)
- [説明書・ソースを含むZIP](https://github.com/yuchamichami/CoreX-Proto-RMK/releases/latest/download/CoreX-firmware-hand-off.zip)

右v0.9.5と左v0.9.0を組み合わせて使います。左v0.9.0を導入済みなら、更新は右だけです。**左右のUF2を取り違えないでください。**

[書き込み・更新・純正左への復元](docs/flashing.md)

v0.9.5は実機への書き込み、設定保持、macOS 15.2のBluetooth設定での残量表示を確認しました。[検証状況](docs/validation.md)

## 開発

[ビルド手順](BUILDING.md) · [変更履歴](CHANGELOG.md) · [ライセンスと使用ライブラリ](THIRD_PARTY_NOTICES.md)

[RMK](https://github.com/rmk-rs/rmk)をベースにしています。Cornixメーカーの公式ファームウェアではありません。
