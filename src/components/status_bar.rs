use yew::prelude::*;
use crate::i18n::{self, Language};

#[derive(Properties, PartialEq)]
pub struct StatusBarProps {
    pub network_status: bool, // true = connected, false = disconnected
    pub is_saving: bool,
    pub on_open_settings: Callback<()>,
    #[prop_or_default]
    pub on_toggle_terminal: Option<Callback<()>>,
    #[prop_or_default]
    pub is_terminal_open: bool,
    #[prop_or_default]
    pub is_terminal_active: bool,
    pub category_name: String,
    pub file_name: String,
}

#[function_component(StatusBar)]
pub fn status_bar(props: &StatusBarProps) -> Html {
    let lang = Language::detect();
    html! {
        <div class="flex mobile:flex-col items-center mobile:items-stretch justify-between px-4 py-1 mobile:px-2 mobile:py-2 bg-gray-800 border-t border-gray-700 text-xs text-gray-400 select-none mobile:space-y-1">
            <div
                class="mobile:hidden flex items-center cursor-pointer hover:bg-gray-700/50 rounded transition-colors pr-2"
                onclick={props.on_open_settings.reform(|_| ())}
                title={i18n::t("settings", lang)}
            >
                <div class="flex items-center px-1 py-0.5 text-gray-500 hover:text-gray-300">
                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-3.5 h-3.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 010 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 010-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28z" />
                        <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                    </svg>
                </div>
                

                <span class="mobile:hidden flex items-center space-x-2 border-l border-gray-700 ml-2 pl-2 py-0.5 font-mono">
                    if props.is_terminal_active {
                        // ターミナルアクティブ時: モニター＋キーボードアイコン＋「ターミナル」
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-3.5 h-3.5 text-emerald-400">
                            <path fill-rule="evenodd" d="M1.5 1A1.5 1.5 0 000 2.5v10A1.5 1.5 0 001.5 14h21A1.5 1.5 0 0024 12.5v-10A1.5 1.5 0 0022.5 1h-21zM2 3h20v9H2V3z" clip-rule="evenodd"/>
                            <path d="M10 14.5h4V17h-4z"/>
                            <rect x="0.5" y="18" width="23" height="5.5" rx="1"/>
                        </svg>
                        <span class="text-emerald-400 font-medium">{ i18n::t("terminal", lang) }</span>
                    } else if !props.file_name.is_empty() {
                        if props.category_name.is_empty() || props.category_name == "__LOCAL__" {
                            if props.category_name.is_empty() && props.file_name == "----" {
                                // 未保存新規シートアイコン (Red X circle)
                                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5 text-red-500">
                                    <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd" />
                                </svg>
                            } else {
                                // ローカルファイルアイコン (White Document)
                                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5 text-white">
                                    <path fill-rule="evenodd" d="M4 4a2 2 0 012-2h4.586A2 2 0 0112 2.586L15.414 6A2 2 0 0116 7.414V16a2 2 0 01-2 2H6a2 2 0 01-2-2V4z" clip-rule="evenodd" />
                                </svg>
                            }
                            <span class="text-gray-300 font-medium">
                                { if props.category_name == "__LOCAL__" {
                                    format!("[{}] {}", i18n::t("local_file", lang), props.file_name)
                                } else {
                                    props.file_name.clone()
                                } }
                            </span>
                        } else {
                            // Google アイコン (G logo)
                            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-3.5 h-3.5 text-emerald-500">
                                <path d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c1.67-1.54 2.63-3.81 2.63-6.09z" />
                                <path d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-1 .67-2.28 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
                                <path d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" />
                                <path d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" />
                            </svg>
                            <span class="text-gray-300 font-medium">{ &props.file_name }</span>
                        }
                    }
                </span>
            </div>
            
            <div class="flex items-center space-x-6 mobile:space-x-0 mobile:justify-center mobile:pt-1 mobile:w-full mobile:text-[10px]">
                if props.is_saving {
                    <div class="flex items-center space-x-2 text-red-500 font-bold">
                        <div class="w-3 h-3 mobile:w-2 mobile:h-2 mobile:border-[1.5px] border-2 border-red-500 border-t-transparent rounded-full animate-spin"></div>
                        <span>{ i18n::t("saving", lang) }</span>
                    </div>
                }


                // ターミナルボタン（Tauri版のみ）
                if let Some(ref on_term) = props.on_toggle_terminal {
                    <button
                        onclick={on_term.reform(|_| ())}
                        class={classes!(
                            "flex", "items-center", "space-x-1", "px-2", "py-0.5", "rounded",
                            "transition-colors", "cursor-pointer", "font-semibold",
                            if props.is_terminal_open { "text-emerald-400 bg-gray-700" } else { "text-gray-500 hover:text-gray-300 hover:bg-gray-700" }
                        )}
                        title={i18n::t("terminal", lang)}
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-3.5 h-3.5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M6.75 7.5l3 2.25-3 2.25m4.5 0h3m-9 8.25h13.5A2.25 2.25 0 0021 18V6a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 6v12a2.25 2.25 0 002.25 2.25z" />
                        </svg>
                    </button>
                }

                <span class={classes!(
                    "flex", "items-center", "space-x-2", "font-semibold",
                    if props.is_saving { "mobile:hidden" } else { "" },
                    if props.network_status { "text-green-500" } else { "text-red-500" }
                )}>
                    <span class={classes!(
                        "w-2", "h-2", "rounded-full",
                        if props.network_status { "bg-green-500" } else { "bg-red-500" }
                    )}></span>
                    <span>
                        { if props.network_status { 
                            i18n::t("network_connected", lang)
                        } else { 
                            i18n::t("offline", lang)
                        } }
                    </span>
                </span>
            </div>
        </div>
    }
}
