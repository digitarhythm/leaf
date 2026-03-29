変更をコミットしてpushし、GitHub Actionsで全環境（macOS ARM/Intel、Windows、Linux）をビルドしてください。

手順:
1. `git status` で変更ファイルを確認する
2. 変更があればステージングしてコミットする（コミットメッセージは変更内容を日本語で簡潔に記述）
3. `git push origin main` でpushする
4. `gh workflow run "Build Tauri App" --ref main` でGitHub Actionsを起動する
5. 返ってきたURLをユーザーに伝える
