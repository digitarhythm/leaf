let editor;
let commandCallback;
let pendingContent = null;
let pendingGutterUnsaved = null;
let pendingMode = null; // 追加: モード設定の待機用
let previewActive = false;
let localFileHandle = null;
let localFilePath = null; // Tauri用: ネイティブファイルパス保持
let internalChange = false;
const _undoStates = new Map(); // シートIDごとのUndo/Redo履歴
const FONT_SIZE_KEY_BASE = 'leaf_font_size';
function getFontSizeKey() {
    const email = localStorage.getItem('leaf_google_email');
    return email ? `${FONT_SIZE_KEY_BASE}_${email}` : FONT_SIZE_KEY_BASE;
}

export function can_install_pwa() {
    const prompt = window.leafDeferredPrompt;
    console.log("[Leaf-PWA] can_install_pwa checked. Status:", !!prompt);
    return !!prompt;
}

export function is_tauri() {
    return !!window.__TAURI__;
}

export async function open_url_in_browser(url) {
    if (is_tauri()) {
        const isAbsolute = url.startsWith('http://') || url.startsWith('https://');
        if (isAbsolute) {
            await window.__TAURI__.core.invoke('open_url_in_browser', { url });
        } else {
            // ローカルHTMLはTauri WebviewWindowで開く
            await window.__TAURI__.core.invoke('open_local_page', { path: url });
        }
    } else {
        window.open(url, '_blank', 'noopener,noreferrer');
    }
}

export async function trigger_pwa_install() {
    const prompt = window.leafDeferredPrompt;
    if (!prompt) {
        console.warn("[Leaf-PWA] trigger_pwa_install called but prompt is null.");
        return false;
    }
    prompt.prompt();
    const { outcome } = await prompt.userChoice;
    console.log(`[Leaf-PWA] User response to the install prompt: ${outcome}`);
    window.leafDeferredPrompt = null;
    return outcome === 'accepted';
}

export function is_webkit_or_safari() {
    const ua = navigator.userAgent.toLowerCase();
    return (ua.indexOf('webkit') !== -1 && ua.indexOf('chrome') === -1 && ua.indexOf('safari') !== -1);
}

export async function open_local_file() {
    if (is_tauri()) {
        try {
            const result = await window.__TAURI__.core.invoke('open_local_file_native');
            localFileHandle = null; // Tauri ではハンドルは使わない
            localFilePath = result.path; // パスを保持
            return { name: result.name, content: result.content, bytes: new Uint8Array(result.bytes), path: result.path };
        } catch (e) {
            if (e === 'cancelled') return null;
            console.error("Tauri file open failed:", e);
            return null;
        }
    }
    try {
        const [handle] = await window.showOpenFilePicker({
            types: [{
                description: 'Text Files',
                accept: { 'text/plain': ['.txt', '.md', '.js', '.ts', '.rs', '.toml', '.json', '.yaml', '.yml', '.sql', '.html', '.css', '.py', '.c', '.cpp', '.h', '.m', '.cs', '.php', '.coffee', '.pl', '.rb', '.java', '.sh', '.xml'] }
            }]
        });
        localFileHandle = handle;
        localFilePath = null;
        const file = await handle.getFile();
        const buffer = await file.arrayBuffer();
        const bytes = new Uint8Array(buffer);

        // 1. UTF-8 でデコードを試みる (不正なバイトがあれば例外を投げる設定)
        const utf8Decoder = new TextDecoder('utf-8', { fatal: true });
        let text;
        try {
            text = utf8Decoder.decode(bytes);
        } catch (e) {
            // 2. UTF-8 が失敗した場合は Shift_JIS を試す
            console.log("[Leaf-SYSTEM] UTF-8 decoding failed. Trying Shift_JIS for legacy support...");
            const sjisDecoder = new TextDecoder('shift-jis');
            text = sjisDecoder.decode(bytes);
        }

        return { name: file.name, content: text, bytes: bytes };
    } catch (e) {
        if (e.name === 'AbortError') return null;
        console.error("Local open failed:", e);
        return null;
    }
}

// シート切り替え時などに、アクティブなローカルシートのパスをセットする
export function set_local_file_path(path) {
    localFilePath = path || null;
}

export async function save_local_file(content, needs_bom) {
    if (is_tauri()) {
        try {
            const result = await window.__TAURI__.core.invoke('save_local_file_native', {
                content: content,
                needsBom: needs_bom,
                currentPath: localFilePath || null
            });
            localFilePath = result.path;
            // nameとpath両方を含むオブジェクトを返す（Rust側でlocal_pathを更新するため）
            return { name: result.name, path: result.path };
        } catch (e) {
            if (e === 'cancelled') return null;
            console.error("Tauri file save failed:", e);
            return null;
        }
    }
    try {
        // ハンドルがない場合は新規作成ダイアログを表示
        if (!localFileHandle) {
            localFileHandle = await window.showSaveFilePicker({
                suggestedName: 'Untitled.txt',
                types: [{
                    description: 'Text Files',
                    accept: { 'text/plain': ['.txt', '.md', '.js', '.ts', '.rs', '.toml', '.json', '.yaml', '.yml', '.sql', '.html', '.css', '.py', '.c', '.cpp', '.h', '.m', '.cs', '.php', '.coffee', '.pl', '.rb', '.java', '.sh', '.xml'] }
                }]
            });
        }

        // パーミッションの確認
        const options = { mode: 'readwrite' };
        if (await localFileHandle.queryPermission(options) !== 'granted') {
            if (await localFileHandle.requestPermission(options) !== 'granted') {
                return null;
            }
        }
        const writable = await localFileHandle.createWritable();
        if (needs_bom) {
            const bom = new Uint8Array([0xEF, 0xBB, 0xBF]);
            await writable.write(bom);
        }
        await writable.write(content);
        await writable.close();
        return localFileHandle.name;
    } catch (e) {
        if (e.name === 'AbortError') return null;
        console.error("Local save failed:", e);
        return null;
    }
}

