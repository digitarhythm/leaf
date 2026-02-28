---
name: gitpush
description: git commit & git push origin main
disable-model-invocation: true
argument-hint: "[patch|minor]"
---

* "$ARGUMENTS"が"patch"だった場合は、package.jsonとCargo.tomlのバージョンを「0.0.1」上げる
* "$ARGUMENTS"が"minor"だった場合は、package.jsonとCargo.tomlのバージョンを「0.1」上げ、3つ目の数値を「0」にする（例：元が0.1.2だった場合は、0.2.0）
* "$ARGUMENTS"が空だった場合は、バージョン番号をどうするかユーザーに尋ねる
* バージョン番号が更新されていた場合はアプリケーションのバージョン表示も更新する
* このコマンドが実行された時点までの作業をコミットする（コミットメッセージは日本語で記述）
* バージョン番号が更新されていた場合はバージョン番号のtagを付ける
* 作業ブランチがmainでなかった場合はmainにmergeする
* mainにmergeした後に、GitHubにmainブランチをpushする
* バージョン番号が更新されていた場合は新しいバージョンで付けたtagもpushする
