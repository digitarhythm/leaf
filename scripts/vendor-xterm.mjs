#!/usr/bin/env node
// xterm.js の配布物を assets/js/xterm/ へベンダリングする。
//
// Leaf は JS バンドラを使わず assets/ をそのまま配信しており、以前は
// jsDelivr の CDN から実行時に読み込んでいた。オフラインや CDN 障害で
// ターミナルが使えなくなるため、同梱に切り替えている。
// UMD ビルドなので、従来どおり <script> で読み込めば window.Terminal /
// window.FitAddon が生える。
import { readFileSync, writeFileSync, mkdirSync, rmSync, copyFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const outDir = join(root, 'assets', 'js', 'xterm');

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

const files = [
    ['@xterm/xterm/lib/xterm.js', 'xterm.js'],
    ['@xterm/xterm/css/xterm.css', 'xterm.css'],
    ['@xterm/addon-fit/lib/addon-fit.js', 'addon-fit.js'],
];
for (const [src, dst] of files) {
    copyFileSync(join(root, 'node_modules', src), join(outDir, dst));
}
copyFileSync(join(root, 'node_modules', '@xterm/xterm/LICENSE'), join(outDir, 'LICENSE'));

const version = JSON.parse(
    readFileSync(join(root, 'node_modules', '@xterm', 'xterm', 'package.json'), 'utf8'),
).version;
const fitVersion = JSON.parse(
    readFileSync(join(root, 'node_modules', '@xterm', 'addon-fit', 'package.json'), 'utf8'),
).version;
writeFileSync(join(outDir, 'VERSION'), `@xterm/xterm@${version}\n@xterm/addon-fit@${fitVersion}\n`, 'utf8');
console.log(`[vendor-xterm] @xterm/xterm@${version} + @xterm/addon-fit@${fitVersion} -> assets/js/xterm/`);
