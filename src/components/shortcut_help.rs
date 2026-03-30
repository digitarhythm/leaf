use yew::prelude::*;
use crate::i18n::{self, Language};
use gloo::timers::callback::Timeout;
use wasm_bindgen::JsCast;

#[derive(Properties, PartialEq)]
pub struct ShortcutHelpProps {
    pub on_close: Callback<()>,
    #[prop_or_default]
    pub on_install: Option<Callback<()>>,
}

struct ShortcutRow {
    key: &'static str,
    action_ja: &'static str,
    action_en: &'static str,
}

const SHORTCUTS: &[ShortcutRow] = &[
    ShortcutRow { key: "Opt/Alt + s",       action_ja: "強制保存（新規シートはドライブへ保存）", action_en: "Force Save (new sheets saved to Drive)" },
    ShortcutRow { key: "Opt/Alt + n",       action_ja: "新規シート作成",                         action_en: "New Sheet" },
    ShortcutRow { key: "Opt/Alt + Shift + n", action_ja: "新規ローカルファイル作成",              action_en: "New Local File" },
    ShortcutRow { key: "Opt/Alt + f",       action_ja: "検索ダイアログ表示",                     action_en: "Show Search Dialog" },
    ShortcutRow { key: "Opt/Alt + o",       action_ja: "ローカルファイルを開く",                  action_en: "Open Local File" },
    ShortcutRow { key: "Opt/Alt + m",       action_ja: "編集シート選択ダイアログ",               action_en: "Sheet Selection Dialog" },
    ShortcutRow { key: "Opt/Alt + l",       action_ja: "Markdownプレビュー表示",                 action_en: "Toggle Markdown Preview" },
    ShortcutRow { key: "Opt/Alt + h",       action_ja: "このヘルプを表示",                       action_en: "Show This Help" },
    ShortcutRow { key: "Opt/Alt + ,",       action_ja: "設定を開く",                             action_en: "Open Settings" },
    ShortcutRow { key: "Opt/Alt + t",       action_ja: "新規ターミナルを開く",                   action_en: "New Terminal" },
    ShortcutRow { key: "Opt/Alt + [",       action_ja: "左のタブに切り替え",                     action_en: "Switch to Left Tab" },
    ShortcutRow { key: "Opt/Alt + ]",       action_ja: "右のタブに切り替え",                     action_en: "Switch to Right Tab" },
    ShortcutRow { key: "Opt/Alt + w",       action_ja: "現在のタブを閉じる",                     action_en: "Close Current Tab" },
    ShortcutRow { key: "Opt/Alt + =",       action_ja: "フォントサイズを大きくする",              action_en: "Increase Font Size" },
    ShortcutRow { key: "Opt/Alt + -",       action_ja: "フォントサイズを小さくする",              action_en: "Decrease Font Size" },
    ShortcutRow { key: "Esc",               action_ja: "ダイアログ / プレビュー / ドロップダウンを閉じる", action_en: "Close Dialog / Preview / Dropdown" },
];

#[function_component(ShortcutHelp)]
pub fn shortcut_help(props: &ShortcutHelpProps) -> Html {
    let lang = Language::detect();
    let is_ja = lang == Language::Ja;
    let is_closing = use_state(|| false);

    let handle_close = {
        let on_close = props.on_close.clone();
        let is_closing = is_closing.clone();
        Callback::from(move |_: ()| {
            is_closing.set(true);
            let cb = on_close.clone();
            Timeout::new(150, move || { cb.emit(()); }).forget();
        })
    };

    // ESCキーで閉じる
    {
        let hc = handle_close.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let mut opts = gloo::events::EventListenerOptions::run_in_capture_phase();
            opts.passive = false;
            let listener = gloo::events::EventListener::new_with_options(&window, "keydown", opts, move |e| {
                let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                if ke.key() == "Escape" {
                    e.stop_immediate_propagation();
                    hc.emit(());
                }
            });
            Box::new(move || drop(listener)) as Box<dyn FnOnce()>
        });
    }

    let title = if is_ja { "ショートカット一覧" } else { "Keyboard Shortcuts" };
    let key_header = if is_ja { "キー" } else { "Key" };
    let action_header = if is_ja { "機能" } else { "Action" };

    let anim_class = if *is_closing {
        "opacity-0 scale-95"
    } else {
        "opacity-100 scale-100"
    };

    html! {
        <div class="fixed inset-0 z-[200] flex items-center justify-center">
            // Backdrop
            <div
                class={classes!(
                    "absolute", "inset-0", "bg-black/60", "transition-opacity", "duration-150",
                    if *is_closing { "opacity-0" } else { "opacity-100" }
                )}
                onclick={{let hc = handle_close.clone(); move |_| hc.emit(())}}
            ></div>

            // Dialog
            <div class={classes!(
                "relative", "z-10", "w-full", "max-w-lg", "mx-4",
                "bg-[#1d2021]", "rounded-xl", "border", "border-[#3c3836]", "shadow-2xl",
                "transition-all", "duration-150", anim_class
            )}>
                // Header
                <div class="flex items-center justify-between px-6 py-4 border-b border-[#3c3836]">
                    <div class="flex items-center gap-3">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5 text-emerald-500">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M6.75 7.5l3 2.25-3 2.25m4.5 0h3m-9 8.25h13.5A2.25 2.25 0 0021 18V6a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 6v12a2.25 2.25 0 002.25 2.25z" />
                        </svg>
                        <h2 class="text-base font-bold text-[#ebdbb2]">{ title }</h2>
                    </div>
                    <button
                        onclick={{let hc = handle_close.clone(); move |_| hc.emit(())}}
                        class="p-1 rounded hover:bg-[#3c3836] text-gray-400 hover:text-white transition-colors"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-5 h-5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                // Shortcut table
                <div class="overflow-y-auto max-h-[60vh] px-4 py-3">
                    <table class="w-full text-xs border-collapse">
                        <thead>
                            <tr class="border-b border-[#3c3836]">
                                <th class="text-left py-2 px-3 text-gray-400 font-semibold w-2/5">{ key_header }</th>
                                <th class="text-left py-2 px-3 text-gray-400 font-semibold">{ action_header }</th>
                            </tr>
                        </thead>
                        <tbody>
                            { for SHORTCUTS.iter().map(|row| {
                                let action = if is_ja { row.action_ja } else { row.action_en };
                                html! {
                                    <tr class="border-b border-[#3c3836]/50 hover:bg-[#282828]/50 transition-colors">
                                        <td class="py-2 px-3">
                                            <kbd class="inline-block bg-[#282828] border border-[#504945] rounded px-1.5 py-0.5 text-[11px] font-mono text-[#d4be98] whitespace-nowrap">
                                                { row.key }
                                            </kbd>
                                        </td>
                                        <td class="py-2 px-3 text-gray-300">{ action }</td>
                                    </tr>
                                }
                            }) }
                        </tbody>
                    </table>
                </div>

                // Footer (PWAインストールボタン)
                if let Some(ref on_install) = props.on_install {
                    <div class="px-6 py-4 border-t border-[#3c3836]">
                        <button
                            onclick={{let cb = on_install.clone(); move |_| cb.emit(())}}
                            class="w-full py-2 px-4 rounded-lg text-sm font-bold bg-emerald-600 hover:bg-emerald-700 text-white transition-colors"
                        >
                            { i18n::t("install_title", lang) }
                        </button>
                    </div>
                }
            </div>
        </div>
    }
}
