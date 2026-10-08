# 説明画像

接続図は[Zen Maru Gothic](https://github.com/googlefonts/zen-marugothic)と白背景で作成しています。PNGなので、読む人の環境にフォントがなくても同じ形で表示されます。

- `connection.svg`：編集用の接続図。
- `connection.png`：READMEに表示する接続図。
- `vial-overview.png`：実機のVial画面。
- `vial-user-settings.png`：VialのUserタブを開いた画面。

Vialの画像は実機で撮影したものです。キー配列には個別の変更が含まれます。

## 接続図を更新する

`connection.svg`を編集します。PyQt5とZen Maru GothicのRegular／Medium／Boldを用意し、リポジトリのルートで実行します。

```sh
python3 tools/render_connection.py --font-dir /path/to/ZenMaruGothic/fonts
```

`connection.png`が更新されます。PyQt5とフォントは図の作成に使います。ファームウェアのビルドには不要です。
