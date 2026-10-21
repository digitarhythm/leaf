# ターミナルの OSC 52（クリップボード連携）仕様

- 対象: デスクトップ版（Tauri）のターミナル。Web 版はターミナルを持たないため対象外
- 状態: **実装済み（2026-10-21）**。テスト 8 件は緑。実機での動作確認待ち（§6）

## 1. 背景

tmux のコピーモードでコピーした文字列が、OS のクリップボードに入らない。

tmux はコピーした内容を **OSC 52** という制御シーケンスで端末へ送り、
端末がそれを受けて OS のクリップボードに書き込む仕組みになっている。
Leaf の端末（xterm.js 5.5）は**本体で OSC 52 を処理しない**ため、送られてきた内容が捨てられている。

```
ESC ] 52 ; <対象> ; <Base64 で符号化した本文> BEL
```

## 2. 動作

```mermaid
sequenceDiagram
  participant T as tmux / vim
  participant X as xterm.js
  participant J as editor_interop.js
  participant R as Tauri (Rust)
  participant OS as OS のクリップボード

  T->>X: ESC ] 52 ; c ; <Base64> BEL
  X->>J: registerOscHandler(52) が受け取る
  J->>J: parseOsc52（Base64 → UTF-8）
  J->>R: invoke('write_clipboard_native', { text })
  R->>OS: 書き込み
```

- 書き込み要求だけに対応する
- **読み出し要求（`?`）には応じない**。端末上のプログラムがクリップボードの中身を
  勝手に読めてしまうため（xterm 系の端末でも既定で無効にしているものが多い）
- 貼り付けは従来どおり Cmd+V（xterm.js 標準）。tmux 自身のバッファからの貼り付け（prefix + `]`）は tmux 側の機能

### クリップボードへの書き込みを Rust 側で行う理由

WebView の `navigator.clipboard.writeText` は、WebKit（macOS）ではクリック等の**ユーザー操作の直後でないと拒否**される。
OSC 52 は端末の出力として届くのでユーザー操作に紐付かず、WebView からは書き込めない。
そのため Tauri のコマンド `write_clipboard_native` で OS のクリップボードへ直接書き込む。

## 3. 解釈の純粋関数（`assets/js/osc52.js`）

```js
parseOsc52(data) // data は `52;` より後ろ（`<対象>;<Base64 または ?>`）
  → { action: 'write', text } | { action: 'ignore' }
```

| # | 規則 |
|---|---|
| 1 | `c;<Base64>` は本文を復元して `write` |
| 2 | 対象（`;` より前）は空・`p`・`s`・数字・複数文字でも `write`（OS のクリップボードは 1 つなので区別しない） |
| 3 | Base64 を UTF-8 として復元する（日本語・改行・タブを含めて） |
| 4 | 本文が `?`（読み出し要求）なら `ignore` |
| 5 | 本文が空なら `ignore`（クリップボードを空にしない） |
| 6 | `;` が無い、Base64 として壊れている場合は `ignore` |
| 7 | 復元後が 1MiB を超える場合は `ignore`（暴走した出力でクリップボードを溢れさせない） |

テスト: `scripts/osc52.test.mjs`（`node --test scripts/osc52.test.mjs`、Node 標準のテスト機能。追加の依存は無い）

## 4. 影響範囲

| ファイル | 変更 |
|---|---|
| `assets/js/osc52.js`（新規） | 解釈の純粋関数 |
| `scripts/osc52.test.mjs`（新規） | テスト 8 件 |
| `assets/js/editor_interop.js` | 端末を作る時に `terminal.parser.registerOscHandler(52, ...)` を登録し、`write_clipboard_native` を呼ぶ |
| `src-tauri/src/lib.rs` | `write_clipboard_native(text)` コマンドを追加 |
| `src-tauri/Cargo.toml` | クリップボード書き込み用のクレート **`arboard`** を追加（Tauri 公式のクリップボードプラグインも内部でこれを使っている） |

## 5. tmux 側の設定

tmux 3.2 以降は既定で OSC 52 を送る（`set-clipboard` の既定値 `external`、
`terminal-features` の既定に `xterm*:clipboard` が含まれ、Leaf の `TERM` は `xterm-256color`）。
`~/.tmux.conf` で `set -g set-clipboard off` にしている場合は送られないので、`on` か `external` にする。

## 6. 動作確認項目（実装後）

- [ ] tmux のコピーモードでコピー → 他のアプリで貼り付けできる
- [ ] 日本語・複数行のコピー
- [ ] tmux の外（素のシェル）で `printf '\e]52;c;%s\a' "$(printf hello | base64)"` → `hello` がクリップボードに入る
- [ ] `printf '\e]52;c;?\a'` で何も起きない（読み出しに応じない）
- [ ] macOS / Windows / Linux
