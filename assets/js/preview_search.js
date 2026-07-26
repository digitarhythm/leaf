// プレビュー（レンダリング済みHTML）内の文字列検索。
// Ace の検索（ext-searchbox）は EditSession のテキストが対象のため、
// プレビュー表示中は使えない。ここでは DOM のテキストノードを走査して
// ヒット箇所を <mark> でラップし、ハイライト／移動／解除を行う。
//
// ハイライトは CSS Custom Highlight API ではなく DOM ラップ方式を採用している。
// Tauri のシステム WebView（特に Linux の WebKitGTK）でのサポート差を避けるため。

const HIT_CLASS = 'leaf-preview-hit';
const CURRENT_CLASS = 'leaf-preview-hit-current';

// 1ヒット = <mark> 要素の配列（複数のテキストノードに跨るヒットは複数要素になる）
let _matches = [];
let _current = -1;

/** 全画面プレビューの本文要素を取得 */
function getPreviewRoot() {
    const container = document.getElementById('preview-scroll');
    if (!container) return null;
    return container.querySelector('.markdown-body') || container;
}

/** 検索対象のテキストノードを収集（SVG/script/style/既存ハイライトは除外） */
function collectTextNodes(root) {
    const nodes = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
        acceptNode(node) {
            if (!node.nodeValue || node.nodeValue.length === 0) return NodeFilter.FILTER_REJECT;
            let el = node.parentElement;
            while (el && el !== root.parentElement) {
                const tag = el.tagName ? el.tagName.toLowerCase() : '';
                // mermaid の図(svg)・スクリプト・スタイル・ハイライト済み要素は対象外
                if (tag === 'svg' || tag === 'script' || tag === 'style' || tag === 'mark') {
                    return NodeFilter.FILTER_REJECT;
                }
                el = el.parentElement;
            }
            return NodeFilter.FILTER_ACCEPT;
        }
    });
    let n;
    while ((n = walker.nextNode())) nodes.push(n);
    return nodes;
}

/**
 * テキストノードの [start, end) を <mark> でラップして返す。
 * splitText は前半を元ノードに残すため、後方から処理すれば
 * 同一ノード内の前方オフセットは無効化されない。
 */
function wrapSegment(node, start, end) {
    let target = node;
    if (start > 0) target = target.splitText(start);
    if (end - start < target.nodeValue.length) target.splitText(end - start);
    const parent = target.parentNode;
    if (!parent) return null;
    const mark = document.createElement('mark');
    mark.className = HIT_CLASS;
    parent.replaceChild(mark, target);
    mark.appendChild(target);
    return mark;
}

/** ハイライトを全て解除して元のテキストノードに戻す */
export function preview_search_clear() {
    for (const marks of _matches) {
        for (const mark of marks) {
            const parent = mark.parentNode;
            if (!parent) continue;
            parent.replaceChild(document.createTextNode(mark.textContent || ''), mark);
            parent.normalize();
        }
    }
    _matches = [];
    _current = -1;
}

/**
 * 検索を実行してヒット件数を返す。実行後は先頭ヒットが選択状態になる。
 * @param {string} query 検索語
 * @param {boolean} matchCase 大文字小文字を区別するか
 * @returns {number} ヒット件数
 */
export function preview_search(query, matchCase) {
    preview_search_clear();
    const root = getPreviewRoot();
    if (!root || !query) return 0;

    const nodes = collectTextNodes(root);
    if (nodes.length === 0) return 0;

    // 連結テキストとノード境界のマップを作る（要素跨ぎのヒットも拾うため）
    let full = '';
    const map = []; // { node, start, end }
    for (const node of nodes) {
        const start = full.length;
        full += node.nodeValue;
        map.push({ node, start, end: full.length });
    }

    const haystack = matchCase ? full : full.toLowerCase();
    const needle = matchCase ? query : query.toLowerCase();

    // ヒット位置（連結テキスト上の [start, end)）を列挙
    const ranges = [];
    let from = 0;
    while (true) {
        const idx = haystack.indexOf(needle, from);
        if (idx === -1) break;
        ranges.push({ start: idx, end: idx + needle.length });
        from = idx + needle.length;
    }
    if (ranges.length === 0) return 0;

    // ヒットをノード単位のセグメントに分解
    const segmented = ranges.map((r) => {
        const segs = [];
        for (const m of map) {
            if (m.end <= r.start) continue;
            if (m.start >= r.end) break;
            const s = Math.max(r.start, m.start) - m.start;
            const e = Math.min(r.end, m.end) - m.start;
            if (e > s) segs.push({ node: m.node, start: s, end: e });
        }
        return segs;
    });

    // 後方から処理（splitText によるオフセットずれを避ける）
    const built = [];
    for (let i = segmented.length - 1; i >= 0; i--) {
        const marks = [];
        const segs = segmented[i];
        for (let j = segs.length - 1; j >= 0; j--) {
            const seg = segs[j];
            const mark = wrapSegment(seg.node, seg.start, seg.end);
            if (mark) marks.unshift(mark);
        }
        if (marks.length > 0) built.unshift(marks);
    }

    _matches = built;
    _current = -1;
    if (_matches.length > 0) preview_search_goto(0);
    return _matches.length;
}

/**
 * 指定インデックスのヒットへ移動（範囲外は循環）。
 * @param {number} index
 * @returns {number} 実際に選択されたインデックス（ヒット無しは -1）
 */
export function preview_search_goto(index) {
    const n = _matches.length;
    if (n === 0) return -1;
    if (_current >= 0 && _current < n) {
        for (const mark of _matches[_current]) mark.classList.remove(CURRENT_CLASS);
    }
    const i = ((index % n) + n) % n;
    _current = i;
    for (const mark of _matches[i]) mark.classList.add(CURRENT_CLASS);
    const first = _matches[i][0];
    if (first && typeof first.scrollIntoView === 'function') {
        first.scrollIntoView({ block: 'center', inline: 'nearest' });
    }
    return i;
}

/** 現在のヒット件数 */
export function preview_search_count() {
    return _matches.length;
}
