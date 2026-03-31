バージョンをパッチアップし、変更をコミットしてpushし、deployし、ローカルでMac(ARM)版をビルドし、GitHub Actionsで全環境（macOS ARM/Intel、Windows、Linux）をビルドしてください。

手順:
1. `git status` で変更ファイルを確認する
2. 変更があればバージョンを patch バンプする（Cargo.toml, src-tauri/Cargo.toml, src-tauri/tauri.conf.json, package.json の4ファイルのバージョン文字列を一括置換）
3. 変更をステージングしてコミットする（コミットメッセージは変更内容を日本語で簡潔に記述）
4. `git push origin main` でpushする
5. `./deploy.sh` でデプロイする
6. Mac ARM版をバックグラウンドでビルドする。必ず以下のコマンドで実行すること（stale dist とキャッシュを確実にクリアするため）:
   ```
   cd /Users/kow/Develop/Web/Leaf && rm -rf dist && rm -rf src-tauri/target/aarch64-apple-darwin/release/build/leaf-app-* src-tauri/target/aarch64-apple-darwin/release/.fingerprint/leaf-app-* && npm run tauri build -- --target aarch64-apple-darwin 2>&1
   ```
7. `gh workflow run "Build Tauri App" --ref main` でGitHub Actionsを起動する
8. GitHub ActionsのURLとMac ARMビルドの完了をユーザーに伝える
