let editor;
let commandCallback;
let pendingContent = null;
let pendingGutterUnsaved = null; 
let pendingMode = null; // 追加: モード設定の待機用
let previewActive = false;
let localFileHandle = null;
const FONT_SIZE_KEY = 'leaf_font_size';

export function can_install_pwa() {
    const prompt = window.leafDeferredPrompt;
    console.log("[Leaf-PWA] can_install_pwa checked. Status:", !!prompt);
    return !!prompt;
}

export async function trigger_pwa_install() {
    const prompt = window.leafDeferredPrompt;
    if (!prompt) {
        console.warn("[Leaf-PWA] trigger_pwa_install called but prompt is null.");
        return false;
    }
    console.log("[Leaf-PWA] Triggering installation prompt...");
    prompt.prompt();
    const { outcome } = await prompt.userChoice;
    console.log("[Leaf-PWA] Installation prompt outcome:", outcome);
    window.leafDeferredPrompt = null;
    return outcome === 'accepted';
}

export function is_webkit_or_safari() {
    const ua = window.navigator.userAgent.toLowerCase();
    const isSafari = ua.indexOf('safari') !== -1 && ua.indexOf('chrome') === -1;
    const isMobileSafari = (ua.indexOf('iphone') !== -1 || ua.indexOf('ipad') !== -1) && ua.indexOf('safari') !== -1;
    return isSafari || isMobileSafari;
}

export async function open_local_file() {
    try {
        const [handle] = await window.showOpenFilePicker({
            types: [{ 
                description: 'Code and Text Files', 
                accept: { 
                    'text/plain': ['.txt', '.md', '.js', '.ts', '.rs', '.toml', '.json', '.yaml', '.yml', '.sql', '.html', '.css', '.py', '.c', '.cpp', '.h', '.m', '.cs', '.php', '.coffee', '.pl', '.rb', '.java', '.sh', '.xml'] 
                } 
            }],
            excludeAcceptAllOption: false,
            multiple: false
        });
        localFileHandle = handle;
        const file = await handle.getFile();
        const text = await file.text();
        return { name: file.name, content: text };
    } catch (e) {
        if (e.name === 'AbortError') return null;
        console.error("Local open failed:", e);
        return null;
    }
}

export async function save_local_file(content) {
    try {
        // ハンドルがない場合は新規作成ダイアログを表示
        if (!localFileHandle) {
            localFileHandle = await window.showSaveFilePicker({
                suggestedName: 'Untitled.txt',
                types: [{
                    description: 'Text Files',
                    accept: {'text/plain': ['.txt', '.md', '.js', '.ts', '.rs', '.toml', '.json', '.yaml', '.yml', '.sql', '.html', '.css', '.py', '.c', '.cpp', '.h', '.m', '.cs', '.php', '.coffee', '.pl', '.rb', '.java', '.sh', '.xml']}
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
}

export function set_window_title(title) {
    document.title = title ? `${title} - Leaf` : "Leaf";
}

export function init_editor(element_id, callback) {
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
        if (commandCallback) commandCallback("change");
    });

    // スクロールイベント (インクリメンタル読み込み用)
    editor.session.on("changeScrollTop", (scrollTop) => {
        const session = editor.session;
        const renderer = editor.renderer;
        
        // 現在の表示内容の高さと、全体の高さを比較
        // 下端から 200px 以内になったら通知
        if (scrollTop + renderer.$size.height > session.getScreenLength() * renderer.lineHeight - 200) {
            console.log("[Leaf-SYSTEM] Triggering load_more (scrollTop: " + scrollTop + ")");
            if (commandCallback) commandCallback("load_more");
        }
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

    editor.setValue(content || "", -1);
    editor.clearSelection();
    editor.session.getUndoManager().reset();
    pendingContent = null;
}

export function get_editor_content() { 
    if (!editor) return null;
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

export function change_font_size(delta) {
    if (!editor) return;
    const currentStyle = editor.getFontSize();
    let currentSize = parseFloat(currentStyle);
    if (isNaN(currentSize)) currentSize = 14;
    const newSize = Math.max(8, Math.min(72, currentSize + delta));
    const sizeStr = newSize + "pt";
    editor.setFontSize(sizeStr);
    localStorage.setItem(FONT_SIZE_KEY, sizeStr);
}

export function generate_uuid() {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
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

export function set_editor_mode(filename) {
    if (!editor) {
        pendingMode = filename;
        return;
    }
    const parts = filename.split('.');
    const ext = parts.length > 1 ? parts.pop().toLowerCase() : "";
    let mode = "ace/mode/text";
    
    const modeMap = {
        "js": "javascript",
        "ts": "typescript",
        "coffee": "coffee",
        "rs": "rust",
        "md": "markdown",
        "markdown": "markdown",
        "html": "html",
        "css": "css",
        "json": "json",
        "py": "python",
        "sh": "sh",
        "bash": "sh",
        "zsh": "sh",
        "pl": "perl",
        "php": "php",
        "rb": "ruby",
        "cs": "csharp",
        "cpp": "c_cpp",
        "c": "c_cpp",
        "h": "c_cpp",
        "m": "c_cpp",
        "java": "java",
        "toml": "toml",
        "yaml": "yaml",
        "yml": "yaml",
        "xml": "xml",
        "sql": "sql"
    };

    if (modeMap[ext]) {
        mode = "ace/mode/" + modeMap[ext];
    } else {
        // デフォルトは以前の JavaScript ではなく text に戻す（不明な拡張子のため）
        mode = "ace/mode/text";
    }
    
    if (editor.session.$modeId !== mode) {
        editor.session.setMode(mode);
        console.log(`[Leaf-SYSTEM] Editor mode set to ${mode} for extension .${ext}`);
    }
}

export function set_preview_active(active) {
    previewActive = active;
}

export function append_editor_content(content) {
    if (!editor) return;
    const session = editor.session;
    const lastRow = session.getLength();
    const lastCol = session.getLine(lastRow - 1).length;
    session.insert({ row: lastRow, column: lastCol }, content);
}

export function exec_editor_command(command) {
    if (editor) {
        editor.execCommand(command);
    }
}
