// editor_interop.js
// --- ver1.12 ---
console.log("%c[Leaf-SYSTEM] NEW SCRIPT LOADED - ver1.12", "color: white; background: #008888; font-size: 16px;");

let editor;
let commandCallback;
let pendingContent = null;
const FONT_SIZE_KEY = 'leaf_font_size';

export function generate_uuid() {
    return crypto.randomUUID();
}

export function set_window_title(title) {
    document.title = title;
}

export function init_editor(elementId, callback) {
    console.log("[Leaf-SYSTEM] init_editor called for:", elementId);
    
    const element = document.getElementById(elementId);
    if (!element) {
        console.warn("[Leaf-SYSTEM] Editor element not found, retrying in 100ms...");
        setTimeout(() => init_editor(elementId, callback), 100);
        return;
    }

    // 既にエディタが存在し、かつコンテナが現在の要素と同じであれば、コールバックのみ更新
    if (editor && editor.container && document.body.contains(editor.container) && editor.container.id === elementId) {
        console.log("[Leaf-SYSTEM] Editor already exists and attached, updating callback.");
        commandCallback = callback;
        if (pendingContent !== null) {
            console.log("[Leaf-SYSTEM] Applying pending content to existing editor.");
            set_editor_content(pendingContent);
        }
        editor.resize();
        return;
    }

    // エディタが存在するが、コンテナがDOMから消えている、あるいは別の要素になった場合は破棄して再作成
    if (editor) {
        console.log("[Leaf-SYSTEM] Re-initializing editor due to DOM change.");
        editor.destroy();
        editor = null;
    }

    if (!window.ace) {
        console.error("Ace editor not loaded");
        return;
    }

    editor = ace.edit(elementId);
    editor.setTheme("ace/theme/twilight");
    editor.session.setMode("ace/mode/text");
    
    const savedFontSize = localStorage.getItem(FONT_SIZE_KEY) || "14pt";
    editor.setOptions({
        fontSize: savedFontSize,
        showLineNumbers: true,
        showGutter: true,
        useWorker: false,
        wrap: true
    });

    commandCallback = callback;
    setupCommands();
    setupGlobalKeys();

    editor.session.on('change', function(delta) {
        if (commandCallback) commandCallback("change");
    });

    // 待機中の内容があれば反映
    if (pendingContent !== null) {
        console.log("[Leaf-SYSTEM] Applying pending content after init.");
        set_editor_content(pendingContent);
    }

    // 初期化直後のリサイズを複数回行う
    let count = 0;
    const interval = setInterval(() => {
        if (editor) {
            editor.resize();
            if (editor.renderer && editor.renderer.lineHeight > 0) {
                clearInterval(interval);
            }
        }
        if (++count > 20) clearInterval(interval);
    }, 100);
}

function setupCommands() {
    editor.commands.addCommand({
        name: "save",
        bindKey: {win: "Alt-S", mac: "Option-S"},
        exec: function(editor) { if (commandCallback) commandCallback("save"); }
    });
    editor.commands.addCommand({
        name: "new_sheet",
        bindKey: {win: "Alt-N", mac: "Option-N"},
        exec: function(editor) { if (commandCallback) commandCallback("new_sheet"); }
    });
    editor.commands.addCommand({
        name: "decreaseFontSize",
        bindKey: {win: "Alt--", mac: "Option--"},
        exec: function(editor) { change_font_size(-1); }
    });
    editor.commands.addCommand({
        name: "increaseFontSize",
        bindKey: {win: "Alt-=", mac: "Option-="},
        exec: function(editor) { change_font_size(1); }
    });
}

function setupGlobalKeys() {
    if (window._leaf_keys_attached) return;
    window._leaf_keys_attached = true;
    window.addEventListener('keydown', function(e) {
        if (e.altKey && e.code === 'KeyS') { e.preventDefault(); if (commandCallback) commandCallback("save"); }
        if (e.altKey && e.code === 'KeyN') { e.preventDefault(); if (commandCallback) commandCallback("new_sheet"); }
        if (e.altKey && e.code === 'KeyO') { e.preventDefault(); if (commandCallback) commandCallback("open"); }
        if (e.altKey && e.code === 'Minus') { e.preventDefault(); change_font_size(-1); }
        if (e.altKey && e.code === 'Equal') { e.preventDefault(); change_font_size(1); }
        if (e.altKey && e.code === 'KeyF') { 
            e.preventDefault(); 
            if (editor) editor.execCommand("find");
        }
    }, {passive: false});
}

export function set_vim_mode(enabled) {
    console.log("[Leaf-SYSTEM] set_vim_mode:", enabled);
    if (!editor) {
        setTimeout(() => set_vim_mode(enabled), 100);
        return;
    }
    const container = editor.container;
    if (enabled) {
        ace.config.loadModule("ace/keyboard/vim", function(m) {
            editor.setKeyboardHandler(m.handler);
            container.classList.add("leaf-vim-enabled");
            
            if (!editor._vim_v1_4_setup) {
                setInterval(() => {
                    const h = editor.getKeyboardHandler();
                    if (!h) return;
                    const isInsert = 
                        (h.state && h.state.insertMode) || 
                        (h.$vimModeHandler && h.$vimModeHandler.state && h.$vimModeHandler.state.insertMode) ||
                        (editor.state && editor.state.cm && editor.state.cm.state.vim && editor.state.cm.state.vim.insertMode);

                    if (isInsert) {
                        if (!container.classList.contains("leaf-insert-mode")) {
                            container.classList.remove("leaf-normal-mode");
                            container.classList.add("leaf-insert-mode");
                        }
                    } else {
                        if (container.classList.contains("leaf-insert-mode")) {
                            container.classList.remove("leaf-insert-mode");
                            container.classList.add("leaf-normal-mode");
                        }
                    }
                }, 100);
                editor._vim_v1_4_setup = true;
            }
            editor.focus();
        });
    } else {
        editor.setKeyboardHandler(null);
        container.classList.remove("leaf-vim-enabled", "leaf-normal-mode", "leaf-insert-mode");
        editor.focus();
    }
}

export function set_editor_content(content) {
    pendingContent = content;
    if (!editor) {
        console.log("[Leaf-SYSTEM] Editor not ready, content pended.");
        return;
    }
    if (editor.getValue() !== content) {
        editor.setValue(content || "", -1);
        editor.clearSelection();
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
    if (!editor) return;
    const container = editor.container;
    if (unsaved) {
        container.classList.add("leaf-unsaved-gutter");
    } else {
        container.classList.remove("leaf-unsaved-gutter");
    }
}

export function change_font_size(delta) {
    if (!editor) return;
    // 現在のサイズを取得（文字列 "14pt" などから数値を抽出）
    const currentStyle = editor.getFontSize();
    let currentSize = parseFloat(currentStyle);
    
    // パース失敗時のフォールバック
    if (isNaN(currentSize)) currentSize = 14;

    const newSize = Math.max(8, Math.min(72, currentSize + delta));
    const sizeStr = newSize + "pt";
    
    editor.setFontSize(sizeStr);
    localStorage.setItem(FONT_SIZE_KEY, sizeStr);
    console.log("[Leaf-SYSTEM] Font size changed to:", sizeStr);
}
