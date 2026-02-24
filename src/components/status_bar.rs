use yew::prelude::*;
use crate::i18n::{self, Language};

#[derive(Properties, PartialEq)]
pub struct StatusBarProps {
    pub network_status: bool, // true = connected, false = disconnected
    pub is_saving: bool,
    pub vim_mode: bool,
    pub on_toggle_vim: Callback<()>,
    pub category_name: String,
    pub file_name: String,
    pub file_extension: String,
    pub on_change_extension: Callback<String>,
}

#[function_component(StatusBar)]
pub fn status_bar(props: &StatusBarProps) -> Html {
    let lang = Language::detect();
    let extensions = crate::app::SUPPORTED_EXTENSIONS;

    html! {
        <div class="flex portrait:flex-col items-center portrait:items-stretch justify-between px-4 py-1 portrait:py-4 bg-gray-800 border-t border-gray-700 text-xs text-gray-400 select-none portrait:space-y-4">
            <div class="flex portrait:flex-col items-center portrait:items-stretch space-x-4 portrait:space-x-0 portrait:space-y-4">
                
                <div class="flex portrait:flex-col items-center portrait:items-stretch space-x-2 portrait:space-x-0 portrait:space-y-2">
                    <button
                        onclick={props.on_toggle_vim.reform(|_| ())}
                        class={classes!(
                            "px-2", "py-0.5", "portrait:py-3", "portrait:text-sm", "rounded", "text-[10px]", "font-bold", "transition-colors", "portrait:w-full",
                            if props.vim_mode { vec!["bg-green-600", "text-white", "hover:bg-green-700"] } else { vec!["bg-gray-600", "text-gray-300", "hover:bg-gray-500"] }
                        )}
                        title={i18n::t("toggle_vim", lang)}
                    >
                        { if props.vim_mode { "Vim: ON" } else { "Vim: OFF" } }
                    </button>

                    if !props.category_name.is_empty() {
                        <select 
                            value={props.file_extension.clone()}
                            onchange={
                                let on_change = props.on_change_extension.clone();
                                Callback::from(move |e: Event| {
                                    let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
                                    on_change.emit(select.value());
                                })
                            }
                            class="bg-gray-700 text-gray-300 text-[10px] portrait:text-sm font-bold py-0.5 portrait:py-3 px-1 rounded border border-gray-600 outline-none hover:bg-gray-600 focus:border-emerald-500 transition-colors cursor-pointer portrait:w-full portrait:text-center text-center"
                        >
                            { for extensions.iter().map(|(ext, key)| {
                                html! {
                                    <option value={*ext} selected={*ext == props.file_extension}>
                                        { format!("{}: .{}", i18n::t(key, lang), ext) }
                                    </option>
                                }
                            }) }
                        </select>
                    }
                </div>

                if !props.file_name.is_empty() {
                    <span class="flex portrait:justify-center items-center space-x-2 border-l portrait:border-l-0 portrait:border-t portrait:pt-4 portrait:mt-2 border-gray-700 ml-4 portrait:ml-0 pl-4 portrait:pl-0 py-0.5 font-mono portrait:w-full portrait:text-sm">
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
                    </span>
                }
            </div>
            
            <div class="flex portrait:flex-col items-center portrait:items-center space-x-6 portrait:space-x-0 portrait:space-y-3 portrait:border-t portrait:border-gray-700 portrait:pt-4 portrait:pb-2 portrait:w-full">
                if props.is_saving {
                    <div class="flex items-center space-x-2 text-red-500 font-bold">
                        <div class="w-3 h-3 border-2 border-red-500 border-t-transparent rounded-full animate-spin"></div>
                        <span>{ i18n::t("saving", lang) }</span>
                    </div>
                }

                <span class={classes!(
                    "flex", "items-center", "space-x-2", "font-semibold",
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
