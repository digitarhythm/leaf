use yew::prelude::*;
use crate::i18n::{self, Language};

#[derive(Properties, PartialEq)]
pub struct ButtonBarProps {
    pub on_new_sheet: Callback<()>,
    pub on_open: Callback<()>,
    pub on_import: Callback<()>,
    pub on_change_font_size: Callback<i32>,
    pub on_change_category: Callback<String>,
    pub on_preview: Callback<()>,
    pub on_help: Callback<()>,
    pub on_logout: Callback<()>,
    pub current_category: String,
    pub categories: Vec<crate::db_interop::JSCategory>,
    pub is_new_sheet: bool,
    pub is_dropdown_open: bool,
    pub on_toggle_dropdown: Callback<bool>,
    pub vim_mode: bool,
    pub on_open_settings: Callback<()>,
    pub file_extension: String,
    pub on_change_extension: Callback<String>,
    #[prop_or_default]
    pub sheet_count: usize,
    #[prop_or_default]
    pub on_open_sheet_list: Option<Callback<()>>,
}

#[function_component(ButtonBar)]
pub fn button_bar(props: &ButtonBarProps) -> Html {
    let lang = Language::detect();
    let is_hamburger_open = use_state(|| false);
    let has_multiple_cats = props.categories.len() > 1;
    let extensions = crate::app::SUPPORTED_EXTENSIONS;

    let on_category_click = {
        let on_toggle = props.on_toggle_dropdown.clone();
        let is_open = props.is_dropdown_open;
        let current_cat = props.current_category.clone();
        Callback::from(move |_| {
            if has_multiple_cats || current_cat.is_empty() || current_cat == "__LOCAL__" {
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
    } else if props.current_category == "__LOCAL__" {
        i18n::t("local_file", lang)
    } else {
        props.categories.iter()
            .find(|c| c.id == props.current_category)
            .map(|c| if c.name == "OTHERS" { i18n::t("OTHERS", lang) } else { c.name.clone() })
            .unwrap_or_else(|| i18n::t("OTHERS", lang))
    };

    // カテゴリを「OTHERS」が先頭に来るようにソート
    let mut sorted_categories = props.categories.clone();
    sorted_categories.sort_by(|a, b| {
        if a.name == "OTHERS" { std::cmp::Ordering::Less }
        else if b.name == "OTHERS" { std::cmp::Ordering::Greater }
        else { std::cmp::Ordering::Equal }
    });

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
                    // 円筒形 (Storage/Database)
                    <path stroke-linecap="round" stroke-linejoin="round" d="M20.25 6.375c0 2.278-3.694 4.125-8.25 4.125S3.75 8.653 3.75 6.375m16.5 0c0-2.278-3.694-4.125-8.25-4.125S3.75 4.097 3.75 6.375m16.5 0v11.25c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125V6.375m16.5 0v3.75m-16.5-3.75v3.75" />
                    // 下向き矢印 (位置を下に調整、縦棒を長く)
                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 18.75l2.25-2.25M12 18.75l-2.25-2.25M12 18.75V9" />
                </svg>
            </button>

            // Category Selector
            <div class="relative inline-block text-left">
                <button
                    onclick={on_category_click}
                    class={classes!(
                        "ml-2", "px-3", "py-1", "rounded", "bg-gray-700", "text-gray-200", "text-xs", "font-bold", "flex", "items-center", "space-x-1", "transition-colors",
                        if has_multiple_cats || props.current_category.is_empty() || props.current_category == "__LOCAL__" { "hover:bg-gray-600 cursor-pointer" } else { "cursor-default" }
                    )}
                >
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3 h-3">
                        <path d="M3.75 3A1.75 1.75 0 002 4.75v10.5c0 .966.784 1.75 1.75 1.75h12.5A1.75 1.75 0 0018 15.25V4.75A1.75 1.75 0 0016.25 3H3.75zM10 6.5a.75.75 0 01.75.75v2.5h2.5a.75.75 0 010 1.5h-2.5v2.5a.75.75 0 01-1.5 0v-2.5h-2.5a.75.75 0 010-1.5h2.5v-2.5A.75.75 0 0110 6.5z" />
                    </svg>
                    <span>{ current_cat_name }</span>
                    if has_multiple_cats || props.current_category.is_empty() || props.current_category == "__LOCAL__" {
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3 h-3 opacity-50">
                            <path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
                        </svg>
                    }
                </button>

                if props.is_dropdown_open && (has_multiple_cats || props.current_category.is_empty() || props.current_category == "__LOCAL__") {
                    <>
                        <div 
                            onclick={let on_t = props.on_toggle_dropdown.clone(); move |_| on_t.emit(false)}
                            class="fixed inset-0 z-[140]"
                        ></div>
                        <div class="absolute left-2 mt-1 w-48 rounded-md shadow-2xl bg-gray-800 border border-gray-700 z-[150] overflow-hidden animate-in fade-in zoom-in duration-100">
                            <div class="py-1 max-h-60 overflow-y-auto">
                                if props.current_category == "__LOCAL__" {
                                    <button
                                        class="w-full text-left px-4 py-2 text-xs bg-emerald-600 text-white font-bold cursor-default"
                                    >
                                        { i18n::t("local_file", lang) }
                                    </button>
                                } else {
                                    <button
                                        onclick={let on_change = props.on_change_category.clone(); let on_toggle = props.on_toggle_dropdown.clone(); move |_| { on_change.emit("__LOCAL__".to_string()); on_toggle.emit(false); }}
                                        class="w-full text-left px-4 py-2 text-xs text-gray-300 hover:bg-gray-700 hover:text-white transition-colors"
                                    >
                                        { i18n::t("local_file", lang) }
                                    </button>
                                }
                                { for sorted_categories.into_iter().map(|cat| {
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
                                                if is_active { "bg-emerald-600 text-white font-bold cursor-default" } else { "text-gray-300 hover:bg-gray-700 hover:text-white" }
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
            <div>
                <select
                    value={props.file_extension.clone()}
                    onchange={
                        let on_change = props.on_change_extension.clone();
                        Callback::from(move |e: Event| {
                            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
                            on_change.emit(select.value());
                        })
                    }
                    class="bg-gray-700 text-gray-200 text-xs font-bold py-1 px-3 rounded border border-gray-600 outline-none hover:bg-gray-600 focus:border-emerald-500 transition-colors cursor-pointer text-center"
                >
                    { for extensions.iter().map(|(ext, key)| {
                        let is_mobile = gloo::utils::document().body()
                            .map(|b| b.class_list().contains("leaf-mobile-mode"))
                            .unwrap_or(false);
                        html! {
                            <option value={*ext} selected={*ext == props.file_extension}>
                                { if is_mobile { format!(".{}", ext) } else { format!("{}: .{}", i18n::t(key, lang), ext) } }
                            </option>
                        }
                    }) }
                </select>
            </div>
            <div class="mobile:hidden flex items-center space-x-1 ml-2 mr-4 border-l border-gray-700 pl-4">
                <button
                    onclick={props.on_change_font_size.reform(|_| -1)}
                    class="p-1 w-8 h-8 rounded hover:bg-gray-700 text-gray-400 hover:text-white flex items-center justify-center font-bold"
                    title={i18n::t("decrease_font_size", lang)}
                >
                    {"-"}
                </button>
                <button
                    onclick={props.on_change_font_size.reform(|_| 1)}
                    class="p-1 w-8 h-8 rounded hover:bg-gray-700 text-gray-400 hover:text-white flex items-center justify-center font-bold"
                    title={i18n::t("increase_font_size", lang)}
                >
                    {"+"}
                </button>
            </div>
            <div class="flex-1"></div>
            <button
                onclick={props.on_help.reform(|_| ())}
                class="mobile:hidden p-1.5 rounded hover:bg-gray-700 text-gray-400 hover:text-white mr-2"
                title={i18n::t("help", lang)}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9 5.25h.008v.008H12v-.008z" />
                </svg>
            </button>
            <button
                onclick={props.on_preview.reform(|_| ())}
                class="mobile:hidden p-1.5 rounded hover:bg-gray-700 text-gray-400 hover:text-white mr-2"
                title={i18n::t("preview", lang)}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                    // 書類
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                    // 目
                    <path stroke-linecap="round" stroke-linejoin="round" d="M15 14.5c0 .828-1.343 1.5-3 1.5s-3-.672-3-1.5S10.343 13 12 13s3 .672 3 1.5z" />
                    <circle cx="12" cy="14.5" r="0.5" fill="currentColor" />
                </svg>
            </button>
            <button
                onclick={props.on_logout.reform(|_| ())}
                class="mobile:hidden p-1.5 rounded hover:bg-gray-700 text-gray-400 hover:text-white mr-2"
                title={i18n::t("logout", lang)}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 9V5.25A2.25 2.25 0 0013.5 3h-6a2.25 2.25 0 00-2.25 2.25v13.5A2.25 2.25 0 007.5 21h6a2.25 2.25 0 002.25-2.25V15m3 0l3-3m0 0l-3-3m3 3H9" />
                </svg>
            </button>
            <span 
                class="mobile:hidden text-green-500 opacity-60 font-bold px-4 text-xl select-none"
                style="font-family: 'Petit Formal Script', cursive;"
            >
                {"Leaf"}
            </span>

            // Sheet list button (Portrait only, shown when multiple sheets)
            if props.sheet_count > 1 {
                if let Some(ref on_open_sl) = props.on_open_sheet_list {
                    <button
                        onclick={let cb = on_open_sl.clone(); move |_| cb.emit(())}
                        class="desktop:hidden p-1.5 rounded hover:bg-gray-700 text-gray-400 hover:text-white transition-colors"
                        title={i18n::t("open_sheets", lang)}
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M6 6.878V6a2.25 2.25 0 012.25-2.25h7.5A2.25 2.25 0 0118 6v.878m-12 0c.235-.083.487-.128.75-.128h10.5c.263 0 .515.045.75.128m-12 0A2.25 2.25 0 003.75 9v.878m0 0c.235-.083.487-.128.75-.128h10.5c.263 0 .515.045.75.128M3.75 9.878A2.25 2.25 0 001.5 12v6a2.25 2.25 0 002.25 2.25h16.5A2.25 2.25 0 0022.5 18v-6a2.25 2.25 0 00-2.25-2.122" />
                        </svg>
                    </button>
                }
            }
            // Hamburger menu (Portrait only)
            <div class="relative inline-block text-left desktop:hidden mr-1">
                <button
                    onclick={let is_open = is_hamburger_open.clone(); move |_| is_open.set(!*is_open)}
                    class={classes!(
                        "p-1.5", "rounded", "hover:bg-gray-700", "text-gray-400", "hover:text-white", "transition-colors",
                        if *is_hamburger_open { "bg-gray-700 text-white" } else { "" }
                    )}
                >
                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5" />
                    </svg>
                </button>
                if *is_hamburger_open {
                    <div 
                        onclick={let is_open = is_hamburger_open.clone(); move |_| is_open.set(false)}
                        class="fixed inset-0 z-[140]"
                    ></div>
                    <div class="absolute right-0 mt-2 w-48 rounded-md shadow-2xl bg-gray-800 border border-gray-700 z-[150] overflow-hidden animate-in fade-in zoom-in duration-100 origin-top-right">
                        <button
                            onclick={let is_open = is_hamburger_open.clone(); let on_help = props.on_help.clone(); move |_| { is_open.set(false); on_help.emit(()); }}
                            class="w-full text-left px-4 py-3 text-sm text-gray-300 hover:bg-gray-700 hover:text-white transition-colors flex items-center space-x-3"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5 opacity-70">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9 5.25h.008v.008H12v-.008z" />
                            </svg>
                            <span>{ i18n::t("help", lang) }</span>
                        </button>
                        <button
                            onclick={let is_open = is_hamburger_open.clone(); let on_preview = props.on_preview.clone(); move |_| { is_open.set(false); on_preview.emit(()); }}
                            class="w-full text-left px-4 py-3 text-sm text-gray-300 hover:bg-gray-700 hover:text-white transition-colors flex items-center space-x-3 border-t border-gray-700/50"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5 opacity-70">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                                <path stroke-linecap="round" stroke-linejoin="round" d="M15 14.5c0 .828-1.343 1.5-3 1.5s-3-.672-3-1.5S10.343 13 12 13s3 .672 3 1.5z" />
                                <circle cx="12" cy="14.5" r="0.5" fill="currentColor" />
                            </svg>
                            <span>{ i18n::t("preview", lang) }</span>
                        </button>
                        <button
                            onclick={let is_open = is_hamburger_open.clone(); let on_settings = props.on_open_settings.clone(); move |_| { is_open.set(false); on_settings.emit(()); }}
                            class="w-full text-left px-4 py-3 text-sm text-gray-300 hover:bg-gray-700 hover:text-white transition-colors flex items-center space-x-3 border-t border-gray-700/50"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5 opacity-70">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 010 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 010-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28z" />
                                <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                            </svg>
                            <span>{ i18n::t("settings", lang) }</span>
                        </button>
                        <button
                            onclick={let is_open = is_hamburger_open.clone(); let on_logout = props.on_logout.clone(); move |_| { is_open.set(false); on_logout.emit(()); }}
                            class="w-full text-left px-4 py-3 text-sm text-red-400 hover:bg-gray-700 hover:text-red-300 transition-colors flex items-center space-x-3 border-t border-gray-700/50"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5 opacity-70">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 9V5.25A2.25 2.25 0 0013.5 3h-6a2.25 2.25 0 00-2.25 2.25v13.5A2.25 2.25 0 007.5 21h6a2.25 2.25 0 002.25-2.25V15m3 0l3-3m0 0l-3-3m3 3H9" />
                            </svg>
                            <span>{ i18n::t("logout", lang) }</span>
                        </button>
                    </div>
                }
            </div>
        </div>
    }
}