export function clear_local_handle() {
    localFileHandle = null;
    localFilePath = null;
}

export function get_safe_chunk(uint8array) {
    if (!uint8array || uint8array.length === 0) return { text: "", bytes_consumed: 0 };

    let len = uint8array.length;
    let end = len;

    // UTF-8 のマルチバイト文字が途切れていないかチェック (末尾3バイトを確認)
    for (let i = 1; i <= 3 && i <= len; i++) {
        let byte = uint8array[len - i];
        if ((byte & 0xC0) === 0xC0) { // リーディングバイト (11xxxxxx)
            let expected = 0;
            if ((byte & 0xE0) === 0xC0) expected = 2;      // 2バイト文字
            else if ((byte & 0xF0) === 0xE0) expected = 3; // 3バイト文字
            else if ((byte & 0xF8) === 0xF0) expected = 4; // 4バイト文字

            if (i < expected) {
                // 文字が途切れているので、この文字の直前までで切る
                end = len - i;
            }
            break;
        }
        if ((byte & 0x80) === 0x00) break; // ASCII (0xxxxxxx) なので安全
    }

    // \r\n の途切断チェック (\r で終わっている場合は \n と泣き別れないように1バイト戻す)
    if (end > 0 && uint8array[end - 1] === 0x0D) {
        end--;
    }

    const consumed = uint8array.slice(0, end);
    const decoder = new TextDecoder('utf-8');
    let text = decoder.decode(consumed);

    return { text, bytes_consumed: end };
}

export function set_window_title(title) {
    document.title = title ? `${title} - Leaf` : "Leaf";
}

export function init_editor(element_id, callback) {
    if (typeof ace === 'undefined') {
        console.error("[Leaf-SYSTEM] Ace Editor is not defined. Offline startup might have failed to load CDN assets.");
        return;
    }
    commandCallback = callback;
    editor = ace.edit(element_id);
    const savedTheme = localStorage.getItem("leaf_editor_theme") || "gruvbox";
    editor.setTheme("ace/theme/" + savedTheme);
    editor.session.setMode("ace/mode/javascript");

    // 基本設定
    editor.setOptions({
        fontSize: localStorage.getItem(getFontSizeKey()) || "14pt",
        fontFamily: "'JetBrains Mono', 'Fira Code', 'Courier New', monospace",
        enableBasicAutocompletion: true,
        enableLiveAutocompletion: true,
        showPrintMargin: false,
        useSoftTabs: true,
        tabSize: 4,
        wrap: true,
        indentedSoftWrap: true
    });

    // キーバインド (Vim)
    editor.setKeyboardHandler("ace/keyboard/vim");

    // 変更イベント
    editor.on("change", () => {
        if (internalChange) return;
        if (commandCallback) commandCallback("change");
    });

    // Vim モードの状態監視
    editor.on("vimModeChange", (e) => {
        const container = editor.container;
        console.log("[Leaf-VIM] Mode changed to:", e.mode);
        if (e.mode === "insert") {
            container.classList.add("leaf-insert-mode");
            container.classList.remove("leaf-normal-mode");
        } else {
            container.classList.add("leaf-normal-mode");
            container.classList.remove("leaf-insert-mode");
        }
    });

    // カスタムコマンド (検索)
    editor.commands.addCommand({
        name: "findInLeaf",
        bindKey: { win: "Alt-F", mac: "Option-F" },
        exec: (editor) => { editor.execCommand("find"); }
    });

    // カスタムコマンド (保存)
    editor.commands.addCommand({
        name: "saveSheet",
        bindKey: { win: "Alt-S", mac: "Option-S" },
        exec: () => { if (commandCallback) commandCallback("save"); }
    });

    // カスタムコマンド (新規)
    editor.commands.addCommand({
        name: "newSheet",
        bindKey: { win: "Alt-N", mac: "Option-N" },
        exec: () => { if (commandCallback) commandCallback("new_sheet"); }
    });

    // カスタムコマンド (新規ローカル)
    editor.commands.addCommand({
        name: "newLocalSheet",
        bindKey: { win: "Alt-Shift-N", mac: "Option-Shift-N" },
        exec: () => { if (commandCallback) commandCallback("new_local_sheet"); }
    });

    // カスタムコマンド (インポート - ローカルファイルを開く)
    editor.commands.addCommand({
        name: "openLocalFile",
        bindKey: { win: "Alt-O", mac: "Alt-O" },
        exec: () => { if (commandCallback) commandCallback("import"); }
    });

    // カスタムコマンド (ヘルプ)
    editor.commands.addCommand({
        name: "showHelp",
        bindKey: { win: "Alt-H", mac: "Alt-H" },
        exec: () => {
            console.log("[Leaf-VIM] Triggering help shortcut (Alt+H)");
            if (commandCallback) commandCallback("help");
        }
    });

    // カスタムコマンド (フォントサイズ+)
    editor.commands.addCommand({
        name: "increaseFontSize",
        bindKey: { win: "Alt-=", mac: "Option-=" },
        exec: () => { change_font_size(1); }
    });

    // カスタムコマンド (フォントサイズ-)
    editor.commands.addCommand({
        name: "decreaseFontSize",
        bindKey: { win: "Alt--", mac: "Option--" },
        exec: () => { change_font_size(-1); }
    });

    // 待機中の内容があれば反映（初期ロード時はUNDO起点をリセット）
    if (pendingContent !== null) {
        set_editor_content(pendingContent);
        editor.session.getUndoManager().reset();
    }

    // 待機中のガーターステータスがあれば反映
    if (pendingGutterUnsaved !== null) {
        set_gutter_status(pendingGutterUnsaved);
    }

    // 待機中のモードがあれば反映
    if (pendingMode !== null) {
        console.log("[Leaf-SYSTEM] Applying pending mode after init:", pendingMode);
        set_editor_mode(pendingMode);
    }

    // 初期化直後のリサイズ
    setTimeout(() => editor.resize(), 100);
    setTimeout(() => editor.resize(), 500);
}

