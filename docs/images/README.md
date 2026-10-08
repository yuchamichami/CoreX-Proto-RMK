# 説明画像

接続図と表は **[Zen Maru Gothic](https://github.com/googlefonts/zen-marugothic)** を使用し、白背景と細い罫線で統一しています。PNG にしているため、読む人の環境にこのフォントがなくても同じ形で表示されます。Vial の2枚は実機のスクリーンショットです。

- `connection.svg`：編集用の接続図
- `connection.png`：README に表示する接続図
- `tables/`：README・操作ガイド・書き込みガイド・ビルド手順の表
- `vial-overview.png`／`vial-user-settings.png`：実機の配列と User 設定画面

表を直すときは、各 Markdown の「表をテキストで読む」内にある表を編集します。PyQt5 と Zen Maru Gothic の Medium／Bold フォントを用意して、リポジトリのルートから次を実行すると、接続図と全ての表画像を作り直せます。

```sh
python3 tools/render_doc_tables.py --font-dir /path/to/ZenMaruGothic/fonts
```

この描画用依存関係は、ファームウェアのビルドには不要です。表内のリンクは画像の下にも表示し、テキスト版も折りたたみで残しています。
