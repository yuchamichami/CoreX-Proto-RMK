# Cornix準拠の初期キーマップ

[CoreX-Cornix-default.vil](CoreX-Cornix-default.vil)は、CoreX右用の初期配列です。配布ファームと組み合わせて使います。Vialの **File → Load saved layout** で読み込みます。現在の配列は、先に **Save current layout** で保存してください。

このプリセットはキーとエンコーダの割り当てを変更します。感度は2倍、Scrollは標準、AMLはONになります。マクロ・コンボ・タップダンスと、Bluetoothの登録は変更しません。

## 基準にしたファイル

2026-10-09に[メーカーの日本語マニュアル](https://docs.channel.io/jezailfunderjp/ja/articles/Cornix-%E6%97%A5%E6%9C%AC%E8%AA%9E%E3%83%9E%E3%83%8B%E3%83%A5%E3%82%A2%E3%83%AB-c1160246)の「初期キーマップ」から取得した `cornix-default-keymap.vil` を使っています。ファームの版番号は付いていません。

- [メーカー配布の原本](https://cf.channel.io/document/spaces/16010/articles/541498/revisions/942375/usermedia/6958e9940227a7198545)
- [照合用に保存した原本](reference/cornix-default-keymap.vil)
- 原本のSHA-256：`f85dd13d58398ea53e29f3fbab88d07b1f86ba09982e7eaac5d36eb314a327ab`

**原本は純正Cornix用です。CoreXへ読み込むのは、上の `CoreX-Cornix-default.vil` にしてください。** 左右の行列とトラボの設定欄が異なります。

## CoreXで加えた変更

通常キー、数字・記号、Bluetooth切替、エンコーダを、純正の物理位置に合わせています。右の内側親指キーはSpace、隣は予備レイヤーを開くキーです。トラックボールは純正配列のMO(2)の位置にあり、物理キーはありません。

- Iは短押しでI、長押しでボールのスクロールになります。
- ボール操作後のJ・K・LによるクリックとAML設定を残しています。
- Vialの感度・Scroll・AML欄を追加しています。
- 既存設定との互換性のため、レイヤー番号を一部置き換えています。純正0→CoreX 0、1→1、2→5、3→4、4→6です。CoreX 2・3はトラボ用です。
- 純正の予備レイヤー5〜9は、初期配列からの入口がなく、エンコーダ以外は未割り当てのため省略しています。

純正の空欄は、元どおり「何もしない」キーです。下のレイヤーのキーを使う透明キーには置き換えていません。Bluetooth用の純正レイヤー2と3は同じ内容ですが、別々に編集できるよう分けて残しています。

## ファームを更新する場合

初期配列ファイルの読み込みでは、Bluetoothの登録は変わりません。ファームの更新では、更新前の版によってPCの再登録や配列の読み込みが必要になります。[書き込みガイド](../docs/flashing.md#corex-を更新する)で確認してください。