export function set_vim_mode(enabled) {
    if (!editor) return;
    const container = editor.container;
    const currentHandler = editor.getKeyboardHandler();
    const isVim = currentHandler && currentHandler.$id === "ace/keyboard/vim";

    if (enabled && !isVim) {
        editor.setKeyboardHandler("ace/keyboard/vim");
        container.classList.add("leaf-vim-enabled");
    } else if (!enabled && isVim) {
        editor.setKeyboardHandler(null);
        container.classList.remove("leaf-vim-enabled", "leaf-normal-mode", "leaf-insert-mode");
        editor.focus();
    }
}

export function set_editor_content(content) {
    pendingContent = content;
    if (!editor) return;

    const currentVal = editor.getValue();
    // 内容が完全に一致するか、改行コードの違いを除いて一致する場合は何もしない
    if (currentVal === content || currentVal.replace(/\r\n/g, "\n") === (content || "").replace(/\r\n/g, "\n")) {
        pendingContent = null;
        return;
    }

    // カーソル位置とスクロール位置を保存
    const cursorPos = editor.getCursorPosition();
    const scrollTop = editor.session.getScrollTop();
    const scrollLeft = editor.session.getScrollLeft();

    internalChange = true;
    try {
        editor.setValue(content || "", -1);
        editor.clearSelection();
        // カーソル位置を復元（行数を超えている場合は末尾に移動）
        const maxRow = editor.session.getLength() - 1;
        const restoreRow = Math.min(cursorPos.row, maxRow);
        const maxCol = editor.session.getLine(restoreRow).length;
        const restoreCol = Math.min(cursorPos.column, maxCol);
        editor.moveCursorToPosition({ row: restoreRow, column: restoreCol });
        editor.clearSelection();
        // スクロール位置を復元
        editor.session.setScrollTop(scrollTop);
        editor.session.setScrollLeft(scrollLeft);
        // UNDO履歴は保持する（自動保存後もUNDOを継続可能にする）
        pendingContent = null;
    } finally {
        internalChange = false;
    }
}

// 新規シートロード・シート切替用（カーソルリセット＋UNDO履歴クリア）
export function load_editor_content(content) {
    pendingContent = content;
    if (!editor) return;

    const currentVal = editor.getValue();
    if (currentVal === content || currentVal.replace(/\r\n/g, "\n") === (content || "").replace(/\r\n/g, "\n")) {
        pendingContent = null;
        return;
    }

    internalChange = true;
    try {
        editor.setValue(content || "", -1);
        editor.clearSelection();
        editor.session.getUndoManager().reset();
        pendingContent = null;
    } finally {
        internalChange = false;
    }
}

export function append_editor_content(content) {
    if (!editor) return;
    const session = editor.session;
    const row = session.getLength();
    console.log("[Leaf-SYSTEM] Appending chunk (" + content.length + " bytes) at row " + row);

    internalChange = true;
    try {
        // 末尾に挿入
        session.insert({
            row: row,
            column: 0
        }, content);
    } finally {
        internalChange = false;
    }
}

export function get_editor_content() {
    if (!editor) return pendingContent;
    return editor.getValue();
}

export function get_char_at_cursor() {
    if (!editor) return '';
    const pos = editor.getCursorPosition();
    const line = editor.session.getLine(pos.row);
    if (!line || pos.column >= line.length) return '';
    // サロゲートペア（絵文字など）対応
    const code = line.charCodeAt(pos.column);
    if (code >= 0xD800 && code <= 0xDBFF && pos.column + 1 < line.length) {
        return line.slice(pos.column, pos.column + 2);
    }
    return line[pos.column] || '';
}

export function resize_editor() {
    if (!editor) return;
    editor.resize(true);
    editor.renderer.updateFull(true);
}

let cursorSyncHandler = null;
let cursorSyncTimer = null;

