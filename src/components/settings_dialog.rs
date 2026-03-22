use yew::prelude::*;
use crate::i18n::{self, Language};

pub const THEMES: &[(&str, &str)] = &[
    ("gruvbox", "Gruvbox"),
    ("monokai", "Monokai"),
    ("dracula", "Tokyo Night"),
    ("nord_dark", "Nord"),
    ("solarized_dark", "Solarized Dark"),
    ("one_dark", "One Dark"),
    ("twilight", "Twilight"),
    ("tomorrow_night", "Tomorrow Night"),
];

#[derive(Properties, PartialEq)]
pub struct SettingsDialogProps {
    pub vim_mode: bool,
    pub on_toggle_vim: Callback<()>,
    pub current_theme: String,
    pub on_change_theme: Callback<String>,
    pub on_close: Callback<()>,
}

#[function_component(SettingsDialog)]
pub fn settings_dialog(props: &SettingsDialogProps) -> Html {
    let lang = Language::detect();
    let is_closing = use_state(|| false);

    let on_close = {
        let is_closing = is_closing.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |_: ()| {
            is_closing.set(true);
            let cb = on_close.clone();
            gloo::timers::callback::Timeout::new(200, move || { cb.emit(()); }).forget();
        })
    };

    html! {
        <div class="fixed inset-0 z-[200] flex items-center justify-center">
            // Backdrop
            <div
                class={classes!(
                    "absolute", "inset-0", "bg-black/60", "transition-opacity", "duration-200",
                    if *is_closing { "opacity-0" } else { "opacity-100" }
                )}
                onclick={let c = on_close.clone(); move |_| c.emit(())}
            ></div>
            // Dialog
            <div class={classes!(
                "relative", "z-10", "w-full", "max-w-md", "mx-4", "bg-[#1d2021]", "rounded-xl",
                "border", "border-[#3c3836]", "shadow-2xl", "overflow-hidden",
                "transition-all", "duration-200",
                if *is_closing { "opacity-0 scale-95" } else { "opacity-100 scale-100" }
            )}
                style="animation: dialog-in 0.2s ease-out;"
            >
                // Header
                <div class="flex items-center justify-between px-6 py-4 border-b border-[#3c3836]">
                    <div class="flex items-center space-x-3">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5 text-emerald-500">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 010 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 010-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28z" />
                            <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                        </svg>
                        <h2 class="text-base font-bold text-[#ebdbb2]">{ i18n::t("settings", lang) }</h2>
                    </div>
                    <button
                        onclick={let c = on_close.clone(); move |_| c.emit(())}
                        class="p-1 rounded hover:bg-[#3c3836] text-gray-400 hover:text-white transition-colors"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-5 h-5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                // Settings items
                <div class="px-6 py-5 space-y-5">
                    // Vim Mode
                    <div class="flex items-center justify-between">
                        <div>
                            <div class="text-sm font-bold text-[#ebdbb2]">{ i18n::t("vim_mode", lang) }</div>
                            <div class="text-xs text-gray-500 mt-0.5">{ "H/J/K/L navigation, modal editing" }</div>
                        </div>
                        <button
                            onclick={props.on_toggle_vim.reform(|_| ())}
                            class={classes!(
                                "relative", "w-11", "h-6", "rounded-full", "transition-colors", "duration-200", "cursor-pointer", "shrink-0",
                                if props.vim_mode { "bg-emerald-500" } else { "bg-gray-600" }
                            )}
                        >
                            <div class={classes!(
                                "absolute", "top-0.5", "w-5", "h-5", "bg-white", "rounded-full", "shadow",
                                "transition-transform", "duration-200",
                                if props.vim_mode { "translate-x-[22px]" } else { "translate-x-0.5" }
                            )}></div>
                        </button>
                    </div>

                    // Separator
                    <div class="border-t border-[#3c3836]"></div>

                    // Editor Theme
                    <div>
                        <div class="text-sm font-bold text-[#ebdbb2] mb-3">{ i18n::t("editor_theme", lang) }</div>
                        <div class="grid grid-cols-2 gap-2">
                            { for THEMES.iter().map(|(id, name)| {
                                let is_selected = props.current_theme == *id;
                                let on_change = props.on_change_theme.clone();
                                let theme_id = id.to_string();
                                html! {
                                    <button
                                        onclick={Callback::from(move |_| on_change.emit(theme_id.clone()))}
                                        class={classes!(
                                            "py-2", "px-3", "rounded-lg", "text-xs", "font-bold", "transition-all", "duration-150",
                                            "border", "text-left",
                                            if is_selected {
                                                "bg-emerald-600 text-white border-emerald-500 shadow-lg shadow-emerald-500/20"
                                            } else {
                                                "bg-[#282828] text-gray-400 border-[#3c3836] hover:bg-[#3c3836] hover:text-gray-200"
                                            }
                                        )}
                                    >
                                        { *name }
                                    </button>
                                }
                            })}
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
