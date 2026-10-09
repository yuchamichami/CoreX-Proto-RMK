# 説明画像

説明図は[Zen Maru Gothic](https://github.com/googlefonts/zen-marugothic)と白背景で作成しています。PNGなので、読む人の環境にフォントがなくても同じ形で表示されます。

- `connection.svg`：編集用の接続図。
- `connection.png`：READMEに表示する接続図。
- `default-keymap.svg`／`default-keymap.png`：初期配列ファイルから描いた配列図。実機の画面ではありません。
- `vial-overview.png`：Vialの描画部品を使ったLayer 0の説明図。配布する初期配列を表示します。
- `vial-user-settings.png`：VialのUserタブを使った設定の説明図。感度1.5倍の選択肢に、説明用の枠を付けています。

Vialの2枚は実機のスクリーンショットではありません。現在の `vial.json` と `CoreX-Cornix-default.vil` からオフラインで生成し、機器名や個別の保存設定を読み取る操作は行いません。ファームの版によって、設定名や見た目が異なる場合があります。

## 接続図を更新する

`connection.svg`を編集します。PyQt5とZen Maru GothicのRegular／Medium／Boldを用意し、リポジトリのルートで実行します。

```sh
python3 tools/render_connection.py --font-dir /path/to/ZenMaruGothic/fonts
```

`connection.png`が更新されます。PyQt5とフォントは図の作成に使います。ファームウェアのビルドには不要です。

## 初期配列図を更新する

初期配列を生成した後、次を実行します。

```sh
python3 tools/render_default_keymap.py --font-dir /path/to/ZenMaruGothic/fonts
```

`keymaps/CoreX-Cornix-default.vil` と右の `vial.json` から、SVGとPNGを作ります。

## Vialの説明図を更新する

PyQt5、Zen Maru GothicのRegular／Medium／Boldと、[Vial GUIのソース](https://github.com/vial-kb/vial-gui)を用意します。Vial GUIはコミット `aef8222a2d0429a183b2ed692d5f9efcfd383f08` で描画を確認しています。

```sh
python3 tools/render_vial_guide.py \
  --vial-source /path/to/vial-gui \
  --font-dir /path/to/ZenMaruGothic/fonts
```

ウィンドウや接続中のキーボードを操作せず、2枚のPNGを更新します。Vialの描画部品を使うため、ソース側の変更で生成手順の調整が必要になる場合があります。生成後は配列とUserタブの文字が読めることを確認してください。