function syncPreviewToLine() {
    const previewEl = document.getElementById('split-preview-scroll');
    if (!previewEl) return;

    const row = editor.getCursorPosition().row;
    const lines = editor.getValue().split('\n');
    const totalRows = lines.length;
    if (totalRows <= 1) return;

    // .markdown-body の直下ブロック要素を取得
    const contentDiv = previewEl.querySelector('.markdown-body');

    // 末尾3行分の余白をpadding-bottomで確保（常時）
    const TAIL_PADDING = '4.5em';
    if (contentDiv && contentDiv.style.paddingBottom !== TAIL_PADDING) {
        contentDiv.style.paddingBottom = TAIL_PADDING;
    }

    if (!contentDiv || contentDiv.children.length === 0) {
        // コードファイルなど非Markdown: 比率ベースにフォールバック
        const ratio = row / (totalRows - 1);
        const maxScroll = previewEl.scrollHeight - previewEl.clientHeight;
        if (maxScroll > 0) {
            const isNearEnd = row >= totalRows - 4;
            let target = ratio * maxScroll - previewEl.clientHeight * 0.25;
            if (isNearEnd) target = maxScroll;
            previewEl.scrollTop = Math.max(0, Math.min(target, maxScroll));
        }
        return;
    }

    // ソース行からブロック開始行のリストを構築
    // 開きフェンス行をブロック開始として扱い、コードブロック内部と閉じフェンスはスキップ
    const blockStarts = [];
    let inCodeBlock = false;
    let prevWasEmpty = true;

    for (let i = 0; i < lines.length; i++) {
        const trimmed = lines[i].trim();
        if (!inCodeBlock && (trimmed.startsWith('```') || trimmed.startsWith('~~~'))) {
            // 開きフェンス → ブロック開始として登録してコードブロックへ
            blockStarts.push(i);
            prevWasEmpty = false;
            inCodeBlock = true;
            continue;
        }
        if (inCodeBlock) {
            // 閉じフェンスでコードブロック終了、それ以外はスキップ
            if (trimmed.startsWith('```') || trimmed.startsWith('~~~')) {
                inCodeBlock = false;
                prevWasEmpty = true; // 閉じフェンス後は次の行を新ブロックとして認識させる
            }
            continue;
        }
        if (trimmed === '') {
            prevWasEmpty = true;
        } else {
            if (i === 0 || prevWasEmpty || /^#{1,6}\s/.test(trimmed)) {
                blockStarts.push(i);
            }
            prevWasEmpty = false;
        }
    }

    if (blockStarts.length === 0) return;

    // カーソル行が属するブロックインデックスを特定
    let blockIdx = 0;
    for (let i = 0; i < blockStarts.length; i++) {
        if (blockStarts[i] <= row) blockIdx = i;
        else break;
    }

    // 対応する DOM 要素を取得（ブロック数とDOM要素数が異なる場合は比例マッピング）
    const blockEls = contentDiv.children;
    let domIdx;
    if (blockStarts.length <= 1 || blockEls.length <= 1) {
        domIdx = 0;
    } else if (blockStarts.length === blockEls.length) {
        domIdx = Math.min(blockIdx, blockEls.length - 1);
    } else {
        // Mermaid等の非同期描画でDOM要素数がズレた場合はハイブリッドアプローチ:
        // ブロック比率とソース行比率を組み合わせてマッピング
        const blockRatio = blockIdx / (blockStarts.length - 1);
        const lineRatio = row / Math.max(1, totalRows - 1);
        // ブロック比率を70%、行比率を30%で混合
        const blendedRatio = blockRatio * 0.7 + lineRatio * 0.3;
        domIdx = Math.round(blendedRatio * (blockEls.length - 1));
        domIdx = Math.min(Math.max(0, domIdx), blockEls.length - 1);
    }
    const targetEl = blockEls[domIdx];
    if (!targetEl) return;

    // 対象ブロックのコンテナ内上端オフセットを計算
    const elTop = targetEl.getBoundingClientRect().top
                  - previewEl.getBoundingClientRect().top
                  + previewEl.scrollTop;

    // 対象ブロックがプレビューの縦中央付近に来るようスクロール
    // 末尾付近は maxScroll まで完全スクロール（padding-bottom で3行分の余白確保済み）
    const maxScroll = previewEl.scrollHeight - previewEl.clientHeight;
    const isNearEnd = row >= totalRows - 4;
    let targetScroll = elTop - previewEl.clientHeight * 0.5;
    if (isNearEnd) {
        targetScroll = Math.max(targetScroll, maxScroll);
    }
    previewEl.scrollTop = Math.max(0, Math.min(targetScroll, maxScroll));
}

export function setup_cursor_sync() {
    if (!editor) return;
    teardown_cursor_sync();
    cursorSyncHandler = function() {
        if (cursorSyncTimer) clearTimeout(cursorSyncTimer);
        cursorSyncTimer = setTimeout(syncPreviewToLine, 50);
    };
    editor.session.selection.on('changeCursor', cursorSyncHandler);
}

export function teardown_cursor_sync() {
    if (cursorSyncTimer) { clearTimeout(cursorSyncTimer); cursorSyncTimer = null; }
    if (!editor || !cursorSyncHandler) return;
    editor.session.selection.off('changeCursor', cursorSyncHandler);
    cursorSyncHandler = null;
}
export function focus_editor() { if (editor) editor.focus(); }

export function get_editor_state() {
    if (!editor) return JSON.stringify({ row: 0, col: 0, scrollTop: 0, scrollLeft: 0 });
    const pos = editor.getCursorPosition();
    return JSON.stringify({
        row: pos.row,
        col: pos.column,
        scrollTop: editor.session.getScrollTop(),
        scrollLeft: editor.session.getScrollLeft()
    });
}

export function set_editor_state(state_json) {
    if (!editor) return;
    try {
        const s = JSON.parse(state_json);
        const maxRow = editor.session.getLength() - 1;
        const row = Math.min(s.row || 0, maxRow);
        const maxCol = editor.session.getLine(row).length;
        const col = Math.min(s.col || 0, maxCol);
        editor.moveCursorToPosition({ row, column: col });
        editor.clearSelection();
        editor.session.setScrollTop(s.scrollTop || 0);
        editor.session.setScrollLeft(s.scrollLeft || 0);
    } catch (_) {}
}

// シートのUndo/Redo履歴を保存
export function save_undo_state(sheetId) {
    if (!editor || !sheetId) return;
    const um = editor.session.getUndoManager();
    _undoStates.set(sheetId, {
        undoStack: JSON.parse(JSON.stringify(um.$undoStack || [])),
        redoStack: JSON.parse(JSON.stringify(um.$redoStack || []))
    });
}

// シートのUndo/Redo履歴を復元（なければリセット）
export function restore_undo_state(sheetId) {
    if (!editor) return;
    const um = editor.session.getUndoManager();
    if (sheetId && _undoStates.has(sheetId)) {
        const state = _undoStates.get(sheetId);
        um.$undoStack = state.undoStack;
        um.$redoStack = state.redoStack;
    } else {
        um.reset();
    }
}

// シート削除時にUndo履歴をクリア
export function clear_undo_state(sheetId) {
    if (sheetId) _undoStates.delete(sheetId);
}

export function load_editor_content_raw(content) {
    if (!editor) { pendingContent = content; return; }
    internalChange = true;
    try {
        editor.setValue(content || "", -1);
        editor.clearSelection();
        editor.moveCursorToPosition({ row: 0, column: 0 });
        editor.session.setScrollTop(0);
        editor.session.setScrollLeft(0);
        // Undo復元はRust側でrestore_undo_stateを呼ぶ
        pendingContent = null;
    } finally {
        internalChange = false;
    }
}

