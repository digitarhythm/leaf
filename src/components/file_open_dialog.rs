use yew::prelude::*;
use crate::drive_interop::{list_files, download_file, move_file};
use crate::db_interop::JSCategory;
use crate::i18n::{self, Language};
use crate::components::dialog::{InputDialog, ConfirmDialog};
use crate::components::preview::Preview;
use wasm_bindgen::{JsValue, JsCast};
use wasm_bindgen_futures::spawn_local;
use web_sys::{KeyboardEvent, AbortController};
use gloo::timers::callback::Timeout;
use gloo::events::{EventListener, EventListenerOptions};

#[derive(Clone, PartialEq)]
pub struct FilePreview {
    pub id: String,
    pub name: String,
    pub content: String,
    pub total_size: u64,
    pub loaded_bytes: u64,
    pub is_markdown: bool,
    pub lang: String,
}

fn get_highlight_lang(filename: &str) -> Option<&str> {
    let ext = filename.split('.').last()?.to_lowercase();
    match ext.as_str() {
        "js" => Some("javascript"),
        "ts" => Some("typescript"),
        "coffee" => Some("coffee"),
        "rs" => Some("rust"),
        "md" | "markdown" => Some("markdown"),
        "html" => Some("html"),
        "css" => Some("css"),
        "json" => Some("json"),
        "py" => Some("python"),
        "sh" | "bash" | "zsh" => Some("sh"),
        "pl" => Some("perl"),
        "php" => Some("php"),
        "rb" => Some("ruby"),
        "cs" => Some("csharp"),
        "cpp" | "c" | "h" | "m" => Some("cpp"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "xml" => Some("xml"),
        "sql" => Some("sql"),
        _ => None,
    }
}

#[derive(Properties, PartialEq)]
pub struct FileOpenDialogProps {
    pub on_close: Callback<()>,
    pub on_select: Callback<(String, String, String)>, // (drive_id, title, category_id)
    pub leaf_data_id: String,
    pub categories: Vec<JSCategory>,
    pub on_refresh: Callback<()>,
    pub on_delete_category: Callback<String>,
    pub on_rename_category: Callback<(String, String)>, // (category_id, new_name)
    pub on_delete_file: Callback<(String, String)>, // (drive_id, filename)
    #[prop_or_default]
    pub on_start_processing: Callback<()>,
    #[prop_or_default]
    pub on_preview_toggle: Callback<bool>,
    #[prop_or_default]
    pub is_sub_dialog_open: bool,
    pub is_creating_category: bool,
    pub on_create_category_toggle: Callback<bool>,
    pub refresh_files_trigger: usize,
    pub is_loading: bool,
    pub on_loading_change: Callback<bool>,
    pub font_size: i32,
    pub on_change_font_size: Callback<i32>,
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
    let editing_category_id = use_state(|| None::<String>);
    let edit_name_input = use_state(|| "".to_string());
    let is_fading_out = use_state(|| false);
    let current_category_id = use_state(|| "".to_string());
    let current_category_name = use_state(|| "".to_string());
    let active_dropdown_file_id = use_state(|| None::<String>); 
    let preview_data = use_state(|| None::<FilePreview>);
    let is_loading_preview = use_state(|| false); // プレビュー用のみ維持（右側のみの表示のため）
    let is_deleting_id = use_state(|| None::<String>); // アニメーション用ステート
    let abort_controller = use_state(|| None::<AbortController>);
    let root_ref = use_node_ref();
    let dropdown_ref = use_node_ref(); 
    let edit_input_ref = use_node_ref();

    let is_sub_dialog_open = props.is_sub_dialog_open;

    // 編集モード開始時に入力フィールドへフォーカス
    {
        let edit_ref = edit_input_ref.clone();
        let editing_id = editing_category_id.clone();
        use_effect_with((*editing_id).clone(), move |id| {
            if id.is_some() {
                Timeout::new(10, move || {
                    if let Some(el) = edit_ref.cast::<web_sys::HtmlInputElement>() {
                        let _ = el.focus();
                        let _ = el.select();
                    }
                }).forget();
            }
            || ()
        });
    }

    // プレビュー状態を親に通知
    {
        let p_data = preview_data.clone();
        let on_toggle = props.on_preview_toggle.clone();
        use_effect_with((*p_data).clone(), move |preview| {
            on_toggle.emit(preview.is_some());
            || ()
        });
    }

    // プレビュー表示中のキーイベント制御
    {
        let p_data = preview_data.clone();
        use_effect_with((*p_data).clone(), move |preview| {
            if preview.is_none() { return Box::new(|| ()) as Box<dyn FnOnce()>; }
            
            let p_data = p_data.clone();
            let window = web_sys::window().unwrap();
            let mut opts = EventListenerOptions::run_in_capture_phase();
            opts.passive = false;
            
            let listener = EventListener::new_with_options(&window, "keydown", opts, move |e| {
                let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                let key = ke.key();
                if key == "Escape" || key == " " {
                    e.prevent_default();
                    e.stop_immediate_propagation();
                    p_data.set(None);
                    return;
                } else if key == "ArrowUp" || key == "ArrowDown" {
                    e.prevent_default();
                    e.stop_immediate_propagation();
                    
                    let doc = web_sys::window().unwrap().document().unwrap();
                    if let Ok(Some(el)) = doc.query_selector(".markdown-body") {
                        let scroll_amount = 40;
                        let current_scroll = el.scroll_top();
                        if key == "ArrowUp" {
                            el.set_scroll_top(current_scroll - scroll_amount);
                        } else {
                            el.set_scroll_top(current_scroll + scroll_amount);
                        }
                    }
                }
            });
            
            Box::new(move || drop(listener)) as Box<dyn FnOnce()>
        });
    }

    // 外側クリックでドロップダウンを閉じる
    {
        let dropdown_active = active_dropdown_file_id.clone();
        let dropdown_node = dropdown_ref.clone();
        use_effect_with((*dropdown_active).clone(), move |active_id: &Option<String>| {
            if active_id.is_none() { return Box::new(|| ()) as Box<dyn FnOnce()>; }
            
            let dropdown_active = dropdown_active.clone();
            let dropdown_node = dropdown_node.clone();
            let window = web_sys::window().unwrap();
            
            let listener = EventListener::new(&window, "mousedown", move |e| {
                let target = e.target().unwrap().unchecked_into::<web_sys::Node>();
                if let Some(dd_el) = dropdown_node.get() {
                    if !dd_el.contains(Some(&target)) {
                        dropdown_active.set(None);
                    }
                }
            });
            
            Box::new(move || drop(listener)) as Box<dyn FnOnce()>
        });
    }

    // 外部（app.rs）からのフォーカス復帰イベントを監視
    {
        let root = root_ref.clone();
        let f_area = focused_area.clone();
        use_effect_with(root, move |r| {
            let mut _listener = None;
            if let Some(el) = r.get() {
                let f_area = f_area.clone();
                _listener = Some(EventListener::new(&el, "leaf-focus-recovery", move |_| {
                    f_area.set(FocusedArea::Categories);
                }));
            }
            || ()
        });
    }

    let pending_delete_file = use_state(|| None::<(String, String)>); 
    let pending_move_file_id = use_state(|| None::<String>); 

    let load_files = {
        let files_state = files.clone();
        let selected_file_idx = selected_file_idx.clone();
        let on_loading_change = props.on_loading_change.clone();
        let current_category_id = current_category_id.clone();
        let current_category_name = current_category_name.clone();
        let focused_area = focused_area.clone();
        let is_fading_out = is_fading_out.clone();
        let abort_ctrl_state = abort_controller.clone();
        let dropdown_active = active_dropdown_file_id.clone();
        let pending_move = pending_move_file_id.clone();
        
        Callback::from(move |(cat_id, cat_name, is_initial): (String, String, bool)| {
            let files_state = files_state.clone();
            let selected_file_idx = selected_file_idx.clone();
            let on_loading_change = on_loading_change.clone();
            let current_category_id = current_category_id.clone();
            let current_category_name = current_category_name.clone();
            let focused_area = focused_area.clone();
            let is_fading_out = is_fading_out.clone();
            let abort_ctrl_state = abort_ctrl_state.clone();
            let dropdown_active = dropdown_active.clone();
            let pending_move = pending_move.clone();
            
            if let Some(ctrl) = (*abort_ctrl_state).as_ref() { ctrl.abort(); }
            let new_ctrl = AbortController::new().unwrap();
            let signal = new_ctrl.signal();
            abort_ctrl_state.set(Some(new_ctrl.clone()));

            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() { let _ = storage.set_item(STORAGE_KEY_LAST_CAT, &cat_id); }
            }

            files_state.set(Vec::new()); 
            on_loading_change.emit(true);
            current_category_id.set(cat_id.clone());
            current_category_name.set(cat_name);
            dropdown_active.set(None);
            pending_move.set(None);
            
            let signal_for_list = signal.clone();
            let on_loading_change_inner = on_loading_change.clone();
            spawn_local(async move {
                let res = list_files(&cat_id, Some(signal_for_list.clone())).await;
                if signal_for_list.aborted() { return; }

                if let Ok(res_val) = res {
                    if let Ok(files_val) = js_sys::Reflect::get(&res_val, &JsValue::from_str("files")) {
                        let array = js_sys::Array::from(&files_val);
                        let mut download_futures = Vec::new();
                        for i in 0..array.length() {
                            if download_futures.len() >= 10 { break; }
                            let v = array.get(i);
                            let id = js_sys::Reflect::get(&v, &JsValue::from_str("id")).unwrap().as_string().unwrap();
                            let name = js_sys::Reflect::get(&v, &JsValue::from_str("name")).unwrap().as_string().unwrap();
                            let size_val = js_sys::Reflect::get(&v, &JsValue::from_str("size")).unwrap_or(JsValue::UNDEFINED);
                            let total_size = size_val.as_string().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                            
                            let id_clone = id.clone();
                            let signal_inner = signal_for_list.clone();
                            download_futures.push(async move {
                                // 1KB をプリフェッチ (一覧表示高速化のため)
                                let range = if total_size > 1024 { Some("0-1023") } else { None };
                                
                                let content_val = match download_file(&id_clone, range, Some(signal_inner)).await {
                                    Ok(c_val) => c_val,
                                    Err(_) => JsValue::UNDEFINED,
                                };
                                
                                let (content, consumed) = if !content_val.is_undefined() {
                                    let res = crate::js_interop::get_safe_chunk(&content_val);
                                    let t = js_sys::Reflect::get(&res, &JsValue::from_str("text")).unwrap().as_string().unwrap_or_default();
                                    let b = js_sys::Reflect::get(&res, &JsValue::from_str("bytes_consumed")).unwrap().as_f64().unwrap_or(0.0) as u64;
                                    (t, b)
                                } else {
                                    ("".to_string(), 0u64)
                                };

                                FilePreview { id, name, content, total_size, loaded_bytes: consumed, is_markdown: false, lang: "".to_string() }
                            });
                        }
                        let previews = futures::future::join_all(download_futures).await;
                        if signal_for_list.aborted() { return; }

                        let has_files = !previews.is_empty();
                        files_state.set(previews);
                        selected_file_idx.set(0);

                        if is_initial && !*is_fading_out {
                            if has_files { focused_area.set(FocusedArea::Files); }
                            else { focused_area.set(FocusedArea::Categories); }
                        }
                    }
                }
                on_loading_change_inner.emit(false);
            });
        })
    };

    let sorted_categories = {
        let mut cats = props.categories.clone();
        cats.sort_by(|a, b| {
            if a.name == "OTHERS" { std::cmp::Ordering::Less }
            else if b.name == "OTHERS" { std::cmp::Ordering::Greater }
            else { a.name.cmp(&b.name) }
        });
        cats
    };

    {
        let sorted_cats = sorted_categories.clone();
        let load_files = load_files.clone();
        let selected_cat_idx = selected_cat_idx.clone();
        let current_cid = current_category_id.clone();
        let refresh_trigger = props.refresh_files_trigger;
        use_effect_with((sorted_cats.clone(), refresh_trigger), move |(cats, _)| {
            if !cats.is_empty() {
                let cid = (*current_cid).clone();
                let exists = cats.iter().any(|c| c.id == cid);
                
                let target_idx = if exists {
                    cats.iter().position(|c| c.id == cid).unwrap_or(0)
                } else {
                    let last_cat_id = web_sys::window()
                        .and_then(|w| w.local_storage().ok().flatten())
                        .and_then(|s| s.get_item(STORAGE_KEY_LAST_CAT).ok().flatten());

                    if let Some(id) = last_cat_id {
                        cats.iter().position(|c| c.id == id).unwrap_or_else(|| {
                            cats.iter().position(|c| c.name == "OTHERS").unwrap_or(0)
                        })
                    } else {
                        cats.iter().position(|c| c.name == "OTHERS").unwrap_or(0)
                    }
                };

                selected_cat_idx.set(target_idx);
                load_files.emit((cats[target_idx].id.clone(), cats[target_idx].name.clone(), false));
            }
            || ()
        });
    }

    {
        let root_ref = root_ref.clone();
        use_effect_with((), move |_| {
            let root = root_ref.clone();
            Timeout::new(10, move || {
                if let Some(div) = root.cast::<web_sys::HtmlElement>() { let _ = div.focus(); }
            }).forget();
            || ()
        });
    }

    let handle_close = {
        let on_close = props.on_close.clone();
        let is_fading_out = is_fading_out.clone();
        Callback::from(move |_: ()| {
            is_fading_out.set(true);
            let on_close = on_close.clone();
            Timeout::new(200, move || {
                on_close.emit(());
            }).forget();
        })
    };

    let on_ok_click = {
        let on_select = props.on_select.clone();
        let files = files.clone();
        let selected_file_idx = selected_file_idx.clone();
        let current_cat_id = current_category_id.clone();
        let is_loading_files = props.is_loading;
        let is_fading_out = is_fading_out.clone();
        let on_start = props.on_start_processing.clone();
        Callback::from(move |_: ()| {
            if !is_loading_files && !files.is_empty() && !*is_fading_out {
                let file = &files[*selected_file_idx];
                let drive_id = file.id.clone();
                let title = file.name.clone();
                let cat_id = (*current_cat_id).clone();
                let on_select = on_select.clone();
                let on_start = on_start.clone();
                is_fading_out.set(true);
                on_start.emit(());
                Timeout::new(200, move || {
                    on_select.emit((drive_id, title, cat_id));
                }).forget();
            }
        })
    };

    let on_keydown = {
        let focused_area = focused_area.clone();
        let selected_cat_idx = selected_cat_idx.clone();
        let selected_file_idx = selected_file_idx.clone();
        let categories = sorted_categories.clone();
        let files_c = files.clone();
        let load_files = load_files.clone();
        let on_ok = on_ok_click.clone();
        let is_fading_out = is_fading_out.clone();
        let is_loading_files = props.is_loading;
        let dropdown_active = active_dropdown_file_id.clone();
        let is_deleting_file = pending_delete_file.clone();
        let preview_data = preview_data.clone();
        let is_loading_preview_cb = is_loading_preview.clone();
        let h_close = handle_close.clone();

        Callback::from(move |e: KeyboardEvent| {
            let current_focus = *focused_area;
            if preview_data.is_some() || is_sub_dialog_open { return; }
            if *is_fading_out || dropdown_active.is_some() || is_deleting_file.is_some() { return; }
            
            let key = e.key();
            let code = e.code();
            let key_lower = key.to_lowercase();
            let is_m_shortcut = e.alt_key() && (code == "KeyM" || key_lower == "m" || key_lower == "µ");

            if is_m_shortcut {
                e.prevent_default();
                e.stop_immediate_propagation();
                h_close.emit(());
                return;
            }

            match key.as_str() {
                " " => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Files && !files_c.is_empty() {
                        let file = &files_c[*selected_file_idx];
                        let file_id = file.id.clone();
                        let file_name = file.name.clone();
                        let total_size = file.total_size;
                        let p_data = preview_data.clone();
                        let is_ld_prev = is_loading_preview_cb.clone();

                        let ext = file_name.split('.').last().unwrap_or("").to_lowercase();
                        let is_markdown = ext == "md" || ext == "markdown";
                        let lang = get_highlight_lang(&file_name).unwrap_or("").to_string();

                        is_ld_prev.set(true);
                        spawn_local(async move {
                            let range = if total_size > 102400 { Some("0-102399") } else { None };
                            if let Ok(cv) = download_file(&file_id, range, None).await {
                                let (content, consumed) = if !cv.is_undefined() {
                                    let res = crate::js_interop::get_safe_chunk(&cv);
                                    let t = js_sys::Reflect::get(&res, &JsValue::from_str("text")).unwrap().as_string().unwrap_or_default();
                                    let b = js_sys::Reflect::get(&res, &JsValue::from_str("bytes_consumed")).unwrap().as_f64().unwrap_or(0.0) as u64;
                                    (t, b)
                                } else { ("".to_string(), 0u64) };

                                p_data.set(Some(FilePreview { id: file_id, name: file_name, content, total_size, loaded_bytes: consumed, is_markdown, lang }));
                            }
                            is_ld_prev.set(false);
                        });
                    }
                }
                "Tab" => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Categories {
                        if !is_loading_files { focused_area.set(FocusedArea::Files); }
                    } else { focused_area.set(FocusedArea::Categories); }
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
                        if *selected_file_idx + 1 < files_c.len() { selected_file_idx.set(*selected_file_idx + 1); }
                    }
                }
                "Enter" => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Categories {
                        if !is_loading_files { focused_area.set(FocusedArea::Files); }
                    } else { on_ok.emit(()); }
                }
                _ => {}
            }
        })
    };

    let on_move_file = {
        let cur_cat_id = current_category_id.clone();
        let dropdown_active = active_dropdown_file_id.clone();
        let pending_move = pending_move_file_id.clone();
        let on_loading_change = props.on_loading_change.clone();
        let files_state = files.clone(); 
        Callback::from(move |(file_id, new_cat_id): (String, String)| {
            let cat_id = (*cur_cat_id).clone();
            let dropdown_active = dropdown_active.clone();
            let pending_move = pending_move.clone();
            let on_loading_change = on_loading_change.clone();
            let files_state = files_state.clone();
            let f_id = file_id.clone();
            
            dropdown_active.set(None);
            on_loading_change.emit(true); 
            
            spawn_local(async move {
                if let Ok(_) = move_file(&f_id, &cat_id, &new_cat_id).await {
                    on_loading_change.emit(false);
                    pending_move.set(Some(f_id.clone())); 
                    
                    Timeout::new(200, move || {
                        let mut current_files = (*files_state).clone();
                        current_files.retain(|f| f.id != f_id);
                        files_state.set(current_files);
                    }).forget();
                } else { 
                    on_loading_change.emit(false); 
                }
            });
        })
    };

    let on_delete_file_confirm = {
        let pending_delete = pending_delete_file.clone();
        let is_deleting = is_deleting_id.clone();
        let files_state = files.clone();
        let on_parent_delete = props.on_delete_file.clone();

        Callback::from(move |_: ()| {
            if let Some((id, name)) = (*pending_delete).clone() {
                let id_for_anim = id.clone();
                let id_for_parent = id.clone();
                let name_for_parent = name.clone();
                let pending = pending_delete.clone();
                let is_del = is_deleting.clone();
                let f_state = files_state.clone();
                let on_del = on_parent_delete.clone();

                pending.set(None); 
                is_del.set(Some(id_for_anim)); 

                Timeout::new(200, move || {
                    on_del.emit((id_for_parent.clone(), name_for_parent));
                    let mut current_files = (*f_state).clone();
                    current_files.retain(|f| f.id != id_for_parent);
                    f_state.set(current_files);
                    is_del.set(None);
                }).forget();
            }
        })
    };

    let on_focus_in = {
        let is_root_focused = is_root_focused.clone();
        Callback::from(move |_| is_root_focused.set(true))
    };
    let on_focus_out = {
        let is_root_focused = is_root_focused.clone();
        let root_ref = root_ref.clone();
        let focused_area = focused_area.clone();
        let preview_active = preview_data.is_some();
        
        Callback::from(move |e: FocusEvent| {
            is_root_focused.set(false);
            if preview_active || is_sub_dialog_open { return; } 

            let related_target = e.related_target();
            let is_outside = if let Some(target) = related_target {
                if let Some(root_el) = root_ref.cast::<web_sys::Node>() {
                    !root_el.contains(Some(&target.unchecked_into::<web_sys::Node>()))
                } else { true }
            } else { true };

            if is_outside {
                let root_ref_c = root_ref.clone();
                let f_area = focused_area.clone();
                
                Timeout::new(10, move || {
                    if let Some(div) = root_ref_c.cast::<web_sys::HtmlElement>() {
                        let _ = div.focus();
                        f_area.set(FocusedArea::Categories);
                        // s_idx.set(0); // インデックスのリセットを削除し、現在の選択を維持
                    }
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
                if *is_fading_out { "animate-backdrop-out" } else { "animate-backdrop-in" }
            )}
        >
            <div 
                class={classes!(
                    "bg-gray-800", "border", "border-gray-700", "rounded-lg", "shadow-2xl", "overflow-hidden", "flex", "flex-col", "relative",
                    if *is_fading_out { "animate-dialog-out" } else { "animate-dialog-in" }
                )}
                style="width: 60vw; height: 70vh;"
            >
                <div class="px-6 py-2 border-b border-gray-700 bg-gray-900 flex justify-between items-center">
                    <h3 class="text-lg font-bold text-white">{ i18n::t("file_selection", lang) }</h3>
                </div>

                if props.is_loading {
                    <div class="absolute inset-0 flex items-center justify-center bg-gray-900/40 z-50 backdrop-blur-[2px]">
                        <div class="w-12 h-12 border-4 border-lime-500 border-t-transparent rounded-full animate-spin shadow-lg"></div>
                    </div>
                }

                <div class="flex-1 flex overflow-hidden">
                    <div class="w-[30%] border-r border-gray-700 flex flex-col overflow-y-auto p-2 space-y-1 bg-gray-900/30">
                        <div class="flex space-x-1 mb-2">
                            <button 
                                onclick={let on_toggle = props.on_create_category_toggle.clone(); move |_| on_toggle.emit(true)}
                                class="flex-1 p-2 rounded-[6px] bg-gray-700 hover:bg-gray-600 shadow-md transition-all text-white flex items-center justify-center space-x-1"
                                title={ i18n::t("new_category", lang) }
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m3.75 9v6m3-3H9m1.5-12H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                                </svg>
                                <span class="text-[10px] font-bold">{ i18n::t("new_category", lang) }</span>
                            </button>
                            <button 
                                onclick={let cb = props.on_refresh.clone(); move |_| cb.emit(())}
                                class="p-2 rounded-[6px] bg-gray-700 hover:bg-gray-600 shadow-md transition-all text-white"
                                title={ i18n::t("refresh_categories", lang) }
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99" />
                                </svg>
                            </button>
                        </div>
                        { for sorted_categories.iter().enumerate().map(|(idx, cat)| {
                            let is_selected = *selected_cat_idx == idx;
                            let is_editing = (*editing_category_id).as_ref() == Some(&cat.id);
                            let area_active = *focused_area == FocusedArea::Categories && *is_root_focused;
                            let is_focused = is_selected && area_active && !is_editing;
                            
                            let id_for_change = cat.id.clone();
                            let name = cat.name.clone();
                            let display_name = if name == "OTHERS" { i18n::t("OTHERS", lang) } else { name.clone() };
                            let load_files = load_files.clone();
                            let selected_cat_idx = selected_cat_idx.clone();
                            let on_delete = props.on_delete_category.clone();
                            let on_rename = props.on_rename_category.clone();
                            let is_no_cat = cat.name == "OTHERS";

                            let editing_id = editing_category_id.clone();
                            let edit_input = edit_name_input.clone();

                            html! {
                                <div key={cat.id.clone()} class={classes!(
                                    "w-full", "rounded-[6px]", "transition-all", "flex", "items-center", "group/cat", "border-[3px]",
                                    if is_focused { vec!["border-lime-400", "ring-1", "ring-lime-400"] } else { vec!["border-transparent"] },
                                    if is_focused { vec!["bg-blue-600", "text-white"] }
                                    else if is_selected { vec!["bg-slate-600", "text-gray-200"] }
                                    else { vec!["bg-gray-700/50", "text-gray-400", "hover:bg-gray-700"] }
                                )}
                                style="height: 6.2%; min-height: 32px; margin-bottom: 0.4%;"
                                >
                                    if is_editing {
                                        <div class="flex-1 flex items-center px-2 space-x-1 h-full">
                                            <input 
                                                ref={edit_input_ref.clone()}
                                                type="text"
                                                value={(*edit_input).clone()}
                                                oninput={let edit_input = edit_input.clone(); Callback::from(move |e: InputEvent| {
                                                    let input: web_sys::HtmlInputElement = e.target_unchecked_into();
                                                    edit_input.set(input.value());
                                                })}
                                                onkeydown={let editing_id = editing_id.clone(); let edit_input = edit_input.clone(); let on_rename = on_rename.clone(); let id = cat.id.clone(); Callback::from(move |e: KeyboardEvent| {
                                                    e.stop_propagation();
                                                    if e.key() == "Enter" && !e.is_composing() {
                                                        let new_name = (*edit_input).trim().to_string();
                                                        if !new_name.is_empty() {
                                                            on_rename.emit((id.clone(), new_name));
                                                        }
                                                        editing_id.set(None);
                                                    } else if e.key() == "Escape" {
                                                        editing_id.set(None);
                                                    }
                                                })}
                                                class="flex-1 bg-gray-900 border border-gray-600 rounded px-2 py-0.5 text-xs text-white outline-none focus:border-blue-500"
                                            />
                                            <button 
                                                onclick={let editing_id = editing_id.clone(); let edit_input = edit_input.clone(); let on_rename = on_rename.clone(); let id = cat.id.clone(); move |e: MouseEvent| {
                                                    e.stop_propagation();
                                                    let new_name = (*edit_input).trim().to_string();
                                                    if !new_name.is_empty() {
                                                        on_rename.emit((id.clone(), new_name));
                                                    }
                                                    editing_id.set(None);
                                                }}
                                                class="p-1 hover:bg-gray-600 rounded transition-colors"
                                                title={i18n::t("save", lang)}
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4 text-green-500">
                                                    <path fill-rule="evenodd" d="M16.704 4.153a.75.75 0 01.143 1.052l-8 10.5a.75.75 0 01-1.127.075l-4.5-4.5a.75.75 0 011.06-1.06l3.894 3.893 7.48-9.817a.75.75 0 011.05-.143z" clip-rule="evenodd" />
                                                </svg>
                                            </button>
                                            <button 
                                                onclick={let editing_id = editing_id.clone(); move |e: MouseEvent| { e.stop_propagation(); editing_id.set(None); }}
                                                class="p-1 hover:bg-gray-600 rounded transition-colors"
                                                title={i18n::t("cancel", lang)}
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4 text-red-500">
                                                    <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
                                                </svg>
                                            </button>
                                        </div>
                                    } else {
                                        <button 
                                            onclick={move |_| { selected_cat_idx.set(idx); load_files.emit((id_for_change.clone(), name.clone(), false)); }}
                                            class="flex-1 text-left px-4 truncate h-full flex items-center outline-none"
                                        >
                                            <span class="truncate">{ display_name }</span>
                                        </button>
                                        if !is_no_cat {
                                            <div class="flex items-center opacity-0 group-hover/cat:opacity-100 transition-opacity">
                                                <button
                                                    onclick={let id = cat.id.clone(); let name = cat.name.clone(); let editing_id = editing_id.clone(); let edit_input = edit_input.clone(); move |e: MouseEvent| { 
                                                        e.stop_propagation(); 
                                                        editing_id.set(Some(id.clone())); 
                                                        edit_input.set(name.clone());
                                                    }}
                                                    class="p-1.5 text-gray-500 hover:text-blue-400 outline-none"
                                                    title={i18n::t("edit", lang)}
                                                >
                                                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5">
                                                        <path d="M5.433 13.917l1.262-3.155A4 4 0 017.58 9.42l6.92-6.918a2.121 2.121 0 013 3l-6.92 6.918c-.383.383-.84.685-1.343.886l-3.154 1.262a.5.5 0 01-.65-.65z" />
                                                        <path d="M3.5 5.75c0-.69.56-1.25 1.25-1.25H10A.75.75 0 0010 3H4.75A2.75 2.75 0 002 5.75v9.5A2.75 2.75 0 004.75 18h9.5A2.75 2.75 0 0017 15.25V10a.75.75 0 00-1.5 0v5.25c0 .69-.56 1.25-1.25 1.25h-9.5c-.69 0-1.25-.56-1.25-1.25v-9.5z" />
                                                    </svg>
                                                </button>
                                                <button
                                                    onclick={let id = cat.id.clone(); move |e: MouseEvent| { e.stop_propagation(); on_delete.emit(id.clone()); }}
                                                    class="p-1.5 text-gray-500 hover:text-red-400 outline-none"
                                                    title={i18n::t("delete", lang)}
                                                >
                                                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5">
                                                        <path fill-rule="evenodd" d="M8.75 1A2.75 2.75 0 006 3.75v.443c-.795.077-1.584.176-2.365.298a.75.75 0 102.244 1.487l.263-.041.608 11.137A2.75 2.75 0 007.5 19h5a2.75 2.75 0 002.747-2.597l.608-11.137.263.041a.75.75 0 102.244-1.487A48.112 48.112 0 0014 4.193V3.75A2.75 2.75 0 0011.25 1h-2.5zM10 4c.84 0 1.673.025 2.5.075V3.75c0-.69-.56-1.25-1.25-1.25h-2.5c-.69 0-1.25.56-1.25 1.25v.325C8.327 4.025 9.16 4 10 4zM8.58 7.72a.75.75 0 00-1.5.06l.3 7.5a.75.75 0 101.498-.06l-.3-7.5zm4.34.06a.75.75 0 10-1.498-.06l-.3 7.5a.75.75 0 001.5.06l.3-7.5z" clip-rule="evenodd" />
                                                    </svg>
                                                </button>
                                            </div>
                                        }
                                    }
                                </div>
                            }
                        }) }
                    </div>

                    <div class="w-[70%] flex flex-col overflow-y-auto relative bg-gray-800/20">
                        if *is_loading_preview {
                            <div class="absolute inset-0 flex items-center justify-center bg-gray-800/30 z-40 backdrop-blur-[1px]">
                                <div class="w-10 h-10 border-4 border-lime-500 border-t-transparent rounded-full animate-spin"></div>
                            </div>
                        }
                        { for files.iter().enumerate().map(|(idx, file)| {
                            let is_selected = *selected_file_idx == idx;
                            let area_active = *focused_area == FocusedArea::Files && *is_root_focused;
                            let is_focused = is_selected && area_active;
                            
                            let s_idx_1 = selected_file_idx.clone();
                            let s_idx_2 = selected_file_idx.clone();
                            let is_loading_files = props.is_loading;
                            let on_ok = on_ok_click.clone();
                            let file_id = file.id.clone();
                            let file_name = file.name.clone();
                            let active_drop = active_dropdown_file_id.clone();
                            let is_drop_open = (*active_drop).as_ref() == Some(&file_id);
                            let on_move = on_move_file.clone();
                            let pending_del = pending_delete_file.clone();
                            let is_fading_item = (*pending_move_file_id).as_ref() == Some(&file_id);
                            let is_deleting_item = (*is_deleting_id).as_ref() == Some(&file_id);
                            let current_cid = (*current_category_id).clone();

                            html! {
                                <div key={file_id.clone()} class={classes!(
                                    "relative", "group/fileitem", "w-full", "px-1.5", "transition-all", "duration-300",
                                    if is_deleting_item { "h-0 p-0 opacity-0 scale-95 pointer-events-none overflow-hidden" } else { "h-[20%] py-1.5 overflow-visible" },
                                    if is_fading_item { "opacity-0 scale-95 pointer-events-none" } else { "" },
                                    if is_drop_open { "z-30" } else { "z-0" }
                                )}>
                                    <button 
                                        onclick={let focused_area = focused_area.clone(); move |_| { if !is_loading_files { s_idx_1.set(idx); focused_area.set(FocusedArea::Files); } }}
                                        ondblclick={let on_ok = on_ok.clone(); move |_| { s_idx_2.set(idx); on_ok.emit(()); }}
                                        class={classes!(
                                            "w-full", "h-full", "text-left", "px-4", "py-2", "rounded-[6px]", "shadow-md", "transition-all", "overflow-hidden", "flex", "flex-col", "border-[3px]",
                                            if is_focused { vec!["border-lime-400", "ring-1", "ring-lime-400"] } else { vec!["border-transparent"] },
                                            if is_focused { vec!["bg-blue-600", "text-white"] }
                                            else if is_selected { vec!["bg-slate-600", "text-gray-200"] }
                                            else { vec!["bg-gray-700/50", "text-gray-400", "hover:bg-gray-700"] }
                                        )}
                                    >
                                        <div class="font-bold text-[10px] opacity-50 mb-0.5 truncate shrink-0">{ &file.name }</div>
                                        <div class="text-xs flex-1 whitespace-pre-wrap font-mono opacity-80 pr-12 leading-snug overflow-hidden">
                                            { &file.content }
                                        </div>
                                    </button>
                                    
                                    <div class={classes!(
                                        "absolute", "top-3", "right-3", "flex", "space-x-1", "z-20", "pointer-events-auto"
                                    )}>
                                        <div class="relative">
                                            <button 
                                                onclick={let active = active_drop.clone(); let id = file_id.clone(); move |e: MouseEvent| { e.stop_propagation(); if (*active).as_ref() == Some(&id) { active.set(None); } else { active.set(Some(id.clone())); } }}
                                                class="p-1.5 rounded bg-gray-600 hover:bg-gray-500 text-white shadow-md border border-gray-500 transition-opacity duration-200 opacity-30 group-hover/fileitem:opacity-80"
                                                title={i18n::t("change_category", lang)}
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-4 h-4">
                                                    <path d="M3 3h8v8H3V3zm0 10h8v8H3v-8zm10-10h8v8h-8V3zm0 10h8v8h-8v-8z" />
                                                </svg>
                                            </button>
                                            if is_drop_open {
                                                <div ref={dropdown_ref.clone()} class="absolute right-0 top-full mt-1 w-48 bg-gray-800 border border-gray-700 rounded-md shadow-xl z-50 overflow-hidden py-1 opacity-100">
                                                    { for sorted_categories.iter().map(|c| {
                                                        let cat_id = c.id.clone();
                                                        let fid = file_id.clone();
                                                        let on_m = on_move.clone();
                                                        let is_curr = cat_id == current_cid;
                                                        let display_name = if c.name == "OTHERS" { i18n::t("OTHERS", lang) } else { c.name.clone() };
                                                        html! {
                                                            <button 
                                                                onclick={if is_curr { 
                                                                    Callback::from(|e: MouseEvent| e.stop_propagation()) 
                                                                } else { 
                                                                    let on_m = on_m.clone();
                                                                    let fid = fid.clone();
                                                                    let cat_id = cat_id.clone();
                                                                    Callback::from(move |e: MouseEvent| { 
                                                                        e.stop_propagation(); 
                                                                        on_m.emit((fid.clone(), cat_id.clone())); 
                                                                    }) 
                                                                }}
                                                                class={classes!(
                                                                    "w-full", "text-left", "px-4", "py-2", "text-xs", "transition-colors",
                                                                    if is_curr { "text-gray-600 cursor-default bg-gray-900/50" } 
                                                                    else { "text-gray-300 hover:bg-blue-600 hover:text-white" }
                                                                )}
                                                            >
                                                                { display_name }
                                                            </button>
                                                        }
                                                    }) }
                                                </div>
                                            }
                                        </div>
                                        <button 
                                            onclick={let fid = file_id.clone(); let fname = file_name.clone(); move |e: MouseEvent| { e.stop_propagation(); pending_del.set(Some((fid.clone(), fname.clone()))); }}
                                            class="p-1.5 rounded bg-gray-600 hover:bg-red-600 text-white shadow-md border border-gray-500 transition-opacity duration-200 opacity-30 group-hover/fileitem:opacity-80"
                                            title={i18n::t("delete", lang)}
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
                                                <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.112 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.112 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
                                            </svg>
                                        </button>
                                    </div>
                                </div>
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
                            { i18n::t("ok", lang) }
                        </button>
                        <button 
                            onclick={handle_close.reform(|_| ())}
                            class="px-6 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded-[6px] shadow-lg transition-all"
                        >
                            { i18n::t("cancel", lang) }
                        </button>
                    </div>
                </div>
            </div>
            if props.is_creating_category {
                <InputDialog 
                    title={i18n::t("new_category", lang)} 
                    message={i18n::t("enter_category_name_message", lang)} 
                    on_confirm={
                        let on_toggle = props.on_create_category_toggle.clone();
                        let on_loading_change = props.on_loading_change.clone();
                        let ldid = props.leaf_data_id.clone();
                        let on_refresh = props.on_refresh.clone();
                        Callback::from(move |name: String| {
                            let on_toggle = on_toggle.clone();
                            let on_loading_change = on_loading_change.clone();
                            let ldid = ldid.clone();
                            let on_refresh = on_refresh.clone();
                            if !name.trim().is_empty() {
                                on_toggle.emit(false);
                                on_loading_change.emit(true);
                                spawn_local(async move {
                                    if let Ok(_) = crate::drive_interop::create_folder(&name, &ldid).await {
                                        on_refresh.emit(());
                                    }
                                    on_loading_change.emit(false);
                                });
                            } else {
                                on_toggle.emit(false);
                            }
                        })
                    }
                    on_cancel={let on_toggle = props.on_create_category_toggle.clone(); Callback::from(move |_| on_toggle.emit(false))}
                />
            }
            if let Some((_, _)) = (*pending_delete_file).clone() {
                <ConfirmDialog 
                    title={i18n::t("delete", lang)} 
                    message={i18n::t("confirm_delete_file", lang)} 
                    on_confirm={on_delete_file_confirm} 
                    on_cancel={let pending = pending_delete_file.clone(); move |_| pending.set(None)} 
                />
            }
            {
                if let Some(p) = (*preview_data).clone() {
                    let content = if p.is_markdown {
                        p.content.clone()
                    } else {
                        format!("```{}\n{}\n```", p.lang, p.content)
                    };
                    let has_more = p.loaded_bytes < p.total_size;
                    html! {
                        <Preview 
                            content={content} 
                            on_close={let p_data = preview_data.clone(); Callback::from(move |_| p_data.set(None))} 
                            has_more={has_more}
                            disable_space_scroll={true}
                            is_sub_dialog_open={is_sub_dialog_open}
                            font_size={props.font_size}
                            on_change_font_size={props.on_change_font_size.clone()}
                        />
                    }
                } else { html! { <></> } }
            }
        </div>
    }
}
