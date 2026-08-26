#!/usr/bin/env node
// ghostty-web の配布物を assets/js/ghostty/ へベンダリングする。
//
// Leaf は JS バンドラを使わず assets/ をそのまま配信しているため、
// npm パッケージを実行時に解決できない。ここで必要なファイルだけを取り込む。
// WASM（libghostty-vt）は ghostty-web.js 内に data: URL としても埋め込まれているが、
// 起動時に 550KB の Base64 を復号するのを避けるため .wasm を別ファイルで置き、
// Ghostty.load() に明示的な URL を渡す。
import { readFileSync, writeFileSync, mkdirSync, readdirSync, rmSync, copyFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const pkgDir = join(root, 'node_modules', 'ghostty-web');
const outDir = join(root, 'assets', 'js', 'ghostty');

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

// ESM 本体と、file: プロトコル時のみ動的 import される Node 互換スタブ
for (const name of readdirSync(join(pkgDir, 'dist'))) {
    if (!name.endsWith('.js')) continue;
    const code = readFileSync(join(pkgDir, 'dist', name), 'utf8')
        .replace(/^\/\/# sourceMappingURL=.*$/gm, '')
        .replace(/\n+$/, '\n');
    writeFileSync(join(outDir, name), code, 'utf8');
}

copyFileSync(join(pkgDir, 'ghostty-vt.wasm'), join(outDir, 'ghostty-vt.wasm'));
copyFileSync(join(pkgDir, 'LICENSE'), join(outDir, 'LICENSE'));

const version = JSON.parse(readFileSync(join(pkgDir, 'package.json'), 'utf8')).version;
writeFileSync(join(outDir, 'VERSION'), `ghostty-web@${version}\n`, 'utf8');
console.log(`[vendor-ghostty] ghostty-web@${version} -> assets/js/ghostty/`);