const loadedThemes = new Set();
export function set_editor_theme(theme_name) {
    if (!editor) return;
    const themePath = "ace/theme/" + theme_name;
    // テーマが既にロード済みか確認
    if (loadedThemes.has(theme_name) || ace.require(themePath)) {
        editor.setTheme(themePath);
        loadedThemes.add(theme_name);
        return;
    }
    // 未ロード → CDNからスクリプトを動的ロード
    const script = document.createElement("script");
    script.src = "https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-" + theme_name + ".js";
    script.crossOrigin = "anonymous";
    script.onload = () => {
        loadedThemes.add(theme_name);
        editor.setTheme(themePath);
    };
    script.onerror = () => console.warn("[Leaf] Failed to load theme:", theme_name);
    document.head.appendChild(script);
}

export function set_gutter_status(mode) {
    if (!editor) {
        pendingGutterUnsaved = mode;
        return;
    }
    const container = editor.container;
    // 一旦全てのステータスクラスを削除
    container.classList.remove("leaf-unsaved-gutter", "leaf-local-gutter");

    if (mode === "unsaved") {
        container.classList.add("leaf-unsaved-gutter");
    } else if (mode === "local") {
        container.classList.add("leaf-local-gutter");
    }
    pendingGutterUnsaved = null;
}

export function get_font_size() {
    if (!editor) {
        const stored = localStorage.getItem(getFontSizeKey());
        return stored ? parseFloat(stored) : 14;
    }
    return parseFloat(editor.getFontSize()) || 14;
}

export function change_font_size(delta) {
    if (!editor) return get_font_size();
    const currentStyle = editor.getFontSize();
    let currentSize = parseFloat(currentStyle);
    if (isNaN(currentSize)) currentSize = 14;
    const newSize = Math.max(8, Math.min(72, currentSize + delta));
    const sizeStr = newSize + "pt";
    editor.setFontSize(sizeStr);
    localStorage.setItem(getFontSizeKey(), sizeStr);

    // Rust 側へ通知
    window.dispatchEvent(new CustomEvent('leaf-font-size-changed', { detail: newSize }));

    return newSize;
}

export function generate_uuid() {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function (c) {
        var r = Math.random() * 16 | 0, v = c == 'x' ? r : (r & 0x3 | 0x8);
        return v.toString(16);
    });
}

export function render_markdown(text) {
    if (typeof marked === 'undefined') {
        // markedがまだロードされていない場合、遅延ロードをトリガーしてプレーンテキストを返す
        if (window.ensureMarked) window.ensureMarked(function() {});
        return '<pre style="white-space:pre-wrap;color:#ebdbb2;">' + text.replace(/</g, '&lt;').replace(/>/g, '&gt;') + '</pre>';
    }
    return marked.parse(text);
}

export function preload_markdown_libs() {
    if (window.ensureMarked) window.ensureMarked(function() {});
    if (window.ensureMermaid) window.ensureMermaid(function() {});
}

export function is_marked_loaded() {
    return typeof marked !== 'undefined';
}

export function set_window_opacity(opacity) {
    if (!is_tauri()) return;
    if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('set_window_opacity', { opacity: opacity });
    }
}

export function set_window_blur(blur) {
    if (!is_tauri()) return;
    if (window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('set_window_blur', { blur: blur });
        // CSSカスタムプロパティ＋クラスで制御（動的に追加される要素にも自動適用）
        if (blur > 0) {
            const bgAlpha = (1.0 - (blur / 100.0) * 0.9).toFixed(2);
            document.documentElement.style.setProperty('--blur-bg-alpha', bgAlpha);
            document.documentElement.style.background = 'transparent';
            document.body.style.background = 'transparent';
            document.documentElement.classList.add('blur-active');
        } else {
            document.documentElement.style.removeProperty('--blur-bg-alpha');
            document.documentElement.style.background = '';
            document.body.style.background = '';
            document.documentElement.classList.remove('blur-active');
        }
    }
}

export function is_macos_tauri() {
    if (!is_tauri()) return false;
    return window._is_macos_tauri || false;
}

export function is_windows_tauri() {
    if (!is_tauri()) return false;
    return window._is_windows_tauri || false;
}

// 起動時にOS判定をキャッシュ
if (is_tauri() && window.__TAURI__ && window.__TAURI__.core) {
    window.__TAURI__.core.invoke('is_macos').then(v => { window._is_macos_tauri = v; });
    window.__TAURI__.core.invoke('is_windows').then(v => { window._is_windows_tauri = v; });
}

// --- スプリット編集エディタ (Ace 第2インスタンス) ---
let _splitEditor = null;
let _splitEditorTimer = null;
let _splitEditorSheetId = null; // スプリットエディタで編集中のシートID

export function init_split_editor(element_id, content, filename, sheetId) {
    _splitEditorSheetId = sheetId || null;
    if (typeof ace === 'undefined') return;
    if (_splitEditor) { _splitEditor.destroy(); _splitEditor = null; }
    _splitEditor = ace.edit(element_id);
    // メインエディタと同じテーマを適用
    const theme = editor ? editor.getTheme() : ('ace/theme/' + (localStorage.getItem('leaf_editor_theme') || 'gruvbox'));
    _splitEditor.setTheme(theme);
    // ファイルモード（コードハイライト）を設定
    if (filename) {
        const modelist = ace.require("ace/ext/modelist");
        const mode = modelist.getModeForPath(filename).mode;
        _splitEditor.session.setMode(mode);
    }
    // Vim モードを同期
    const isVim = editor && editor.getKeyboardHandler && editor.getKeyboardHandler() && editor.getKeyboardHandler().$id === 'ace/keyboard/vim';
    if (isVim) _splitEditor.setKeyboardHandler('ace/keyboard/vim');
    _splitEditor.setOptions({
        fontSize: localStorage.getItem(getFontSizeKey()) || '14pt',
        fontFamily: "'JetBrains Mono', 'Fira Code', 'Courier New', monospace",
        showPrintMargin: false, useSoftTabs: true, tabSize: 4, wrap: true, indentedSoftWrap: true,
        enableBasicAutocompletion: false, enableLiveAutocompletion: false
    });
    _splitEditor.setValue(content || '', -1);
    _splitEditor.clearSelection();
    // Undo履歴を復元（メインエディタと共有）
    if (_splitEditorSheetId && _undoStates.has(_splitEditorSheetId)) {
        const state = _undoStates.get(_splitEditorSheetId);
        const um = _splitEditor.session.getUndoManager();
        um.$undoStack = state.undoStack;
        um.$redoStack = state.redoStack;
    } else {
        _splitEditor.session.getUndoManager().reset();
    }
    _splitEditor.on('change', () => {
        if (_splitEditorTimer) clearTimeout(_splitEditorTimer);
        _splitEditorTimer = setTimeout(() => {
            window.dispatchEvent(new CustomEvent('split-editor-changed'));
        }, 200);
    });
    setTimeout(() => { if (_splitEditor) { _splitEditor.resize(); _splitEditor.focus(); } }, 50);
}

