// editor_interop.js
// --- ver1.5 ---
console.log("%c[Leaf-SYSTEM] NEW SCRIPT LOADED - ver1.5", "color: white; background: #008888; font-size: 16px;");

let editor;
let commandCallback;

export function generate_uuid() {
    return crypto.randomUUID();
}

export function set_window_title(title) {
    document.title = title;
}

export function init_editor(elementId, callback) {
    console.log("[Leaf-SYSTEM] init_editor called");
    
    // 既にエディタが初期化されている場合は、コールバックだけ更新して終了
    if (editor) {
        console.log("[Leaf-SYSTEM] Editor already exists, updating callback.");
        commandCallback = callback;
        return;
    }

    if (!window.ace) {
        console.error("Ace editor not loaded");
        return;
    }
    const element = document.getElementById(elementId);
    if (!element) {
        console.error("Editor element not found: " + elementId);
        return;
    }

    editor = ace.edit(elementId);
    editor.setTheme("ace/theme/twilight");
    editor.session.setMode("ace/mode/text");
    
    editor.setOptions({
        fontSize: "14pt",
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

    let count = 0;
    const interval = setInterval(() => {
        editor.resize();
        if (++count > 10) clearInterval(interval);
    }, 100);
}

function setupCommands() {
    editor.commands.addCommand({
        name: "save",
        bindKey: {win: "Ctrl-S", mac: "Command-S"},
        exec: function(editor) { if (commandCallback) commandCallback("save"); }
    });
    editor.commands.addCommand({
        name: "close",
        bindKey: {win: "Ctrl-W", mac: "Command-W"},
        exec: function(editor) { if (commandCallback) commandCallback("close"); }
    });
    editor.commands.addCommand({
        name: "new_sheet",
        bindKey: {win: "Ctrl-T", mac: "Command-T"},
        exec: function(editor) { if (commandCallback) commandCallback("new_sheet"); }
    });
    // 標準のfindコマンドはAceに組み込まれているため、ここでは定義不要です
}

function setupGlobalKeys() {
    if (window._leaf_keys_attached) return;
    window._leaf_keys_attached = true;
    window.addEventListener('keydown', function(e) {
        if ((e.metaKey || e.ctrlKey) && e.key === 'w') { e.preventDefault(); if (commandCallback) commandCallback("close"); }
        if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); if (commandCallback) commandCallback("save"); }
        if ((e.metaKey || e.ctrlKey) && e.key === 't') { e.preventDefault(); if (commandCallback) commandCallback("new_sheet"); }
        if ((e.metaKey || e.ctrlKey) && e.key === 'f') { 
            // Ace本体の組み込みコマンドを直接実行（無限ループを回避）
            e.preventDefault(); 
            editor.execCommand("find");
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
                console.log("[Leaf-SYSTEM] Starting ver1.4 mode observer...");
                
                setInterval(() => {
                    const h = editor.getKeyboardHandler();
                    if (!h) return;

                    // あらゆる可能性から挿入モードを判定
                    const isInsert = 
                        (h.state && h.state.insertMode) || 
                        (h.$vimModeHandler && h.$vimModeHandler.state && h.$vimModeHandler.state.insertMode) ||
                        (editor.state && editor.state.cm && editor.state.cm.state.vim && editor.state.cm.state.vim.insertMode);

                    if (isInsert) {
                        if (!container.classList.contains("leaf-insert-mode")) {
                            console.log("[Leaf-SYSTEM] Mode -> INSERT");
                            container.classList.remove("leaf-normal-mode");
                            container.classList.add("leaf-insert-mode");
                        }
                    } else {
                        if (container.classList.contains("leaf-insert-mode")) {
                            console.log("[Leaf-SYSTEM] Mode -> NORMAL");
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
    if (!editor) return;
    if (editor.getValue() !== content) {
        editor.setValue(content || "", -1);
        editor.clearSelection();
    }
}

export function get_editor_content() { return editor ? editor.getValue() : ""; }
export function resize_editor() { if (editor) editor.resize(); }
