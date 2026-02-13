use yew::prelude::*;
use crate::i18n::{self, Language};

#[derive(Properties, PartialEq)]
pub struct ButtonBarProps {
    pub on_new_sheet: Callback<()>,
    pub on_open: Callback<()>,
    pub on_toggle_vim: Callback<()>,
    pub on_change_font_size: Callback<i32>,
    pub vim_mode: bool,
}

#[function_component(ButtonBar)]
pub fn button_bar(props: &ButtonBarProps) -> Html {
    let lang = Language::detect();

    html! {
        <div class="flex items-center space-x-2 bg-gray-800 py-1 px-2 border-b border-gray-700">
            <button
                onclick={props.on_new_sheet.reform(|_| ())}
                class="p-1.5 rounded hover:bg-gray-700 text-white"
                title={i18n::t("new_sheet", lang)}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m3.75 9v6m3-3H9m1.5-12H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                </svg>
            </button>
            <button
                onclick={props.on_open.reform(|_| ())}
                class="p-1.5 rounded hover:bg-gray-700 text-white"
                title={i18n::t("open_file", lang)}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.06-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z" />
                </svg>
            </button>
            <div class="flex items-center space-x-1 ml-2 mr-4 border-l border-gray-700 pl-4">
                <button
                    onclick={props.on_change_font_size.reform(|_| -1)}
                    class="p-1 w-8 h-8 rounded hover:bg-gray-700 text-gray-400 hover:text-white flex items-center justify-center font-bold"
                    title="Decrease Font Size"
                >
                    {"ー"}
                </button>
                <button
                    onclick={props.on_change_font_size.reform(|_| 1)}
                    class="p-1 w-8 h-8 rounded hover:bg-gray-700 text-gray-400 hover:text-white flex items-center justify-center font-bold"
                    title="Increase Font Size"
                >
                    {"＋"}
                </button>
            </div>
            <button
                onclick={props.on_toggle_vim.reform(|_| ())}
                class={classes!(
                    "px-2", "py-0.5", "rounded", "text-xs", "font-medium", "transition-colors",
                    if props.vim_mode { vec!["bg-green-600", "text-white", "hover:bg-green-700"] } else { vec!["bg-gray-600", "text-gray-300", "hover:bg-gray-500"] }
                )}
                title={i18n::t("toggle_vim", lang)}
            >
                { if props.vim_mode { "Vim: ON" } else { "Vim: OFF" } }
            </button>
            <div class="flex-1"></div>
            <span 
                class="text-green-500 opacity-30 font-bold px-4 text-xl select-none"
                style="font-family: 'Petit Formal Script', cursive;"
            >
                {"Leaf"}
            </span>
        </div>
    }
}