export function destroy_split_editor() {
    // 破棄前に未保存のデバウンス中変更を即時フラッシュ（sheets_refに反映させる）
    if (_splitEditorTimer) {
        clearTimeout(_splitEditorTimer);
        _splitEditorTimer = null;
        if (_splitEditor) {
            // split-editor-changed イベントを同期的に発火してRust側の保存処理を走らせる
            window.dispatchEvent(new CustomEvent('split-editor-changed'));
        }
    }
    // Undo履歴を保存してからエディタを破棄
    if (_splitEditor && _splitEditorSheetId) {
        const um = _splitEditor.session.getUndoManager();
        _undoStates.set(_splitEditorSheetId, {
            undoStack: JSON.parse(JSON.stringify(um.$undoStack || [])),
            redoStack: JSON.parse(JSON.stringify(um.$redoStack || []))
        });
    }
    if (_splitEditor) { _splitEditor.destroy(); _splitEditor = null; }
    _splitEditorSheetId = null;
}

export function get_split_editor_content() {
    if (!_splitEditor) return '';
    return _splitEditor.getValue();
}

export function get_char_at_split_editor_cursor() {
    if (!_splitEditor) return '';
    const pos = _splitEditor.getCursorPosition();
    const line = _splitEditor.session.getLine(pos.row);
    if (!line || pos.column >= line.length) return '';
    const code = line.charCodeAt(pos.column);
    if (code >= 0xD800 && code <= 0xDBFF && pos.column + 1 < line.length) {
        return line.slice(pos.column, pos.column + 2);
    }
    return line[pos.column] || '';
}

export function focus_split_editor() {
    if (_splitEditor) _splitEditor.focus();
}

// スプリットエディタの内容をメインエディタに同期（edit mode 終了時）
export function sync_split_editor_to_main() {
    if (!_splitEditor || !editor) return;
    const content = _splitEditor.getValue();
    if (editor.getValue() === content) return;
    internalChange = true;
    try {
        editor.setValue(content, -1);
        editor.clearSelection();
    } finally {
        internalChange = false;
    }
}

// --- Terminal (xterm.js) - 複数インスタンス対応 ---
const _terminals = new Map(); // id -> { terminal, fitAddon, unlisten, exitUnlisten }
let _ptyOutputUnlisten = null;
let _ptyExitUnlisten = null;

async function ensureXtermLoaded() {
    if (window.Terminal) return;
    await new Promise((resolve, reject) => {
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = 'https://cdn.jsdelivr.net/npm/@xterm/xterm@5/css/xterm.min.css';
        document.head.appendChild(link);
        const script = document.createElement('script');
        script.src = 'https://cdn.jsdelivr.net/npm/@xterm/xterm@5/lib/xterm.min.js';
        script.onload = () => {
            const fitScript = document.createElement('script');
            fitScript.src = 'https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0/lib/addon-fit.min.js';
            fitScript.onload = resolve;
            fitScript.onerror = reject;
            document.head.appendChild(fitScript);
        };
        script.onerror = reject;
        document.head.appendChild(script);
    });
}

function ensureGlobalListeners() {
    if (_ptyOutputUnlisten) return;
    window.__TAURI__.event.listen('pty-output', (event) => {
        const { id, data } = event.payload;
        const inst = _terminals.get(id);
        if (inst && inst.terminal) {
            // 最初の出力でスピナーを非表示
            if (inst.spinner) {
                inst.spinner.remove();
                inst.spinner = null;
            }
            const bytes = Uint8Array.from(atob(data), c => c.charCodeAt(0));
            inst.terminal.write(bytes);
        }
    }).then(fn => { _ptyOutputUnlisten = fn; });
    window.__TAURI__.event.listen('pty-exit', (event) => {
        const { id } = event.payload;
        const inst = _terminals.get(id);
        // 既にexit処理中なら重複を無視
        if (inst && inst._exiting) return;
        if (inst && inst.terminal) {
            inst._exiting = true;
            inst.terminal.write('\r\n[Process exited]\r\n');
            // フェードアウト後にタブを閉じる
            if (inst.wrapper) {
                inst.wrapper.style.transition = 'opacity 0.1s ease-out';
                inst.wrapper.style.opacity = '0';
                setTimeout(() => {
                    window.dispatchEvent(new CustomEvent('terminal-exit', { detail: { id } }));
                }, 150);
            } else {
                window.dispatchEvent(new CustomEvent('terminal-exit', { detail: { id } }));
            }
        }
    }).then(fn => { _ptyExitUnlisten = fn; });
}

let _activeTermId = null;

