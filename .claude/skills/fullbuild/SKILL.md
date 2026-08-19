---
name: fullbuild
description: バージョンを上げてpush、本番へデプロイ、Mac ARM版のビルド、GitHub Actionsでの全環境ビルドまでを一括で行う
disable-model-invocation: true
argument-hint: "[patch|minor|major]"
---

# フルビルド

「patchでフルビルドしてください」のように依頼された時の一連の作業。
"$ARGUMENTS" には patch / minor / major のいずれかが入る（省略された場合はどの段階にするかユーザーに尋ねる）。

この依頼は**バージョンアップの許可を含む**（通常はユーザーの指示なしにバージョンを上げない運用のため）。

## 手順

1. **開発サーバーを停止する**
   `trunk serve` と `node server/index.js` が動いていれば終了させる（ビルド成果物の競合を防ぐ）。

2. **バージョンを "$ARGUMENTS" の段階でバンプする**
   対象は次の6ファイル。書き換え後に各1箇所ずつ置換されたことを必ず確認する。
   - `package.json`
   - `Cargo.toml`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
   - `Cargo.lock`（`name = "leaf"` のエントリ）
   - `src-tauri/Cargo.lock`（`name = "leaf-app"` のエントリ）

   ※ ファイルの書き換えは文字コードUTF-8・改行コードLFを保つこと（Pythonなら `open(path, newline="")`）。
   ※ アプリ内のバージョン表示は `env!("CARGO_PKG_VERSION")` を参照しているため個別の更新は不要。

3. **テストとビルドを通す**
   - `cargo test`（全件パスすること）
   - `cargo check --target wasm32-unknown-unknown`（**警告ゼロ**であること）
   - `npm run build`

4. **コミット・タグ・push**
   - 変更をステージしてコミットする（コミットメッセージは日本語。1行目は `v<バージョン>: <要約>`）
   - `src-tauri/.DS_Store` は対象外（コミットしない）
   - 注釈付きタグ `v<バージョン>` を作成し、`git push --follow-tags origin main` でまとめてpushする
   - このタグpushがGitHub Actionsのリリースビルドを起動する

5. **本番へデプロイ**
   `./deploy.sh` を実行する。完了後 `https://leaf.digitarhythm.net` が HTTP 200 を返すことを確認する。
   （デプロイ中に出る setlocale / post-quantum の警告はサーバー側の既存事項で無視してよい）

6. **Mac ARM版をローカルでビルド**（バックグラウンド実行）
   stale な dist とキャッシュを必ず消してから実行する。
   ```
   rm -rf dist && rm -rf src-tauri/target/aarch64-apple-darwin/release/build/leaf-app-* \
     src-tauri/target/aarch64-apple-darwin/release/.fingerprint/leaf-app-* \
     && npm run tauri build -- --target aarch64-apple-darwin
   ```

7. **GitHub Actionsの完了を待つ**（バックグラウンド実行）
   全5環境（macOS aarch64 / macOS x86_64 / Windows / Linux amd64 / Linux arm64）の結果を確認する。
   失敗した場合はログを確認し、外部からのダウンロード失敗など一時的な原因であれば
   `gh run rerun <run-id> --failed` で再実行する。

8. **結果を報告する**
   バージョン、コミットハッシュ、デプロイ結果、Mac ARM版のDMGのパス、
   GitHub Actionsの各環境の結果、リリースページのURLをまとめて伝える。

## 補足

* `gh release view` が稀に 401 を返すことがあるが、再実行すれば取得できる
* 待ち時間の長い処理（Mac ARMビルド、GitHub Actionsの完了待ち）は必ずバックグラウンドで実行する
* 各作業の完了時は `say` コマンドで通知する
