// OSC 52（端末からクリップボードへの書き込み要求）の解釈。
//
// tmux や vim はコピーした文字列を `ESC ] 52 ; <対象> ; <Base64> BEL` で端末へ送る。
// xterm.js は本体で OSC 52 を処理しないため、Leaf 側でハンドラを登録して受け取る。
// ここは表示や通信から切り離した純粋関数だけを置き、node:test で単体テストする
// （テストは scripts/osc52.test.mjs）。
// 仕様は docs/TERMINAL_OSC52_SPEC.md。

/** 復元後の本文の上限（暴走した出力でクリップボードを溢れさせない） */
const MAX_TEXT_BYTES = 1024 * 1024;

const IGNORE = Object.freeze({ action: 'ignore' });

/**
 * OSC 52 の本体（`52;` より後ろ）を解釈する。
 *
 * @param {string} data `<対象>;<Base64 または ?>`
 * @returns {{action: 'write', text: string} | {action: 'ignore'}}
 */
export function parseOsc52(data) {
    if (typeof data !== 'string') return IGNORE;
    const sep = data.indexOf(';');
    if (sep < 0) return IGNORE;
    // 対象（c / p / s / 数字など）は区別しない。OS のクリップボードは 1 つだけ
    const payload = data.slice(sep + 1).trim();
    // 読み出し要求には応じない（端末上のプログラムに中身を読ませない）
    if (payload === '' || payload === '?') return IGNORE;
    // Base64 として正しい文字だけで構成されているか（atob は空白などを黙って受け入れるため先に弾く）
    if (!/^[A-Za-z0-9+/]+={0,2}$/.test(payload)) return IGNORE;

    let binary;
    try {
        binary = atob(payload);
    } catch (e) {
        return IGNORE;
    }
    if (binary.length === 0 || binary.length > MAX_TEXT_BYTES) return IGNORE;

    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    let text;
    try {
        text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    } catch (e) {
        return IGNORE;
    }
    return { action: 'write', text };
}
