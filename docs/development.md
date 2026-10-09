# 開発資料

使い始める手順は[README](../README.md)、設定や困ったときの操作は[使い方](usage.md)を参照してください。

## ソースと配布物

利用案内の対象は、左右とも **v0.10.1** です。

- [v0.10.1のソース](https://github.com/yuchamichami/CoreX-Proto-RMK/tree/v0.10.1)
- [v0.10.1の説明書・ソース・ファームを含むZIP](https://github.com/yuchamichami/CoreX-Proto-RMK/releases/download/v0.10.1/CoreX-firmware-hand-off.zip)
- [以前のバージョン](https://github.com/yuchamichami/CoreX-Proto-RMK/releases)

`main`のソースと`firmware/`には、右の読み取り間隔を変更した比較試験版v0.10.2が残っています。v0.10.1をビルドする場合は、そのタグのソースを使ってください。ZIP内の説明書も、その配布時点の内容です。

## ビルド・検証・変更履歴

- [ビルド手順](../BUILDING.md)：開発環境、設定、診断ログの読み方
- [検証記録](validation.md)：版ごとの試験結果、測定値、未確認項目
- [変更履歴](../CHANGELOG.md)：各版の変更点
- [設計・改善の検討](design-review.md)：実装方針と参考資料

8ms読み取りの比較試験と、カクつきの報告を受けて15ms版へ戻した経過は、[v0.10.2の検証記録](validation.md#v0102)にまとめています。

## 待機と復帰の設計

センサーは待機中も電源を保ち、最初の動きで復帰します。通常の復帰では再初期化や移動データの読み捨てをしません。実装は[電源管理の説明](../BUILDING.md#スリープとpaw3222)、動作確認の範囲は[検証記録](validation.md)を参照してください。

Bluetooth接続を維持して復帰する動作は[純正Cornixの説明](https://docs.channel.io/jezailfunderjp/ja/articles/Cornix-日本語マニュアル-c1160246)を参考にしています。待機までの5分はCoreXの設定値で、純正の公開値ではありません。
