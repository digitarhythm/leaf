use yew::prelude::*;
use wasm_bindgen::JsCast;
use crate::i18n::{self, Language};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EmptySaveBehavior {
    Delete,
    Nothing,
    Confirm,
}

impl EmptySaveBehavior {
    pub fn from_str(s: &str) -> Self {
        match s {
            "delete" => Self::Delete,
            "nothing" => Self::Nothing,
            _ => Self::Confirm,
        }
    }
    pub fn to_str(self) -> &'static str {
        match self {
            Self::Delete => "delete",
            Self::Nothing => "nothing",
            Self::Confirm => "confirm",
        }
    }
}

pub const DARK_THEMES: &[(&str, &str)] = &[
    ("gruvbox", "Gruvbox"),
    ("monokai", "Monokai"),
    ("dracula", "Dracula"),
    ("nord_dark", "Nord"),
    ("solarized_dark", "Solarized Dark"),
    ("one_dark", "One Dark"),
    ("twilight", "Twilight"),
    ("tomorrow_night", "Tomorrow Night"),
];

pub const LIGHT_THEMES: &[(&str, &str)] = &[
    ("chrome", "Chrome"),
    ("clouds", "Clouds"),
    ("crimson_editor", "Crimson Editor"),
    ("dawn", "Dawn"),
    ("dreamweaver", "Dreamweaver"),
    ("eclipse", "Eclipse"),
    ("github", "GitHub"),
    ("solarized_light", "Solarized Light"),
];

#[derive(Properties, PartialEq)]
pub struct SettingsDialogProps {
    pub vim_mode: bool,
    pub on_toggle_vim: Callback<()>,
    pub current_theme: String,
    pub on_change_theme: Callback<String>,
    pub empty_save_behavior: EmptySaveBehavior,
    pub on_change_empty_save: Callback<EmptySaveBehavior>,
    #[prop_or(100)]
    pub window_opacity: i32,
    #[prop_or_default]
    pub on_change_opacity: Option<Callback<i32>>,
    #[prop_or_default]
    pub window_blur: i32,
    #[prop_or_default]
    pub on_change_blur: Option<Callback<i32>>,
    #[prop_or(14)]
    pub terminal_font_size: i32,
    #[prop_or_default]
    pub on_change_terminal_font_size: Option<Callback<i32>>,
    #[prop_or_default]
    pub is_guest_mode: bool,
    #[prop_or_default]
    pub local_auto_save: bool,
    #[prop_or_default]
    pub on_toggle_local_auto_save: Option<Callback<()>>,
    #[prop_or_default]
    pub on_google_login: Option<Callback<()>>,
    pub on_close: Callback<()>,
}

