let editor;
let commandCallback;
let pendingContent = null;
let pendingGutterUnsaved = null; 
let pendingMode = null; // 追加: モード設定の待機用
let previewActive = false;
const FONT_SIZE_KEY = 'leaf_font_size';

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

    // カスタムコマンド (開く)
    editor.commands.addCommand({
        name: "openFileDialog",
        bindKey: { win: "Alt-O", mac: "Option-O" },
        exec: () => { if (commandCallback) commandCallback("open"); }
    });

    // カスタムコマンド (インポート)
    editor.commands.addCommand({
        name: "importFile",
        bindKey: { win: "Alt-I", mac: "Option-I" },
        exec: () => { if (commandCallback) commandCallback("import"); }
    });

    // カスタムコマンド (プレビュー)
    editor.commands.addCommand({
        name: "togglePreview",
        bindKey: { win: "Alt-M", mac: "Option-M" },
        exec: () => { if (commandCallback) commandCallback("preview"); }
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
    if (enabled) {
        editor.setKeyboardHandler("ace/keyboard/vim");
        container.classList.add("leaf-vim-enabled");
    } else {
        editor.setKeyboardHandler(null);
        container.classList.remove("leaf-vim-enabled", "leaf-normal-mode", "leaf-insert-mode");
        editor.focus();
    }
}

export function set_editor_content(content) {
    pendingContent = content;
    if (!editor) return;
    if (editor.getValue() !== content) {
        editor.setValue(content || "", -1);
        editor.clearSelection();
        editor.session.getUndoManager().reset();
        pendingContent = null;
    }
}

export function get_editor_content() { 
    if (!editor) return null;
    return editor.getValue(); 
}

export function resize_editor() { if (editor) editor.resize(); }
export function focus_editor() { if (editor) editor.focus(); }

export function set_gutter_status(unsaved) {
    if (!editor) {
        pendingGutterUnsaved = unsaved;
        return;
    }
    const container = editor.container;
    if (unsaved) {
        container.classList.add("leaf-unsaved-gutter");
    } else {
        container.classList.remove("leaf-unsaved-gutter");
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
        "coffee": "javascript",
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
    
    editor.session.setMode(mode);
    console.log(`[Leaf-SYSTEM] Editor mode set to ${mode} for extension .${ext}`);
}

export function set_preview_active(active) {
    previewActive = active;
}
