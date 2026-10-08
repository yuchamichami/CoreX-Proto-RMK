# 説明画像

接続図は[Zen Maru Gothic](https://github.com/googlefonts/zen-marugothic)と白背景で作成しています。PNGなので、読む人の環境にフォントがなくても同じ形で表示されます。

- `connection.svg`：編集用の接続図。
- `connection.png`：READMEに表示する接続図。
- `default-keymap.svg`／`default-keymap.png`：初期配列ファイルから描いた配列図。実機の画面ではありません。
- `vial-overview.png`：実機のVial画面。
- `vial-user-settings.png`：VialのUserタブを開いた画面。

Vialの画像は実機で撮影したものです。キー配列には個別の変更が含まれます。

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

`keymaps/coreX-Cornix-default.vil` と右の `vial.json` から、SVGとPNGを作ります。