#[function_component(SettingsDialog)]
pub fn settings_dialog(props: &SettingsDialogProps) -> Html {
    let lang = Language::detect();
    let is_closing = use_state(|| false);

    // ESCキーで閉じる
    {
        let on_close_esc = props.on_close.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let listener = gloo::events::EventListener::new(&window, "keydown", move |e| {
                let ke = e.dyn_ref::<web_sys::KeyboardEvent>().unwrap();
                if ke.key() == "Escape" {
                    e.stop_immediate_propagation();
                    on_close_esc.emit(());
                }
            });
            Box::new(move || drop(listener)) as Box<dyn FnOnce()>
        });
    }

    let is_light = LIGHT_THEMES.iter().any(|(id, _)| *id == props.current_theme.as_str());
    let theme_tab = use_state(move || is_light); // false=Dark, true=Light

    let on_close = {
        let is_closing = is_closing.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |_: ()| {
            is_closing.set(true);
            let cb = on_close.clone();
            gloo::timers::callback::Timeout::new(300, move || { cb.emit(()); }).forget();
        })
    };

    html! {
        <div class="fixed inset-0 z-[200] flex items-center justify-center">
            // Backdrop
            <div
                class={classes!(
                    "absolute", "inset-0", "bg-black/60", "transition-opacity", "duration-300",
                    if *is_closing { "opacity-0" } else { "opacity-100" }
                )}
                onclick={let c = on_close.clone(); move |_| c.emit(())}
            ></div>
            // Dialog
            <div class={classes!(
                "relative", "z-10", "w-full", "max-w-md", "mx-4", "bg-[#1d2021]", "rounded-xl",
                "border", "border-[#3c3836]", "shadow-2xl", "overflow-hidden",
                "transition-all", "duration-300",
                if *is_closing { "opacity-0 scale-95" } else { "opacity-100 scale-100" }
            )}
                style="animation: dialog-in 0.3s ease-out;"
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

                    // Local Auto-Save
                    <div class="flex items-center justify-between">
                        <div>
                            <div class="text-sm font-bold text-[#ebdbb2]">{ i18n::t("local_auto_save", lang) }</div>
                            <div class="text-xs text-gray-500 mt-0.5">{ i18n::t("local_auto_save_desc", lang) }</div>
                        </div>
                        if let Some(ref on_toggle) = props.on_toggle_local_auto_save {
                            <button
                                onclick={on_toggle.reform(|_| ())}
                                class={classes!(
                                    "relative", "w-11", "h-6", "rounded-full", "transition-colors", "duration-200", "cursor-pointer", "shrink-0",
                                    if props.local_auto_save { "bg-emerald-500" } else { "bg-gray-600" }
                                )}
                            >
                                <div class={classes!(
                                    "absolute", "top-0.5", "w-5", "h-5", "bg-white", "rounded-full", "shadow",
                                    "transition-transform", "duration-200",
                                    if props.local_auto_save { "translate-x-[22px]" } else { "translate-x-0.5" }
                                )}></div>
                            </button>
                        }
                    </div>

                    // Separator
                    <div class="border-t border-[#3c3836]"></div>

                    // Empty Save Behavior
                    <div>
                        <div class="text-sm font-bold text-[#ebdbb2] mb-3">{ i18n::t("empty_save_behavior", lang) }</div>
                        <div class="flex flex-col gap-2">
                            { for [(EmptySaveBehavior::Confirm, "empty_save_confirm"), (EmptySaveBehavior::Delete, "empty_save_delete"), (EmptySaveBehavior::Nothing, "empty_save_nothing")].iter().map(|(behavior, key)| {
                                let is_selected = props.empty_save_behavior == *behavior;
                                let on_change = props.on_change_empty_save.clone();
                                let b = *behavior;
                                html! {
                                    <button
                                        onclick={Callback::from(move |_| on_change.emit(b))}
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
                                        { i18n::t(key, lang) }
                                    </button>
                                }
                            })}
                        </div>
                    </div>

                    // Window Opacity (Tauri macOS only)
                    if let Some(ref on_opacity) = props.on_change_opacity {
                        <div class="border-t border-[#3c3836]"></div>
                        <div>
                            <div class="flex items-center justify-between mb-2">
                                <div class="text-sm font-bold text-[#ebdbb2]">{ i18n::t("window_opacity", lang) }</div>
                                <span class="text-xs text-gray-400 font-mono">{ format!("{}%", props.window_opacity) }</span>
                            </div>
                            <input type="range" min="50" max="100" step="5"
                                value={props.window_opacity.to_string()}
                                onchange={let cb = on_opacity.clone(); Callback::from(move |e: Event| { let input: web_sys::HtmlInputElement = e.target_unchecked_into(); if let Ok(v) = input.value().parse::<i32>() { cb.emit(v); } })}
                                oninput={let cb = on_opacity.clone(); Callback::from(move |e: InputEvent| { let input: web_sys::HtmlInputElement = e.target_unchecked_into(); if let Ok(v) = input.value().parse::<i32>() { cb.emit(v); } })}
                                class="w-full h-2 bg-[#3c3836] rounded-lg appearance-none cursor-pointer accent-emerald-500"
                            />
                            <div class="flex justify-between text-[10px] text-gray-600 mt-1">
                                <span>{ "50%" }</span><span>{ "75%" }</span><span>{ "100%" }</span>
                            </div>
                        </div>
                    }

                    // Window Blur (Tauri Windows only)
                    if let Some(ref on_blur) = props.on_change_blur {
                        <div class="border-t border-[#3c3836]"></div>
                        <div>
                            <div class="flex items-center justify-between mb-2">
                                <div class="text-sm font-bold text-[#ebdbb2]">{ i18n::t("window_blur", lang) }</div>
                                <span class="text-xs text-gray-400 font-mono">{ format!("{}%", props.window_blur) }</span>
                            </div>
                            <input type="range" min="0" max="100" step="10"
                                value={props.window_blur.to_string()}
                                onchange={let cb = on_blur.clone(); Callback::from(move |e: Event| { let input: web_sys::HtmlInputElement = e.target_unchecked_into(); if let Ok(v) = input.value().parse::<i32>() { cb.emit(v); } })}
                                oninput={let cb = on_blur.clone(); Callback::from(move |e: InputEvent| { let input: web_sys::HtmlInputElement = e.target_unchecked_into(); if let Ok(v) = input.value().parse::<i32>() { cb.emit(v); } })}
                                class="w-full h-2 bg-[#3c3836] rounded-lg appearance-none cursor-pointer accent-emerald-500"
                            />
                            <div class="flex justify-between text-[10px] text-gray-600 mt-1">
                                <span>{ "OFF" }</span><span>{ "50%" }</span><span>{ "100%" }</span>
                            </div>
                        </div>
                    }

                    // Terminal Font Size (Tauri only)
                    if let Some(ref on_tfs) = props.on_change_terminal_font_size {
                        <div class="border-t border-[#3c3836]"></div>
                        <div>
                            <div class="flex items-center justify-between">
                                <div class="text-sm font-bold text-[#ebdbb2]">{ i18n::t("terminal_font_size", lang) }</div>
                                <div class="flex items-center space-x-2">
                                    <button
                                        onclick={let cb = on_tfs.clone(); let v = props.terminal_font_size; Callback::from(move |_| cb.emit(v - 1))}
                                        class="w-7 h-7 flex items-center justify-center rounded bg-[#282828] border border-[#3c3836] text-gray-300 hover:bg-[#3c3836] hover:text-white transition-colors font-bold text-base"
                                    >{ "-" }</button>
                                    <span class="text-sm font-mono text-[#ebdbb2] w-8 text-center">{ props.terminal_font_size }</span>
                                    <button
                                        onclick={let cb = on_tfs.clone(); let v = props.terminal_font_size; Callback::from(move |_| cb.emit(v + 1))}
                                        class="w-7 h-7 flex items-center justify-center rounded bg-[#282828] border border-[#3c3836] text-gray-300 hover:bg-[#3c3836] hover:text-white transition-colors font-bold text-base"
                                    >{ "+" }</button>
                                </div>
                            </div>
                        </div>
                    }

                    // Separator
                    <div class="border-t border-[#3c3836]"></div>

                    // Editor Theme
                    <div>
                        <div class="text-sm font-bold text-[#ebdbb2] mb-2">{ i18n::t("editor_theme", lang) }</div>
                        // Dark/Light タブ
                        <div class="flex mb-3 border-b border-[#3c3836]">
                            <button
                                onclick={let t = theme_tab.clone(); Callback::from(move |_| t.set(false))}
                                class={classes!(
                                    "flex-1", "py-1.5", "text-xs", "font-bold", "transition-colors", "border-b-2",
                                    if !*theme_tab { "text-[#ebdbb2] border-b-emerald-500" } else { "text-gray-500 border-b-transparent hover:text-gray-300" }
                                )}
                            >{ "Dark" }</button>
                            <button
                                onclick={let t = theme_tab.clone(); Callback::from(move |_| t.set(true))}
                                class={classes!(
                                    "flex-1", "py-1.5", "text-xs", "font-bold", "transition-colors", "border-b-2",
                                    if *theme_tab { "text-[#ebdbb2] border-b-emerald-500" } else { "text-gray-500 border-b-transparent hover:text-gray-300" }
                                )}
                            >{ "Light" }</button>
                        </div>
                        <div class="grid grid-cols-2 gap-2">
                            { for (if *theme_tab { LIGHT_THEMES } else { DARK_THEMES }).iter().map(|(id, name)| {
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

                    // Google Login (guest mode only, at bottom)
                    if props.is_guest_mode {
                        if let Some(ref on_login) = props.on_google_login {
                            <div class="border-t border-[#3c3836]"></div>
                            <div>
                                <div class="text-sm font-bold text-[#ebdbb2] mb-1">{ i18n::t("google_login", lang) }</div>
                                <div class="text-xs text-gray-500 mb-3">{ i18n::t("google_login_desc", lang) }</div>
                                <button
                                    onclick={on_login.reform(|_| ())}
                                    class="w-full py-2 px-4 rounded-lg text-sm font-bold bg-emerald-600 hover:bg-emerald-700 text-white transition-colors"
                                >
                                    { i18n::t("google_login", lang) }
                                </button>
                            </div>
                        }
                    }
                </div>
            </div>
        </div>
    }
}
