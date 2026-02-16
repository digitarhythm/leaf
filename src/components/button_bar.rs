use yew::prelude::*;
use crate::i18n::{self, Language};

#[derive(Properties, PartialEq)]
pub struct ButtonBarProps {
    pub on_new_sheet: Callback<()>,
    pub on_open: Callback<()>,
    pub on_import: Callback<()>,
    pub on_change_font_size: Callback<i32>,
    pub on_change_category: Callback<String>, 
    pub on_help: Callback<()>,
    pub on_logout: Callback<()>,
    pub current_category: String,             
    pub categories: Vec<crate::db_interop::JSCategory>,
    pub is_new_sheet: bool,
    pub is_dropdown_open: bool,
    pub on_toggle_dropdown: Callback<bool>,
}

#[function_component(ButtonBar)]
pub fn button_bar(props: &ButtonBarProps) -> Html {
    let lang = Language::detect();

    let has_multiple_cats = props.categories.len() > 1;

    let on_category_click = {
        let on_toggle = props.on_toggle_dropdown.clone();
        let is_open = props.is_dropdown_open;
        let current_cat = props.current_category.clone();
        Callback::from(move |_| {
            if has_multiple_cats || current_cat.is_empty() {
                on_toggle.emit(!is_open);
            }
        })
    };

    let current_cat_name = if props.current_category.is_empty() {
        if props.is_new_sheet {
            i18n::t("no_category", lang)
        } else {
            i18n::t("local_file", lang)
        }
    } else {
        props.categories.iter()
            .find(|c| c.id == props.current_category)
            .map(|c| if c.name == "OTHERS" { i18n::t("OTHERS", lang) } else { c.name.clone() })
            .unwrap_or_else(|| i18n::t("OTHERS", lang))
    };

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
            <button
                onclick={props.on_import.reform(|_| ())}
                class="p-1.5 rounded hover:bg-gray-700 text-white"
                title={i18n::t("import_file", lang)}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 16.5V9.75m0 0l3 3m-3-3l-3 3M6.75 19.5a4.5 4.5 0 01-1.41-8.775 5.25 5.25 0 0110.233-2.33 3 3 0 013.758 3.848A3.752 3.752 0 0118 19.5H6.75z" />
                </svg>
            </button>

            // Category Selector
            <div class="relative inline-block text-left">
                <button
                    onclick={on_category_click}
                    class={classes!(
                        "ml-2", "px-3", "py-1", "rounded", "bg-gray-700", "text-gray-200", "text-xs", "font-bold", "flex", "items-center", "space-x-1", "transition-colors",
                        if has_multiple_cats || props.current_category.is_empty() { "hover:bg-gray-600 cursor-pointer" } else { "cursor-default" }
                    )}
                >
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3 h-3">
                        <path d="M3.75 3A1.75 1.75 0 002 4.75v10.5c0 .966.784 1.75 1.75 1.75h12.5A1.75 1.75 0 0018 15.25V4.75A1.75 1.75 0 0016.25 3H3.75zM10 6.5a.75.75 0 01.75.75v2.5h2.5a.75.75 0 010 1.5h-2.5v2.5a.75.75 0 01-1.5 0v-2.5h-2.5a.75.75 0 010-1.5h2.5v-2.5A.75.75 0 0110 6.5z" />
                    </svg>
                    <span>{ current_cat_name }</span>
                    if has_multiple_cats || props.current_category.is_empty() {
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3 h-3 opacity-50">
                            <path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
                        </svg>
                    }
                </button>

                if props.is_dropdown_open && (has_multiple_cats || props.current_category.is_empty()) {
                    <>
                        <div 
                            onclick={let on_t = props.on_toggle_dropdown.clone(); move |_| on_t.emit(false)}
                            class="fixed inset-0 z-[140]"
                        ></div>
                        <div class="absolute left-2 mt-1 w-48 rounded-md shadow-2xl bg-gray-800 border border-gray-700 z-[150] overflow-hidden animate-in fade-in zoom-in duration-100">
                            <div class="py-1 max-h-60 overflow-y-auto">
                                if props.current_category.is_empty() {
                                    <button
                                        class="w-full text-left px-4 py-2 text-xs bg-blue-600 text-white font-bold cursor-default"
                                    >
                                        { if props.is_new_sheet { i18n::t("no_category", lang) } else { i18n::t("local_file", lang) } }
                                    </button>
                                }
                                { for props.categories.iter().map(|cat| {
                                    let id = cat.id.clone();
                                    let name = cat.name.clone();
                                    let display_name = if name == "OTHERS" { i18n::t("OTHERS", lang) } else { name };
                                    let on_change = props.on_change_category.clone();
                                    let on_toggle = props.on_toggle_dropdown.clone();
                                    let is_active = cat.id == props.current_category;
                                    
                                    html! {
                                        <button
                                            onclick={if is_active { Callback::from(|_| ()) } else { Callback::from(move |_| { on_change.emit(id.clone()); on_toggle.emit(false); }) }}
                                            class={classes!(
                                                "w-full", "text-left", "px-4", "py-2", "text-xs", "transition-colors",
                                                if is_active { "bg-blue-600 text-white font-bold cursor-default" } else { "text-gray-300 hover:bg-gray-700 hover:text-white" }
                                            )}
                                        >
                                            { display_name }
                                        </button>
                                    }
                                }) }
                            </div>
                        </div>
                    </>
                }
            </div>
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
            <div class="flex-1"></div>
            <button
                onclick={props.on_help.reform(|_| ())}
                class="p-1.5 rounded hover:bg-gray-700 text-gray-400 hover:text-white mr-2"
                title={i18n::t("help", lang)}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9 5.25h.008v.008H12v-.008z" />
                </svg>
            </button>
            <button
                onclick={props.on_logout.reform(|_| ())}
                class="p-1.5 rounded hover:bg-gray-700 text-gray-400 hover:text-white mr-2"
                title={i18n::t("logout", lang)}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 9V5.25A2.25 2.25 0 0013.5 3h-6a2.25 2.25 0 00-2.25 2.25v13.5A2.25 2.25 0 007.5 21h6a2.25 2.25 0 002.25-2.25V15m3 0l3-3m0 0l-3-3m3 3H9" />
                </svg>
            </button>
            <span 
                class="text-green-500 opacity-60 font-bold px-4 text-xl select-none"
                style="font-family: 'Petit Formal Script', cursive;"
            >
                {"Leaf"}
            </span>
        </div>
    }
}