export async function terminal_open(id, containerId, cols, rows) {
    if (!is_tauri()) return false;
    await ensureXtermLoaded();
    ensureGlobalListeners();

    const container = document.getElementById(containerId);
    if (!container) return false;

    // 既存の表示中ターミナルをデタッチ
    if (_activeTermId && _terminals.has(_activeTermId)) {
        const prev = _terminals.get(_activeTermId);
        if (prev.wrapper && prev.wrapper.parentNode === container) {
            container.removeChild(prev.wrapper);
        }
    }

    // 既にPTY起動済みなら再アタッチ
    if (_terminals.has(id)) {
        const inst = _terminals.get(id);
        container.appendChild(inst.wrapper);
        _activeTermId = id;
        if (inst.fitAddon) {
            const fitAndResize = () => {
                const rect = container.getBoundingClientRect();
                if (rect.width > 0 && rect.height > 0) {
                    inst.fitAddon.fit();
                    window.__TAURI__?.core?.invoke('pty_resize', {
                        id,
                        cols: inst.terminal.cols,
                        rows: inst.terminal.rows,
                    });
                } else {
                    setTimeout(fitAndResize, 30);
                }
            };
            setTimeout(fitAndResize, 50);
        }
        inst.terminal.focus();
        return true;
    }

    // 新規作成
    const wrapper = document.createElement('div');
    // overflow:hidden は Windows WebView2 の IME 入力時に xterm.js の hidden textarea が
    // 祖先コンテナをスクロールさせ、Ace エディタが左にずれる問題を防ぐために必要
    wrapper.style.cssText = 'width:100%;height:100%;position:relative;overflow:hidden;';
    container.appendChild(wrapper);

    // Windows WebView2 IME workaround: IME 変換中に WebView2 がスクロール位置を変更しても
    // wrapper および container のスクロールを即座に 0 にリセットする
    const resetScroll = () => { wrapper.scrollLeft = 0; wrapper.scrollTop = 0; };
    wrapper.addEventListener('scroll', resetScroll, { passive: true });
    const resetContainerScroll = () => { container.scrollLeft = 0; container.scrollTop = 0; };
    container.addEventListener('scroll', resetContainerScroll, { passive: true });

    // ローディングスピナー（PTY起動待ち）
    const spinner = document.createElement('div');
    spinner.style.cssText = 'position:absolute;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:center;background:#1d2021;z-index:10;gap:12px;';
    spinner.innerHTML = `
        <style>@keyframes leaf-spin{to{transform:rotate(360deg)}}</style>
        <div style="width:32px;height:32px;border:3px solid #3c3836;border-top-color:#98971a;border-radius:50%;animation:leaf-spin 0.8s linear infinite;"></div>
        <span style="color:#928374;font-size:12px;font-family:monospace;">Starting shell...</span>
    `;
    wrapper.appendChild(spinner);

    const terminal = new window.Terminal({
        cursorBlink: true, fontSize: 14,
        fontFamily: "'JetBrains Mono', 'Menlo', 'Monaco', 'Courier New', monospace",
        theme: {
            background: '#1d2021', foreground: '#ebdbb2', cursor: '#ebdbb2', selectionBackground: '#504945',
            black: '#282828', red: '#cc241d', green: '#98971a', yellow: '#d79921',
            blue: '#458588', magenta: '#b16286', cyan: '#689d6a', white: '#a89984',
            brightBlack: '#928374', brightRed: '#fb4934', brightGreen: '#b8bb26',
            brightYellow: '#fabd2f', brightBlue: '#83a598', brightMagenta: '#d3869b',
            brightCyan: '#8ec07c', brightWhite: '#ebdbb2',
        },
        allowProposedApi: true,
    });

    let fitAddon = null;
    if (window.FitAddon) {
        fitAddon = new window.FitAddon.FitAddon();
        terminal.loadAddon(fitAddon);
    }
    terminal.open(wrapper);
    if (fitAddon) setTimeout(() => fitAddon.fit(), 100);

    const fitCols = terminal.cols || cols || 80;
    const fitRows = terminal.rows || rows || 24;
    await window.__TAURI__.core.invoke('pty_spawn', { id, cols: fitCols, rows: fitRows });

    terminal.onData((data) => {
        window.__TAURI__.core.invoke('pty_write', { id, data });
    });

    if (fitAddon) {
        new ResizeObserver(() => {
            if (fitAddon && terminal) {
                // コンテナが表示されている時のみリサイズ（非表示時は幅0になるため無視）
                const rect = wrapper.getBoundingClientRect();
                if (rect.width > 20 && rect.height > 20) {
                    fitAddon.fit();
                    window.__TAURI__.core.invoke('pty_resize', { id, cols: terminal.cols, rows: terminal.rows });
                }
            }
        }).observe(wrapper);
    }

    _terminals.set(id, { terminal, fitAddon, wrapper, spinner, resetScroll, resetContainerScroll, container });
    _activeTermId = id;
    return true;
}

export function terminal_close(id) {
    const inst = _terminals.get(id);
    if (inst) {
        if (inst.resetScroll && inst.wrapper) inst.wrapper.removeEventListener('scroll', inst.resetScroll);
        if (inst.resetContainerScroll && inst.container) inst.container.removeEventListener('scroll', inst.resetContainerScroll);
        if (inst.wrapper && inst.wrapper.parentNode) inst.wrapper.parentNode.removeChild(inst.wrapper);
        if (inst.terminal) inst.terminal.dispose();
        _terminals.delete(id);
    }
    if (_activeTermId === id) _activeTermId = null;
    if (is_tauri() && window.__TAURI__ && window.__TAURI__.core) {
        window.__TAURI__.core.invoke('pty_kill', { id });
    }
}

export function terminal_is_open(id) {
    return _terminals.has(id);
}

export function terminal_set_font_size(size) {
    const clamped = Math.max(8, Math.min(32, size));
    for (const [id, inst] of _terminals.entries()) {
        if (inst.terminal) {
            inst.terminal.options.fontSize = clamped;
            if (inst.fitAddon) {
                inst.fitAddon.fit();
                window.__TAURI__?.core?.invoke('pty_resize', {
                    id,
                    cols: inst.terminal.cols,
                    rows: inst.terminal.rows,
                });
            }
        }
    }
    return clamped;
}

