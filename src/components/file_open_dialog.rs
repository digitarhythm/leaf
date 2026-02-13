use yew::prelude::*;
use crate::drive_interop::{list_files, download_file};
use crate::db_interop::JSCategory;
use crate::i18n::{self, Language};
use crate::components::dialog::InputDialog;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use web_sys::{KeyboardEvent, AbortController};

#[derive(Clone, PartialEq)]
pub struct FilePreview {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Properties, PartialEq)]
pub struct FileOpenDialogProps {
    pub on_close: Callback<()>,
    pub on_select: Callback<(String, String, String)>, // (drive_id, title, category_id)
    pub leaf_data_id: String,
    pub categories: Vec<JSCategory>,
    pub on_refresh: Callback<()>,
    pub on_delete_category: Callback<String>,
    #[prop_or_default]
    pub on_start_processing: Callback<()>,
}

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
    let is_creating_category = use_state(|| false);
    let is_fading_out = use_state(|| false);
    let current_category_id = use_state(|| "".to_string());
    let current_category_name = use_state(|| "".to_string());
    let abort_controller = use_state(|| None::<AbortController>);
    let root_ref = use_node_ref();

    // カテゴリ選択時のファイル一覧取得
    let load_files = {
        let files_state = files.clone();
        let selected_file_idx = selected_file_idx.clone();
        let is_loading = is_loading_files.clone();
        let current_category_id = current_category_id.clone();
        let current_category_name = current_category_name.clone();
        let focused_area = focused_area.clone();
        let is_fading_out = is_fading_out.clone();
        let abort_ctrl_state = abort_controller.clone();
        
        Callback::from(move |(cat_id, cat_name, is_initial): (String, String, bool)| {
            let files_state = files_state.clone();
            let selected_file_idx = selected_file_idx.clone();
            let is_loading = is_loading.clone();
            let current_category_id = current_category_id.clone();
            let current_category_name = current_category_name.clone();
            let focused_area = focused_area.clone();
            let is_fading_out = is_fading_out.clone();
            let abort_ctrl_state = abort_ctrl_state.clone();
            
            // 以前のリクエストをキャンセル
            if let Some(ctrl) = (*abort_ctrl_state).as_ref() {
                ctrl.abort();
            }
            
            // 新しいコントローラーを作成
            let new_ctrl = AbortController::new().unwrap();
            let signal = new_ctrl.signal();
            abort_ctrl_state.set(Some(new_ctrl.clone()));

            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    let _ = storage.set_item(STORAGE_KEY_LAST_CAT, &cat_id);
                }
            }

            is_loading.set(true);
            current_category_id.set(cat_id.clone());
            current_category_name.set(cat_name);
            
            let signal_for_list = signal.clone();
            spawn_local(async move {
                let res = list_files(&cat_id, Some(signal_for_list.clone())).await;
                
                // シグナルが中断されていたら、新しいリクエストが開始されているので、
                // この古いリクエストではインジケータを触らない。
                if signal_for_list.aborted() {
                    return;
                }

                if let Ok(res_val) = res {
                    if let Ok(files_val) = js_sys::Reflect::get(&res_val, &JsValue::from_str("files")) {
                        let array = js_sys::Array::from(&files_val);
                        let mut download_futures = Vec::new();
                        for i in 0..array.length() {
                            if download_futures.len() >= 10 { break; }
                            let v = array.get(i);
                            let id = js_sys::Reflect::get(&v, &JsValue::from_str("id")).unwrap().as_string().unwrap();
                            let name = js_sys::Reflect::get(&v, &JsValue::from_str("name")).unwrap().as_string().unwrap();
                            let id_clone = id.clone();
                            let signal_inner = signal_for_list.clone();
                            download_futures.push(async move {
                                let content = if let Ok(c_val) = download_file(&id_clone, Some("0-1024"), Some(signal_inner)).await {
                                    c_val.as_string().unwrap_or_default()
                                } else { "".to_string() };
                                FilePreview { id, name, content }
                            });
                        }
                        let previews = futures::future::join_all(download_futures).await;
                        
                        if signal_for_list.aborted() {
                            return;
                        }

                        let has_files = !previews.is_empty();
                        files_state.set(previews);
                        selected_file_idx.set(0);

                        if is_initial && !*is_fading_out {
                            if has_files { focused_area.set(FocusedArea::Files); }
                            else { focused_area.set(FocusedArea::Categories); }
                        }
                    }
                }
                is_loading.set(false);
            });
        })
    };

    {
        let props_cats = props.categories.clone();
        let load_files = load_files.clone();
        let selected_cat_idx = selected_cat_idx.clone();
        use_effect_with(props_cats.clone(), move |cats: &Vec<JSCategory>| {
            if !cats.is_empty() {
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

    let on_ok_click = {
        let on_select = props.on_select.clone();
        let files = files.clone();
        let selected_file_idx = selected_file_idx.clone();
        let current_cat_id = current_category_id.clone();
        let is_loading_files = is_loading_files.clone();
        let is_fading_out = is_fading_out.clone();
        let on_start = props.on_start_processing.clone();
        Callback::from(move |_: ()| {
            if !*is_loading_files && !files.is_empty() && !*is_fading_out {
                let file = &files[*selected_file_idx];
                let drive_id = file.id.clone();
                let title = file.name.clone();
                let cat_id = (*current_cat_id).clone();
                let on_select = on_select.clone();
                let on_start = on_start.clone();
                is_fading_out.set(true);
                on_start.emit(());
                gloo::timers::callback::Timeout::new(200, move || {
                    on_select.emit((drive_id, title, cat_id));
                }).forget();
            }
        })
    };

    let on_keydown = {
        let focused_area = focused_area.clone();
        let selected_cat_idx = selected_cat_idx.clone();
        let selected_file_idx = selected_file_idx.clone();
        let categories = props.categories.clone();
        let files = files.clone();
        let load_files = load_files.clone();
        let on_ok = on_ok_click.clone();
        let is_fading_out = is_fading_out.clone();
        let loading_handle = is_loading_files.clone();

        Callback::from(move |e: KeyboardEvent| {
            let current_focus = *focused_area;
            if *is_fading_out { return; }
            
            match e.key().as_str() {
                "Tab" => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Categories {
                        if !*loading_handle {
                            focused_area.set(FocusedArea::Files);
                        }
                    } else {
                        focused_area.set(FocusedArea::Categories);
                    }
                }
                "ArrowUp" => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Categories {
                        if *selected_cat_idx > 0 {
                            let new_idx = *selected_cat_idx - 1;
                            selected_cat_idx.set(new_idx);
                            load_files.emit((categories[new_idx].id.clone(), categories[new_idx].name.clone(), false));
                        }
                    } else {
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
                    } else {
                        if *selected_file_idx + 1 < files.len() { selected_file_idx.set(*selected_file_idx + 1); }
                    }
                }
                "Enter" => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Categories {
                        if !*loading_handle {
                            focused_area.set(FocusedArea::Files);
                        }
                    } else {
                        on_ok.emit(());
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
                <div class="px-6 py-3 border-b border-gray-700 bg-gray-900 flex justify-between items-center">
                    <h3 class="text-lg font-bold text-white">{ i18n::t("file_selection", lang) }</h3>
                </div>

                <div class="px-4 py-2 border-b border-gray-700 bg-gray-800/50 flex justify-end space-x-2">
                    <button 
                        onclick={let is_creating = is_creating_category.clone(); move |_| is_creating.set(true)}
                        class="p-2 rounded-[6px] bg-gray-700 hover:bg-gray-600 shadow-md transition-all text-white flex items-center space-x-2"
                        title={ i18n::t("new_category", lang) }
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m3.75 9v6m3-3H9m1.5-12H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                        </svg>
                        <span class="text-xs font-bold px-1">{ i18n::t("new_category", lang) }</span>
                    </button>
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

                <div class="flex-1 flex overflow-hidden">
                    <div class="w-[30%] border-r border-gray-700 flex flex-col overflow-y-auto p-2 space-y-1 bg-gray-900/30">
                        { for props.categories.iter().enumerate().map(|(idx, cat)| {
                            let is_selected = *selected_cat_idx == idx;
                            let area_active = *focused_area == FocusedArea::Categories && *is_root_focused;
                            let is_focused = is_selected && area_active;
                            
                            let id_for_change = cat.id.clone();
                            let id_for_delete = cat.id.clone();
                            let name = cat.name.clone();
                            let load_files = load_files.clone();
                            let selected_cat_idx = selected_cat_idx.clone();
                            let on_delete = props.on_delete_category.clone();
                            let is_no_cat = cat.name == "NO_CATEGORY";

                            html! {
                                <div class={classes!(
                                    "w-full", "rounded-[6px]", "transition-all", "flex", "items-center", "group", "border-[3px]",
                                    if is_focused { vec!["border-lime-400", "ring-1", "ring-lime-400"] } else { vec!["border-transparent"] },
                                    if is_focused { vec!["bg-blue-600", "text-white"] }
                                    else if is_selected { vec!["bg-slate-600", "text-gray-200"] }
                                    else { vec!["bg-gray-700/50", "text-gray-400", "hover:bg-gray-700"] }
                                )}
                                style="height: 6.2%; min-height: 32px; margin-bottom: 0.4%;"
                                >
                                    <button 
                                        onclick={move |_| { selected_cat_idx.set(idx); load_files.emit((id_for_change.clone(), name.clone(), false)); }}
                                        class="flex-1 text-left px-4 truncate h-full flex items-center outline-none"
                                    >
                                        <span class="truncate">{ &cat.name }</span>
                                    </button>
                                    if !is_no_cat {
                                        <button
                                            onclick={move |e: MouseEvent| { e.stop_propagation(); on_delete.emit(id_for_delete.clone()); }}
                                            class="p-2 text-gray-500 hover:text-red-400 opacity-0 group-hover:opacity-100 transition-opacity outline-none"
                                            title={i18n::t("delete", lang)}
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5">
                                                <path fill-rule="evenodd" d="M8.75 1A2.75 2.75 0 006 3.75v.443c-.795.077-1.584.176-2.365.298a.75.75 0 10.244 1.487l.263-.041.608 11.137A2.75 2.75 0 007.5 19h5a2.75 2.75 0 002.747-2.597l.608-11.137.263.041a.75.75 0 10.244-1.487A48.112 48.112 0 0014 4.193V3.75A2.75 2.75 0 0011.25 1h-2.5zM10 4c.84 0 1.673.025 2.5.075V3.75c0-.69-.56-1.25-1.25-1.25h-2.5c-.69 0-1.25.56-1.25 1.25v.325C8.327 4.025 9.16 4 10 4zM8.58 7.72a.75.75 0 00-1.5.06l.3 7.5a.75.75 0 101.498-.06l-.3-7.5zm4.34.06a.75.75 0 10-1.498-.06l-.3 7.5a.75.75 0 001.5.06l.3-7.5z" clip-rule="evenodd" />
                                            </svg>
                                        </button>
                                    }
                                </div>
                            }
                        }) }
                    </div>

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
                            
                            let s_idx_1 = selected_file_idx.clone();
                            let s_idx_2 = selected_file_idx.clone();
                            let is_loading_files = is_loading_files.clone();
                            let on_ok = on_ok_click.clone();

                            html! {
                                <button 
                                    onclick={move |_| if !*is_loading_files { s_idx_1.set(idx) }}
                                    ondblclick={let on_ok = on_ok.clone(); move |_| { s_idx_2.set(idx); on_ok.emit(()); }}
                                    class={classes!(
                                        "w-full", "text-left", "p-4", "rounded-[6px]", "shadow-md", "transition-all", "overflow-hidden", "flex", "flex-col", "border-[3px]",
                                        if is_focused { vec!["border-lime-400", "ring-1", "ring-lime-400"] } else { vec!["border-transparent"] },
                                        if is_focused { vec!["bg-blue-600", "text-white"] }
                                        else if is_selected { vec!["bg-slate-600", "text-gray-200"] }
                                        else { vec!["bg-gray-700/50", "text-gray-400", "hover:bg-gray-700"] }
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

                <div class="bg-gray-900 border-t border-gray-700 px-6 py-2 flex items-center justify-between">
                    <div class="text-[10px] text-gray-500 font-medium">
                        { i18n::t("guide_keys", lang) }
                    </div>
                    <div class="flex space-x-3">
                        <button 
                            onclick={on_ok_click.reform(|_| ())}
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
            if *is_creating_category {
                <InputDialog 
                    title={i18n::t("new_category", lang)} 
                    message={i18n::t("enter_category_name_message", lang)} 
                    on_confirm={
                        let is_creating = is_creating_category.clone();
                        let ldid = props.leaf_data_id.clone();
                        let on_refresh = props.on_refresh.clone();
                        Callback::from(move |name: String| {
                            let is_creating = is_creating.clone();
                            let ldid = ldid.clone();
                            let on_refresh = on_refresh.clone();
                            if !name.trim().is_empty() {
                                spawn_local(async move {
                                    if let Ok(_) = crate::drive_interop::create_folder(&name, &ldid).await {
                                        on_refresh.emit(());
                                    }
                                    is_creating.set(false);
                                });
                            } else {
                                is_creating.set(false);
                            }
                        })
                    }
                    on_cancel={let is_creating = is_creating_category.clone(); Callback::from(move |_| is_creating.set(false))}
                />
            }
        </div>
    }
}
