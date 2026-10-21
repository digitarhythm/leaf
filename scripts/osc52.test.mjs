// OSC 52 の解釈のテスト（docs/TERMINAL_OSC52_SPEC.md §3）。
// 実行: node --test scripts/osc52.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseOsc52 } from '../assets/js/osc52.js';

const b64 = (s) => Buffer.from(s, 'utf8').toString('base64');

// 規則 1
test('クリップボード(c)への書き込みは本文を返す', () => {
    assert.deepEqual(parseOsc52(`c;${b64('hello')}`), { action: 'write', text: 'hello' });
});

// 規則 2
test('対象が空・p・s・数字でも書き込みとして扱う', () => {
    for (const target of ['', 'p', 's', '0', 'cp']) {
        assert.deepEqual(parseOsc52(`${target};${b64('x')}`), { action: 'write', text: 'x' });
    }
});

// 規則 3
test('日本語などのマルチバイト文字を UTF-8 として復元する', () => {
    assert.deepEqual(parseOsc52(`c;${b64('日本語のコピー')}`), { action: 'write', text: '日本語のコピー' });
});

// 規則 3（改行・タブ）
test('改行やタブを含む複数行をそのまま復元する', () => {
    const text = 'line1\n\tline2\r\nline3';
    assert.deepEqual(parseOsc52(`c;${b64(text)}`), { action: 'write', text });
});

// 規則 4
test('読み出し要求(?)は無視する', () => {
    assert.deepEqual(parseOsc52('c;?'), { action: 'ignore' });
});

// 規則 5
test('本文が空なら無視する（クリップボードを空にしない）', () => {
    assert.deepEqual(parseOsc52('c;'), { action: 'ignore' });
});

// 規則 6
test('区切りが無い・Base64 として壊れている場合は無視する', () => {
    assert.deepEqual(parseOsc52('garbage'), { action: 'ignore' });
    assert.deepEqual(parseOsc52('c;***not base64***'), { action: 'ignore' });
});

// 規則 7
test('大きすぎる本文は無視する', () => {
    const big = 'a'.repeat(1024 * 1024 + 1);
    assert.deepEqual(parseOsc52(`c;${b64(big)}`), { action: 'ignore' });
});
