use yew::prelude::*;
use crate::drive_interop::{list_files, download_file};
use crate::db_interop::JSCategory;
use crate::i18n::{self, Language};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use web_sys::KeyboardEvent;

#[derive(Clone, PartialEq)]
pub struct FilePreview {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Properties, PartialEq)]
pub struct FileOpenDialogProps {
    pub on_close: Callback<()>,
    pub on_select: Callback<(String, String, String)>, // (drive_id, title, category)
    pub leaf_data_id: String,
    pub categories: Vec<JSCategory>,
    pub on_refresh: Callback<()>,
    #[prop_or_default]
    pub on_start_processing: Callback<()>,
}

// ... (omitted - I will use exact literal in actual call)

#[derive(PartialEq, Clone, Copy)]
enum FocusedArea {
    Categories,
    Files,
}

const STORAGE_KEY_LAST_CAT: &str = "leaf_last_category";

#[function_component(FileOpenDialog)]
pub fn file_open_dialog(props: &FileOpenDialogProps) -> Html {
    let lang = Language::detect();

    let focused_area = use_state(|| FocusedArea::Categories);
    let is_root_focused = use_state(|| false); 
    let selected_cat_idx = use_state(|| 0usize);
    let selected_file_idx = use_state(|| 0usize);
    let files = use_state(|| Vec::<FilePreview>::new());
    let is_loading_files = use_state(|| false);
    let is_fading_out = use_state(|| false);
    let current_category_name = use_state(|| "".to_string());
    let root_ref = use_node_ref();

    // カテゴリ選択時のファイル一覧取得
    let load_files = {
        let files_state = files.clone();
        let selected_file_idx = selected_file_idx.clone();
        let is_loading = is_loading_files.clone();
        let current_category_name = current_category_name.clone();
        let focused_area = focused_area.clone();
        let is_fading_out = is_fading_out.clone();
        
        Callback::from(move |(cat_id, cat_name, is_initial): (String, String, bool)| {
            let files_state = files_state.clone();
            let selected_file_idx = selected_file_idx.clone();
            let is_loading = is_loading.clone();
            let current_category_name = current_category_name.clone();
            let focused_area = focused_area.clone();
            let is_fading_out = is_fading_out.clone();
            
            // 最後に選択されたカテゴリIDを保存
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    let _ = storage.set_item(STORAGE_KEY_LAST_CAT, &cat_id);
                }
            }

            is_loading.set(true);
            current_category_name.set(cat_name);
            spawn_local(async move {
                if let Ok(res) = list_files(&cat_id).await {
                    if let Ok(files_val) = js_sys::Reflect::get(&res, &JsValue::from_str("files")) {
                        let array = js_sys::Array::from(&files_val);
                        let mut download_futures = Vec::new();
                        for i in 0..array.length() {
                            if download_futures.len() >= 10 { break; }
                            let v = array.get(i);
                            let id = js_sys::Reflect::get(&v, &JsValue::from_str("id")).unwrap().as_string().unwrap();
                            let name = js_sys::Reflect::get(&v, &JsValue::from_str("name")).unwrap().as_string().unwrap();
                            let id_clone = id.clone();
                            download_futures.push(async move {
                                let content = if let Ok(c_val) = download_file(&id_clone, Some("0-1024")).await {
                                    c_val.as_string().unwrap_or_default()
                                } else { "".to_string() };
                                FilePreview { id, name, content }
                            });
                        }
                        let previews = futures::future::join_all(download_futures).await;
                        
                        let has_files = !previews.is_empty();
                        files_state.set(previews);
                        selected_file_idx.set(0);

                        // 初期表示時のみのフォーカス制御
                        if is_initial && !*is_fading_out {
                            if has_files {
                                focused_area.set(FocusedArea::Files);
                            } else {
                                focused_area.set(FocusedArea::Categories);
                            }
                        }
                    }
                }
                is_loading.set(false);
            });
        })
    };

    // 初期表示時に適切なカテゴリを読み込む
    {
        let props_cats = props.categories.clone();
        let load_files = load_files.clone();
        let selected_cat_idx = selected_cat_idx.clone();
        use_effect_with(props_cats.clone(), move |cats: &Vec<JSCategory>| {
            if !cats.is_empty() {
                // 1. localStorage から取得
                // 2. なければ NO_CATEGORY を探す
                // 3. なければ 0 番目
                let last_cat_id = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .and_then(|s| s.get_item(STORAGE_KEY_LAST_CAT).ok().flatten());

                let target_idx = if let Some(id) = last_cat_id {
                    cats.iter().position(|c| c.id == id).unwrap_or(0)
                } else {
                    cats.iter().position(|c| c.name == "NO_CATEGORY").unwrap_or(0)
                };

                selected_cat_idx.set(target_idx);
                load_files.emit((cats[target_idx].id.clone(), cats[target_idx].name.clone(), true));
            }
            || ()
        });
    }

    // フォーカス管理
    {
        let root_ref = root_ref.clone();
        use_effect_with((), move |_| {
            let root = root_ref.clone();
            gloo::timers::callback::Timeout::new(10, move || {
                if let Some(div) = root.cast::<web_sys::HtmlElement>() { 
                    let _ = div.focus(); 
                }
            }).forget();
            || ()
        });
    }

    // キーボードイベント
    let on_keydown = {
        let focused_area = focused_area.clone();
        let selected_cat_idx = selected_cat_idx.clone();
        let selected_file_idx = selected_file_idx.clone();
        let categories = props.categories.clone();
        let files = files.clone();
        let load_files = load_files.clone();
        let on_close = props.on_close.clone();
        let on_select = props.on_select.clone();
        let on_start = props.on_start_processing.clone();
        let current_cat_name = current_category_name.clone();
        let is_loading_files = is_loading_files.clone();
        let is_fading_out = is_fading_out.clone();

        Callback::from(move |e: KeyboardEvent| {
            let current_focus = *focused_area;
            let loading = *is_loading_files;
            if *is_fading_out { return; }
            
            match e.key().as_str() {
                // ... (skipping some lines for brevity in thought, but I will provide exact below)
                "Tab" => {
                    e.prevent_default();
                    focused_area.set(if current_focus == FocusedArea::Categories { FocusedArea::Files } else { FocusedArea::Categories });
                }
                "ArrowUp" => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Categories {
                        if *selected_cat_idx > 0 {
                            let new_idx = *selected_cat_idx - 1;
                            selected_cat_idx.set(new_idx);
                            load_files.emit((categories[new_idx].id.clone(), categories[new_idx].name.clone(), false));
                        }
                    } else if !loading {
                        if *selected_file_idx > 0 { selected_file_idx.set(*selected_file_idx - 1); }
                    }
                }
                "ArrowDown" => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Categories {
                        if *selected_cat_idx + 1 < categories.len() {
                            let new_idx = *selected_cat_idx + 1;
                            selected_cat_idx.set(new_idx);
                            load_files.emit((categories[new_idx].id.clone(), categories[new_idx].name.clone(), false));
                        }
                    } else if !loading {
                        if *selected_file_idx + 1 < files.len() { selected_file_idx.set(*selected_file_idx + 1); }
                    }
                }
                "Escape" => { on_close.emit(()); }
                "Enter" => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Categories {
                        if !categories.is_empty() {
                            let idx = *selected_cat_idx;
                            load_files.emit((categories[idx].id.clone(), categories[idx].name.clone(), false));
                            focused_area.set(FocusedArea::Files);
                        }
                    } else if !loading {
                        if !files.is_empty() {
                            let file = &files[*selected_file_idx];
                            let drive_id = file.id.clone();
                            let title = file.name.clone();
                            let cat = (*current_cat_name).clone();
                            let on_select = on_select.clone();
                            let on_start = on_start.clone();
                            is_fading_out.set(true);
                            on_start.emit(());
                            gloo::timers::callback::Timeout::new(200, move || {
                                on_select.emit((drive_id, title, cat));
                            }).forget();
                        }
                    }
                }
                _ => {}
            }
        })
    };

    let on_focus_in = {
        let is_root_focused = is_root_focused.clone();
        Callback::from(move |_| is_root_focused.set(true))
    };
    let on_focus_out = {
        let is_root_focused = is_root_focused.clone();
        Callback::from(move |_| is_root_focused.set(false))
    };

    let on_ok_click = {
        let on_select = props.on_select.clone();
        let on_start = props.on_start_processing.clone();
        let files = files.clone();
        let selected_file_idx = selected_file_idx.clone();
        let current_cat_name = current_category_name.clone();
        let is_loading_files = is_loading_files.clone();
        let is_fading_out = is_fading_out.clone();
        Callback::from(move |_| {
            if !*is_loading_files && !files.is_empty() && !*is_fading_out {
                let file = &files[*selected_file_idx];
                let drive_id = file.id.clone();
                let title = file.name.clone();
                let cat = (*current_cat_name).clone();
                let on_select = on_select.clone();
                let on_start = on_start.clone();
                is_fading_out.set(true);
                on_start.emit(());
                gloo::timers::callback::Timeout::new(200, move || {
                    on_select.emit((drive_id, title, cat));
                }).forget();
            }
        })
    };

    html! {
        <div 
            ref={root_ref} tabindex="0" onkeydown={on_keydown}
            onfocusin={on_focus_in} onfocusout={on_focus_out}
            class={classes!(
                "fixed", "inset-0", "z-[100]", "flex", "items-center", "justify-center", "bg-black/60", "backdrop-blur-sm", "p-4", "outline-none",
                if *is_fading_out { "opacity-0 transition-opacity duration-200" } else { "" }
            )}
        >
            <div 
                class={classes!(
                    "bg-gray-800", "border", "border-gray-700", "rounded-lg", "shadow-2xl", "overflow-hidden", "flex", "flex-col", "relative",
                    if *is_fading_out { "animate-dialog-out" } else { "animate-dialog-in" }
                )}
                style="width: 80vw; height: 80vh;"
            >
                // Title Bar
                <div class="px-6 py-3 border-b border-gray-700 bg-gray-900 flex justify-between items-center">
                    <h3 class="text-lg font-bold text-white">{ i18n::t("file_selection", lang) }</h3>
                </div>

                // Top Button Bar
                <div class="px-4 py-2 border-b border-gray-700 bg-gray-800/50 flex justify-end">
                    <button 
                        onclick={let cb = props.on_refresh.clone(); move |_| cb.emit(())}
                        class="p-2 rounded-[6px] bg-gray-700 hover:bg-gray-600 shadow-md transition-all text-white"
                        title={ i18n::t("refresh_categories", lang) }
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99" />
                        </svg>
                    </button>
                </div>

                // Main Content (Split Pane)
                <div class="flex-1 flex overflow-hidden">
                    // Left: Categories (30%)
                    <div class="w-[30%] border-r border-gray-700 flex flex-col overflow-y-auto p-2 space-y-1 bg-gray-900/30">
                        { for props.categories.iter().enumerate().map(|(idx, cat)| {
                            let is_selected = *selected_cat_idx == idx;
                            let area_active = *focused_area == FocusedArea::Categories && *is_root_focused;
                            let is_focused = is_selected && area_active;
                            let show_selection = is_selected && area_active;
                            let id = cat.id.clone();
                            let name = cat.name.clone();
                            let load_files = load_files.clone();
                            let selected_cat_idx = selected_cat_idx.clone();
                            html! {
                                <button 
                                    onclick={move |_| { selected_cat_idx.set(idx); load_files.emit((id.clone(), name.clone(), false)); }}
                                    class={classes!(
                                        "w-full", "text-left", "px-4", "rounded-[6px]", "shadow-md", "transition-all", "flex", "items-center", "border-2",
                                        if is_focused { vec!["border-lime-400", "ring-1", "ring-lime-400"] } else { vec!["border-transparent"] },
                                        if show_selection { vec!["bg-blue-600", "text-white"] } else { vec!["bg-gray-700/50", "text-gray-400", "hover:bg-gray-700"] }
                                    )}
                                    style="height: 6.2%; min-height: 32px; margin-bottom: 0.4%;"
                                >
                                    <span class="truncate">{ &cat.name }</span>
                                </button>
                            }
                        }) }
                    </div>

                    // Right: Files (70%)
                    <div class="w-[70%] flex flex-col overflow-y-auto p-4 space-y-2 relative">
                        if *is_loading_files {
                            <div class="absolute inset-0 flex items-center justify-center bg-gray-800/30 z-10">
                                <div class="w-10 h-10 border-4 border-lime-500 border-t-transparent rounded-full animate-spin"></div>
                            </div>
                        }
                        { for files.iter().enumerate().map(|(idx, file)| {
                            let is_selected = *selected_file_idx == idx;
                            let area_active = *focused_area == FocusedArea::Files && *is_root_focused;
                            let is_focused = is_selected && area_active;
                            let show_selection = is_selected && area_active;
                            let selected_file_idx = selected_file_idx.clone();
                            let is_loading_files = is_loading_files.clone();
                            html! {
                                <button 
                                    onclick={move |_| if !*is_loading_files { selected_file_idx.set(idx) }}
                                    class={classes!(
                                        "w-full", "text-left", "p-4", "rounded-[6px]", "shadow-md", "transition-all", "overflow-hidden", "flex", "flex-col", "border-2",
                                        if is_focused { vec!["border-lime-400", "ring-1", "ring-lime-400"] } else { vec!["border-transparent"] },
                                        if show_selection { vec!["bg-blue-600", "text-white"] } else { vec!["bg-gray-700/50", "text-gray-400", "hover:bg-gray-700"] }
                                    )}
                                    style="height: 19%; min-height: 80px; margin-bottom: 1%;"
                                >
                                    <div class="font-bold text-xs opacity-50 mb-1">{ &file.name }</div>
                                    <div class="text-sm line-clamp-2 whitespace-pre-wrap font-mono opacity-80">
                                        { &file.content }
                                    </div>
                                </button>
                            }
                        }) }
                    </div>
                </div>

                // Bottom Button Bar & Operation Guide
                <div class="bg-gray-900 border-t border-gray-700 px-6 py-2 flex items-center justify-between">
                    <div class="text-[10px] text-gray-500 font-medium">
                        { i18n::t("guide_keys", lang) }
                    </div>
                    <div class="flex space-x-3">
                        <button 
                            onclick={on_ok_click}
                            class="px-8 py-2 bg-lime-600 hover:bg-lime-700 text-white font-bold rounded-[6px] shadow-lg transition-all"
                        >
                            {"OK"}
                        </button>
                        <button 
                            onclick={props.on_close.reform(|_| ())}
                            class="px-6 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded-[6px] shadow-lg transition-all"
                        >
                            { i18n::t("cancel", lang) }
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}
