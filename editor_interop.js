let editor;
let commandCallback;
let pendingContent = null;
let pendingGutterUnsaved = null;
let pendingMode = null; // 追加: モード設定の待機用
let previewActive = false;
let localFileHandle = null;
let localFilePath = null; // Tauri用: ネイティブファイルパス保持
let internalChange = false;
const FONT_SIZE_KEY = 'leaf_font_size';

export function can_install_pwa() {
    const prompt = window.leafDeferredPrompt;
    console.log("[Leaf-PWA] can_install_pwa checked. Status:", !!prompt);
    return !!prompt;
}

export function is_tauri() {
    return !!window.__TAURI__;
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
            return { name: result.name, content: result.content, bytes: new Uint8Array(result.bytes) };
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

export async function save_local_file(content, needs_bom) {
    if (is_tauri()) {
        try {
            const result = await window.__TAURI__.core.invoke('save_local_file_native', {
                content: content,
                needsBom: needs_bom,
                currentPath: localFilePath || null
            });
            localFilePath = result.path;
            return result.name;
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
    editor.setTheme("ace/theme/gruvbox");
    editor.session.setMode("ace/mode/javascript");

    // 基本設定
    editor.setOptions({
        fontSize: localStorage.getItem(FONT_SIZE_KEY) || "14pt",
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

    // 待機中の内容があれば反映
    if (pendingContent !== null) {
        set_editor_content(pendingContent);
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

export function resize_editor() { if (editor) editor.resize(); }
export function focus_editor() { if (editor) editor.focus(); }

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
        const stored = localStorage.getItem(FONT_SIZE_KEY);
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
    localStorage.setItem(FONT_SIZE_KEY, sizeStr);

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
    if (typeof marked === 'undefined') return text;
    return marked.parse(text);
}

export function init_mermaid(element) {
    if (typeof mermaid === 'undefined') return;
    mermaid.run({
        nodes: element.querySelectorAll('.language-mermaid')
    });
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
        'yml': 'yaml'
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