export function terminal_focus(id) {
    const container = document.getElementById('terminal-area');
    if (!container) return;
    // 別のターミナルから切り替え
    if (_activeTermId && _activeTermId !== id && _terminals.has(_activeTermId)) {
        const prev = _terminals.get(_activeTermId);
        if (prev.wrapper && prev.wrapper.parentNode === container) {
            container.removeChild(prev.wrapper);
        }
    }
    const inst = _terminals.get(id);
    if (inst) {
        if (inst.wrapper && inst.wrapper.parentNode !== container) {
            container.appendChild(inst.wrapper);
        }
        // terminal-areaが表示されレイアウト確定後にfit+resizeを実行
        if (inst.fitAddon) {
            const fitAndResize = () => {
                const rect = container.getBoundingClientRect();
                if (rect.width > 0 && rect.height > 0) {
                    inst.fitAddon.fit();
                    window.__TAURI__?.core?.invoke('pty_resize', {
                        id,
                        cols: inst.terminal.cols,
                        rows: inst.terminal.rows,
                    });
                } else {
                    // まだ非表示なので少し待って再試行
                    setTimeout(fitAndResize, 30);
                }
            };
            setTimeout(fitAndResize, 50);
        }
        inst.terminal.focus();
        _activeTermId = id;
    }
}

export function init_mermaid(element) {
    if (typeof mermaid === 'undefined') return;
    mermaid.run({
        nodes: element.querySelectorAll('.language-mermaid'),
        suppressErrors: true,
    }).then(() => {
        // Mermaid描画完了後にカーソル同期を再実行（SVGの高さが変わるため）
        requestAnimationFrame(() => syncPreviewToLine());
    }).catch(() => {});
}

export function set_preview_active(active) {
    previewActive = active;
}

export function highlight_code(code, lang) {
    if (typeof hljs === 'undefined') return code;

    // 拡張子から hljs の言語名へのマッピング
    const langMap = {
        'rs': 'rust',
        'js': 'javascript',
        'ts': 'typescript',
        'py': 'python',
        'md': 'markdown',
        'sh': 'bash',
        'yml': 'yaml',
        'coffee': 'coffeescript',
        'toml': 'ini'
    };

    const targetLang = langMap[lang] || lang;
    const language = hljs.getLanguage(targetLang) ? targetLang : null;

    try {
        if (language) {
            return hljs.highlight(code, { language }).value;
        } else {
            // 言語が特定できない場合は自動判定
            return hljs.highlightAuto(code).value;
        }
    } catch (e) {
        console.error("[Leaf-SYSTEM] Highlighting failed:", e);
        return code;
    }
}

export function exec_editor_command(command) {
    if (editor) {
        editor.execCommand(command);
    }
}

export function set_editor_mode(filename) {
    if (!editor) {
        pendingMode = filename;
        return;
    }
    const modelist = ace.require("ace/ext/modelist");
    const mode = modelist.getModeForPath(filename).mode;
    console.log("[Leaf-SYSTEM] Setting editor mode to", mode, "for filename", filename);
    editor.session.setMode(mode);
    pendingMode = null;
}

export function scroll_into_view_graceful(container, index, duration_ms) {
    if (!container) return;
    const item = container.children[index];
    if (!item) return;

    const containerRect = container.getBoundingClientRect();
    const itemRect = item.getBoundingClientRect();

    let targetScrollTop = container.scrollTop;

    // 要素が上にはみ出している場合、または下にはみ出している場合にスクロール位置を計算
    if (itemRect.top < containerRect.top) {
        targetScrollTop -= (containerRect.top - itemRect.top + 8); // 上部に少し余裕を持たせる
    } else if (itemRect.bottom > containerRect.bottom) {
        targetScrollTop += (itemRect.bottom - containerRect.bottom + 8); // 下部に少し余裕を持たせる
    } else {
        return; // 既に完全に見えている場合は何もしない
    }

    const start = container.scrollTop;
    const change = targetScrollTop - start;
    const startTime = performance.now();

    function animate(currentTime) {
        const elapsed = currentTime - startTime;
        const progress = Math.min(elapsed / duration_ms, 1);

        // Easing: easeInOutQuad
        const ease = progress < 0.5 ? 2 * progress * progress : -1 + (4 - 2 * progress) * progress;

        container.scrollTop = start + change * ease;
        if (progress < 1) requestAnimationFrame(animate);
    }
    requestAnimationFrame(animate);
}

// --- ピンチジェスチャーによるフォントサイズ変更 ---
(function() {
    let initialDistance = 0;
    let lastFontChangeDistance = 0;
    const PINCH_THRESHOLD = 30; // px移動でフォントサイズ1段階変更

    function getDistance(touches) {
        const dx = touches[0].clientX - touches[1].clientX;
        const dy = touches[0].clientY - touches[1].clientY;
        return Math.sqrt(dx * dx + dy * dy);
    }

    document.addEventListener('touchstart', function(e) {
        if (e.touches.length === 2) {
            initialDistance = getDistance(e.touches);
            lastFontChangeDistance = initialDistance;
        }
    }, { passive: true });

    document.addEventListener('touchmove', function(e) {
        if (e.touches.length === 2) {
            e.preventDefault();
            const currentDistance = getDistance(e.touches);
            const delta = currentDistance - lastFontChangeDistance;

            if (Math.abs(delta) >= PINCH_THRESHOLD) {
                const steps = Math.trunc(delta / PINCH_THRESHOLD);
                change_font_size(steps);
                lastFontChangeDistance += steps * PINCH_THRESHOLD;
            }
        }
    }, { passive: false });

    document.addEventListener('touchend', function(e) {
        if (e.touches.length < 2) {
            initialDistance = 0;
            lastFontChangeDistance = 0;
        }
    }, { passive: true });
})();
