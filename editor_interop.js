// editor_interop.js
// --- VERSION: 10000 (CLEAN FIX) ---
console.log("%c[Leaf-SYSTEM] NEW SCRIPT LOADED - VERSION 10000", "color: white; background: green; font-size: 20px;");

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

    // 安定化リサイズ
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
}

function setupGlobalKeys() {
    if (window._leaf_keys_attached) return;
    window._leaf_keys_attached = true;
    window.addEventListener('keydown', function(e) {
        if ((e.metaKey || e.ctrlKey) && e.key === 'w') { e.preventDefault(); if (commandCallback) commandCallback("close"); }
        if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); if (commandCallback) commandCallback("save"); }
        if ((e.metaKey || e.ctrlKey) && e.key === 't') { e.preventDefault(); if (commandCallback) commandCallback("new_sheet"); }
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
            
            if (!editor._vim_polling_v10) {
                console.log("[Leaf-SYSTEM] Starting mode observer...");
                setInterval(() => {
                    const h = editor.getKeyboardHandler();
                    if (h && h.state) {
                        const isInsert = h.state.insertMode;
                        if (isInsert && !container.classList.contains("leaf-insert-mode")) {
                            console.log("[Leaf-SYSTEM] Mode -> INSERT");
                            container.classList.add("leaf-insert-mode");
                        } else if (!isInsert && container.classList.contains("leaf-insert-mode")) {
                            console.log("[Leaf-SYSTEM] Mode -> NORMAL");
                            container.classList.remove("leaf-insert-mode");
                        }
                    }
                }, 100);
                editor._vim_polling_v10 = true;
            }
            editor.focus();
        });
    } else {
        editor.setKeyboardHandler(null);
        container.classList.remove("leaf-vim-enabled", "leaf-insert-mode");
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
