use yew::prelude::*;
use gloo::events::EventListener;
use gloo::events::EventListenerOptions;
use gloo::timers::callback::Timeout;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use crate::js_interop::get_safe_chunk;
use crate::drive_interop::{list_files, download_file, move_file};
use crate::i18n::{self, Language};
use crate::db_interop::JSCategory;
use crate::components::preview::Preview;
use web_sys::AbortController;
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Clone, PartialEq)]
struct FilePreview {
    id: String,
    name: String,
    content: String,
    total_size: u64,
    loaded_bytes: u64,
    is_markdown: bool,
    lang: String,
    is_loaded: bool, // 読み込み完了フラグ
}

enum FileAction {
    Set(Vec<FilePreview>),
    UpdateContent(String, String, u64), // id, content, loaded_bytes
    Remove(String),
    Clear,
}

struct FileState {
    list: Vec<FilePreview>,
}

impl Reducible for FileState {
    type Action = FileAction;
    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            FileAction::Set(list) => Rc::new(FileState { list }),
            FileAction::UpdateContent(id, content, loaded_bytes) => {
                let mut list = self.list.clone();
                if let Some(f) = list.iter_mut().find(|x| x.id == id) {
                    f.content = content; 
                    f.loaded_bytes = loaded_bytes;
                    f.is_loaded = true; // 内容が（空であっても）確定した
                }
                Rc::new(FileState { list })
            }
            FileAction::Remove(id) => {
                let mut list = self.list.clone();
                list.retain(|x| x.id != id);
                Rc::new(FileState { list })
            }
            FileAction::Clear => Rc::new(FileState { list: Vec::new() }),
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct FileOpenDialogProps {
    pub on_close: Callback<()>,
    pub on_select: Callback<(String, String, String)>, // id, title, cat_id
    pub leaf_data_id: String,
    pub categories: Vec<JSCategory>,
    pub on_refresh: Callback<()>,
    pub on_delete_category: Callback<String>,
    pub on_rename_category: Callback<(String, String)>,
    pub on_delete_file: Callback<(String, String)>,
    pub on_start_processing: Callback<()>,
    pub on_preview_toggle: Callback<bool>,
    pub is_sub_dialog_open: bool,
    pub is_creating_category: bool,
    pub on_create_category_toggle: Callback<bool>,
    pub refresh_files_trigger: usize,
    pub is_loading: bool,
    pub on_loading_change: Callback<bool>,
    #[prop_or_default]
    pub on_network_status_change: Callback<bool>,
    pub font_size: i32,
    pub on_change_font_size: Callback<i32>,
}

#[derive(PartialEq, Clone, Copy)]
enum FocusedArea { Categories, Files }

const STORAGE_KEY_LAST_CAT: &str = "leaf_last_category";

#[function_component(FileOpenDialog)]
pub fn file_open_dialog(props: &FileOpenDialogProps) -> Html {
    let lang = Language::detect();
    let is_wide_layout = use_state(|| false);
    let focused_area = use_state(|| FocusedArea::Categories);
    let is_root_focused = use_state(|| false); 
    let selected_cat_idx = use_state(|| 0usize);
    let selected_file_idx = use_state(|| None::<usize>);
    let files = use_reducer(|| FileState { list: Vec::new() });
    let editing_category_id = use_state(|| None::<String>);
    let edit_name_input = use_state(|| "".to_string());
    let is_fading_out = use_state(|| false);
    let current_category_id = use_state(|| "".to_string());
    let current_category_name = use_state(|| "".to_string());
    let active_dropdown_file_id = use_state(|| None::<String>); 
    let preview_modal_data = use_state(|| None::<FilePreview>);
    let is_preview_fading_out = use_state(|| false); 
    let is_loading_preview = use_state(|| false); 
    let is_deleting_id = use_state(|| None::<String>);
    let abort_controller = use_state(|| None::<AbortController>);
    let fetching_ids = use_mut_ref(|| HashSet::<String>::new());
    let pending_delete_file = use_state(|| None::<(String, String)>); 
    let pending_move_file_id = use_state(|| None::<String>); 
    let processing_move_id = use_state(|| None::<String>); 

    let root_ref = use_node_ref();
    let dropdown_ref = use_node_ref(); 
    let edit_input_ref = use_node_ref();
    let preview_area_ref = use_node_ref();
    let cat_list_ref = use_node_ref();
    let file_list_ref = use_node_ref();
    let preview_modal_scroll_ref = use_node_ref();

    let _is_sub_dialog_open = props.is_sub_dialog_open;

    // ウィンドウサイズ監視
    {
        let is_wide = is_wide_layout.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let check_size = {
                let is_wide_c = is_wide.clone();
                let window_c = window.clone();
                move || {
                    let win_w = window_c.inner_width().unwrap().as_f64().unwrap_or(0.0);
                    let scr_w = window_c.screen().ok().and_then(|s| s.width().ok()).map(|w| w as f64).unwrap_or(1920.0);
                    is_wide_c.set(win_w > scr_w / 2.0);
                }
            };
            check_size();
            let listener = EventListener::new(&window, "resize", move |_| { check_size(); });
            move || { drop(listener); }
        });
    }

    // フォーカス制御
    {
        let root_ref_c = root_ref.clone();
        let is_ld = props.is_loading;
        let is_sub = props.is_sub_dialog_open;
        let p_modal_active = preview_modal_data.is_some();
        use_effect_with((is_ld, is_sub, p_modal_active), move |(ld, sub, p_active)| {
            if !*ld && !*sub && !*p_active {
                let r = root_ref_c.clone();
                Timeout::new(150, move || { if let Some(div) = r.cast::<web_sys::HtmlElement>() { let _ = div.focus(); } }).forget();
            }
            || ()
        });
    }

    // プレビュー閉鎖
    let handle_close_preview = {
        let p_data = preview_modal_data.clone();
        let is_p_fading = is_preview_fading_out.clone();
        let on_prev_toggle = props.on_preview_toggle.clone();
        Callback::from(move |_: ()| {
            is_p_fading.set(true);
            let p_data_c = p_data.clone();
            let is_p_fading_c = is_p_fading.clone();
            let on_prev_toggle_c = on_prev_toggle.clone();
            Timeout::new(200, move || { 
                p_data_c.set(None); 
                is_p_fading_c.set(false); 
                on_prev_toggle_c.emit(false);
            }).forget();
        })
    };

    // モーダルキー制御 (スクロール対応)
    {
        let p_data = preview_modal_data.clone();
        let close_p = handle_close_preview.clone();
        let scroll_ref = preview_modal_scroll_ref.clone();
        use_effect_with((*p_data).clone(), move |preview| {
            if preview.is_none() { return Box::new(|| ()) as Box<dyn FnOnce()>; }
            let close_p_c = close_p.clone();
            let scroll_ref_c = scroll_ref.clone();
            let window = web_sys::window().unwrap();
            let mut opts = EventListenerOptions::run_in_capture_phase(); opts.passive = false;
            let listener = EventListener::new_with_options(&window, "keydown", opts, move |e| {
                let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                let key = ke.key();
                
                if key == "Escape" || key == " " { 
                    e.prevent_default(); 
                    e.stop_propagation(); 
                    e.stop_immediate_propagation();
                    close_p_c.emit(()); 
                    return;
                }

                if let Some(el) = scroll_ref_c.cast::<web_sys::HtmlElement>() {
                    let scroll_step = 40;
                    let page_step = (el.client_height() as f64 * 0.5) as i32;
                    match key.as_str() {
                        "ArrowUp" => { e.prevent_default(); e.stop_propagation(); el.set_scroll_top(el.scroll_top() - scroll_step); }
                        "ArrowDown" => { e.prevent_default(); e.stop_propagation(); el.set_scroll_top(el.scroll_top() + scroll_step); }
                        "PageUp" => { e.prevent_default(); e.stop_propagation(); el.set_scroll_top(el.scroll_top() - page_step); }
                        "PageDown" => { e.prevent_default(); e.stop_propagation(); el.set_scroll_top(el.scroll_top() + page_step); }
                        "Home" => { e.prevent_default(); e.stop_propagation(); el.set_scroll_top(0); }
                        "End" => { e.prevent_default(); e.stop_propagation(); el.set_scroll_top(el.scroll_height()); }
                        _ => {}
                    }
                }
            });
            Box::new(move || drop(listener)) as Box<dyn FnOnce()>
        });
    }

    let sorted_categories = use_memo(props.categories.clone(), |cats| {
        let mut sorted = cats.clone();
        sorted.sort_by(|a, b| {
            if a.name == "OTHERS" { std::cmp::Ordering::Less }
            else if b.name == "OTHERS" { std::cmp::Ordering::Greater }
            else { a.name.cmp(&b.name) }
        });
        sorted
    });

    // 各シートの10KB先読み処理
    let trigger_prefetch = {
        let files_reducer = files.clone();
        let fetching_ids = fetching_ids.clone();
        Callback::from(move |(file_info, signal): ((String, u64), web_sys::AbortSignal)| {
            let (file_id, total_size) = file_info;
            if fetching_ids.borrow().contains(&file_id) { return; }
            
            fetching_ids.borrow_mut().insert(file_id.clone());
            let reducer = files_reducer.clone();
            let ids = fetching_ids.clone();
            spawn_local(async move {
                let range = if total_size > 10240 { Some("0-10239") } else { None };
                if let Ok(cv) = download_file(&file_id, range, Some(signal)).await {
                    let safe = get_safe_chunk(&cv);
                    let t = js_sys::Reflect::get(&safe, &JsValue::from_str("text")).unwrap().as_string().unwrap_or_default();
                    let b = js_sys::Reflect::get(&safe, &JsValue::from_str("bytes_consumed")).unwrap().as_f64().unwrap_or(0.0) as u64;
                    reducer.dispatch(FileAction::UpdateContent(file_id.clone(), t, b));
                }
                ids.borrow_mut().remove(&file_id);
            });
        })
    };

    let handle_close = {
        let on_close = props.on_close.clone();
        let is_fading_out_h = is_fading_out.clone();
        Callback::from(move |_: ()| {
            is_fading_out_h.set(true);
            let on_close_inner = on_close.clone();
            Timeout::new(200, move || { on_close_inner.emit(()); }).forget();
        })
    };

    let load_files = {
        let files_reducer = files.clone();
        let selected_file_idx = selected_file_idx.clone();
        let on_loading_change = props.on_loading_change.clone();
        let current_category_id = current_category_id.clone();
        let current_category_name = current_category_name.clone();
        let abort_ctrl_state = abort_controller.clone();
        let fetching_ids = fetching_ids.clone();
        let is_fading_out_h = is_fading_out.clone();
        let f_area_h = focused_area.clone();
        let on_nc_c = props.on_network_status_change.clone();
        let pending_move = pending_move_file_id.clone();
        let processing_move = processing_move_id.clone();
        Callback::from(move |(cat_id, cat_name, is_initial): (String, String, bool)| {
            if let Some(ctrl) = (*abort_ctrl_state).as_ref() { ctrl.abort(); }
            let new_ctrl = AbortController::new().unwrap();
            let signal = new_ctrl.signal();
            abort_ctrl_state.set(Some(new_ctrl.clone()));
            if let Some(window) = web_sys::window() { if let Ok(Some(storage)) = window.local_storage() { let _ = storage.set_item(STORAGE_KEY_LAST_CAT, &cat_id); } }
            
            files_reducer.dispatch(FileAction::Clear);
            fetching_ids.borrow_mut().clear();
            selected_file_idx.set(None);
            pending_move.set(None);
            processing_move.set(None);
            current_category_id.set(cat_id.clone());
            current_category_name.set(cat_name);
            on_loading_change.emit(true);

            let reducer_inner = files_reducer.clone();
            let sig_inner = signal.clone();
            let on_ld_inner = on_loading_change.clone();
            let cid_inner = cat_id.clone();
            let f_area_inner = f_area_h.clone();
            let is_fading_inner = is_fading_out_h.clone();
            let on_nc_inner = on_nc_c.clone();

            spawn_local(async move {
                let res = list_files(&cid_inner, Some(sig_inner.clone())).await;
                if sig_inner.aborted() { return; }
                
                if let Ok(res_val) = res {
                    on_nc_inner.emit(true); 
                    if let Ok(files_val) = js_sys::Reflect::get(&res_val, &JsValue::from_str("files")) {
                        let array = js_sys::Array::from(&files_val);
                        let mut all_metadata = Vec::new();
                        for i in 0..array.length() {
                            let v = array.get(i);
                            let id = js_sys::Reflect::get(&v, &JsValue::from_str("id")).unwrap().as_string().unwrap();
                            let name = js_sys::Reflect::get(&v, &JsValue::from_str("name")).unwrap().as_string().unwrap();
                            let size_val = js_sys::Reflect::get(&v, &JsValue::from_str("size")).unwrap_or(JsValue::UNDEFINED);
                            let total_size = size_val.as_string().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                            let ext = name.split('.').last().unwrap_or("").to_lowercase();
                            all_metadata.push(FilePreview { id, name, content: "".to_string(), total_size, loaded_bytes: 0, is_markdown: ext == "md" || ext == "markdown", lang: ext, is_loaded: false });
                        }
                        all_metadata.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                        reducer_inner.dispatch(FileAction::Set(all_metadata));
                    }
                } else {
                    on_nc_inner.emit(false);
                }
                on_ld_inner.emit(false);
                if is_initial && !*is_fading_inner { f_area_inner.set(FocusedArea::Categories); }
            });
        })
    };

    // ファイルリストが更新されたらバックグラウンド読み込みを開始
    {
        let list = files.list.clone();
        let prefetch = trigger_prefetch.clone();
        let abort_ctrl = abort_controller.clone();
        use_effect_with(list, move |list| {
            if let Some(ctrl) = (*abort_ctrl).as_ref() {
                let signal = ctrl.signal();
                for file in list.iter() {
                    if !file.is_loaded {
                        prefetch.emit(((file.id.clone(), file.total_size), signal.clone()));
                    }
                }
            }
            || ()
        });
    }

    // 編集モード監視
    {
        let edit_ref = edit_input_ref.clone();
        let editing_id = editing_category_id.clone();
        use_effect_with((*editing_id).clone(), move |id| {
            if id.is_some() { Timeout::new(10, move || { if let Some(el) = edit_ref.cast::<web_sys::HtmlInputElement>() { let _ = el.focus(); let _ = el.select(); } }).forget(); }
            || ()
        });
    }
    // ドロップダウンの外側クリックで閉じる
    {
        let ads_state = active_dropdown_file_id.clone();
        let dr = dropdown_ref.clone();
        use_effect_with((*active_dropdown_file_id).clone(), move |active_id| {
            let mut _listener = None;
            if active_id.is_some() {
                let ads = ads_state.clone();
                let dr = dr.clone();
                let window = web_sys::window().unwrap();
                _listener = Some(EventListener::new(&window, "mousedown", move |e| {
                    let target = e.target();
                    if let (Some(target), Some(dropdown_el)) = (target, dr.get()) {
                        let node = target.unchecked_into::<web_sys::Node>();
                        if !dropdown_el.contains(Some(&node)) {
                            ads.set(None);
                        }
                    }
                }));
            }
            || { drop(_listener); }
        });
    }

    // 選択インデックス変更時の自動スクロール
    {
        let file_list_ref_c = file_list_ref.clone();
        let selected_idx = *selected_file_idx;
        use_effect_with(selected_idx, move |idx| {
            if let Some(i) = *idx {
                if let Some(container) = file_list_ref_c.cast::<web_sys::Element>() {
                    crate::js_interop::scroll_into_view_graceful(&container, i as u32, 200.0);
                }
            }
            || ()
        });
    }

    {
        let root = root_ref.clone();
        use_effect_with(root, move |r| {
            let mut _listener = None;
            if let Some(el) = r.get() {
                _listener = Some(EventListener::new(&el, "leaf-focus-recovery", move |_| { }));
            }
            || ()
        });
    }

    {
        let sorted_cats = sorted_categories.clone();
        let load_files_c = load_files.clone();
        let current_cid_c = current_category_id.clone();
        let selected_cat_idx_c = selected_cat_idx.clone();
        let refresh_trigger = props.refresh_files_trigger;
        let cats_len = sorted_cats.len();
        use_effect_with((refresh_trigger, cats_len), move |_| {
            if !sorted_cats.is_empty() {
                let cid = (*current_cid_c).clone();
                let target_idx = sorted_cats.iter().position(|c| c.id == cid).unwrap_or_else(|| {
                    let last_id = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).and_then(|s| s.get_item(STORAGE_KEY_LAST_CAT).ok().flatten());
                    if let Some(id) = last_id { sorted_cats.iter().position(|c| c.id == id).unwrap_or_else(|| sorted_cats.iter().position(|c| c.name == "OTHERS").unwrap_or(0)) }
                    else { sorted_cats.iter().position(|c| c.name == "OTHERS").unwrap_or(0) }
                });
                selected_cat_idx_c.set(target_idx);
                load_files_c.emit((sorted_cats[target_idx].id.clone(), sorted_cats[target_idx].name.clone(), false));
            }
            || ()
        });
    }

    let on_ok_click = {
        let on_select = props.on_select.clone();
        let files_reducer = files.clone();
        let selected_file_idx = selected_file_idx.clone();
        let current_cat_id = current_category_id.clone();
        let is_fading_out_h = is_fading_out.clone();
        let on_start = props.on_start_processing.clone();
        let is_loading = props.is_loading;
        Callback::from(move |_: ()| {
            if let Some(idx) = *selected_file_idx {
                if !is_loading && !files_reducer.list.is_empty() && !*is_fading_out_h {
                    let file = &files_reducer.list[idx];
                    let drive_id = file.id.clone(); let title = file.name.clone(); let cat_id = (*current_cat_id).clone();
                    let on_select_inner = on_select.clone(); let on_start_inner = on_start.clone();
                    is_fading_out_h.set(true); on_start_inner.emit(());
                    Timeout::new(200, move || { on_select_inner.emit((drive_id, title, cat_id)); }).forget();
                }
            }
        })
    };

    let on_keydown = {
        let focused_area_c = focused_area.clone();
        let selected_cat_idx_c = selected_cat_idx.clone();
        let selected_file_idx_c = selected_file_idx.clone();
        let categories_c = sorted_categories.clone();
        let files_reducer = files.clone();
        let load_files_cc = load_files.clone();
        let on_ok_c = on_ok_click.clone();
        let is_fading_out_cc = is_fading_out.clone();
        let is_deleting_cc = is_deleting_id.clone();
        let preview_modal_c = preview_modal_data.clone();
        let is_preview_fading_out_c = is_preview_fading_out.clone();
        let is_loading_preview_cc = is_loading_preview.clone();
        let h_close_c = handle_close.clone();
        let on_prev_toggle_c = props.on_preview_toggle.clone();
        let is_sub_dialog_open = props.is_sub_dialog_open;
        let is_creating_cat = props.is_creating_category;
        let is_loading_prev_val = *is_loading_preview;
        let has_pending_del = pending_delete_file.is_some();

        Callback::from(move |e: KeyboardEvent| {
            let current_focus = *focused_area_c;
            if preview_modal_c.is_some() || is_sub_dialog_open || is_creating_cat || is_loading_prev_val || has_pending_del {
                return;
            }
            if *is_fading_out_cc || is_deleting_cc.is_some() { return; }
            let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
            let key = ke.key(); let code = ke.code(); let key_lower = key.to_lowercase();
            let is_m_shortcut = e.alt_key() && (code == "KeyM" || key_lower == "m" || key_lower == "µ");
            if is_m_shortcut { e.prevent_default(); e.stop_immediate_propagation(); h_close_c.emit(()); return; }
            match key.as_str() {
                " " => {
                    e.prevent_default();
                    if let Some(idx) = *selected_file_idx_c {
                        if current_focus == FocusedArea::Files && !files_reducer.list.is_empty() {
                            let file = &files_reducer.list[idx];
                            let file_id = file.id.clone(); let file_name = file.name.clone(); let total_size = file.total_size;
                            let p_modal = preview_modal_c.clone(); let is_ld_prev = is_loading_preview_cc.clone();
                            let is_md = file.is_markdown; let lang_c = file.lang.clone();
                            let is_fade = is_preview_fading_out_c.clone();
                            
                            if file.is_loaded {
                                is_fade.set(false);
                                on_prev_toggle_c.emit(true);
                                p_modal.set(Some(FilePreview { id: file_id, name: file_name, content: file.content.clone(), total_size, loaded_bytes: file.loaded_bytes, is_markdown: is_md, lang: lang_c, is_loaded: true }));
                            } else {
                                is_ld_prev.set(true);
                                let on_pt = on_prev_toggle_c.clone();
                                spawn_local(async move {
                                    if let Ok(cv) = download_file(&file_id, None, None).await {
                                        let safe = get_safe_chunk(&cv);
                                        let t = js_sys::Reflect::get(&safe, &JsValue::from_str("text")).unwrap().as_string().unwrap_or_default();
                                        let b = js_sys::Reflect::get(&safe, &JsValue::from_str("bytes_consumed")).unwrap().as_f64().unwrap_or(0.0) as u64;
                                        is_fade.set(false);
                                        on_pt.emit(true);
                                        p_modal.set(Some(FilePreview { id: file_id, name: file_name, content: t, total_size, loaded_bytes: b, is_markdown: is_md, lang: lang_c, is_loaded: true }));
                                    }
                                    is_ld_prev.set(false);
                                });
                            }
                        }
                    }
                }
                "Tab" => { 
                    e.prevent_default(); 
                    if current_focus == FocusedArea::Categories { 
                        focused_area_c.set(FocusedArea::Files); 
                        if selected_file_idx_c.is_none() && !files_reducer.list.is_empty() { selected_file_idx_c.set(Some(0)); }
                    } else { 
                        focused_area_c.set(FocusedArea::Categories); 
                    } 
                }
                "ArrowUp" => { 
                    e.prevent_default(); 
                    if current_focus == FocusedArea::Categories { 
                        if *selected_cat_idx_c > 0 { let new_idx = *selected_cat_idx_c - 1; selected_cat_idx_c.set(new_idx); load_files_cc.emit((categories_c[new_idx].id.clone(), categories_c[new_idx].name.clone(), false)); } 
                    } else {
                        let cur_idx = selected_file_idx_c.unwrap_or(0);
                        if cur_idx > 0 { selected_file_idx_c.set(Some(cur_idx - 1)); }
                        else if selected_file_idx_c.is_none() && !files_reducer.list.is_empty() { selected_file_idx_c.set(Some(0)); }
                    }
                }
                "ArrowDown" => { 
                    e.prevent_default(); 
                    if current_focus == FocusedArea::Categories { 
                        if *selected_cat_idx_c + 1 < categories_c.len() { let new_idx = *selected_cat_idx_c + 1; selected_cat_idx_c.set(new_idx); load_files_cc.emit((categories_c[new_idx].id.clone(), categories_c[new_idx].name.clone(), false)); } 
                    } else {
                        let cur_idx = selected_file_idx_c.unwrap_or(0);
                        if selected_file_idx_c.is_none() && !files_reducer.list.is_empty() { selected_file_idx_c.set(Some(0)); }
                        else if cur_idx + 1 < files_reducer.list.len() { selected_file_idx_c.set(Some(cur_idx + 1)); }
                    }
                }
                "Enter" => { e.prevent_default(); if current_focus == FocusedArea::Categories { focused_area_c.set(FocusedArea::Files); if selected_file_idx_c.is_none() && !files_reducer.list.is_empty() { selected_file_idx_c.set(Some(0)); } } else { on_ok_c.emit(()); } }
                _ => {}
            }
        })
    };

    let on_move_file = {
        let cur_cid_c = current_category_id.clone();
        let files_reducer = files.clone(); 
        let pending_move = pending_move_file_id.clone();
        let ads_state = active_dropdown_file_id.clone();
        let proc_move = processing_move_id.clone();
        Callback::from(move |(file_id, new_cat_id): (String, String)| {
            ads_state.set(None); 
            let old_cid = (*cur_cid_c).clone();
            let reducer = files_reducer.clone(); let f_id = file_id.clone();
            let p_move = pending_move.clone();
            let proc_m = proc_move.clone();
            
            proc_m.set(Some(f_id.clone()));
            spawn_local(async move {
                if let Ok(_) = move_file(&f_id, &old_cid, &new_cat_id).await {
                    proc_m.set(None); 
                    p_move.set(Some(f_id.clone())); 
                    Timeout::new(200, move || { reducer.dispatch(FileAction::Remove(f_id.clone())); }).forget();
                } else { 
                    proc_m.set(None);
                }
            });
        })
    };

    let on_delete_file_confirm = {
        let pending_delete_c = pending_delete_file.clone();
        let is_del_id_c = is_deleting_id.clone();
        let files_reducer = files.clone();
        let on_parent_delete_c = props.on_delete_file.clone();
        Callback::from(move |_: ()| {
            if let Some((id, name)) = (*pending_delete_c).clone() {
                let id_for_anim = id.clone(); let id_for_parent = id.clone(); let name_for_parent = name.clone();
                let reducer = files_reducer.clone(); let on_del = on_parent_delete_c.clone();
                let is_del = is_del_id_c.clone();
                pending_delete_c.set(None); is_del.set(Some(id_for_anim)); 
                Timeout::new(200, move || {
                    on_del.emit((id_for_parent.clone(), name_for_parent));
                    reducer.dispatch(FileAction::Remove(id_for_parent.clone()));
                    is_del.set(None);
                }).forget();
            }
        })
    };

    let on_focus_in = { let is_root_f = is_root_focused.clone(); Callback::from(move |_| is_root_f.set(true)) };
    let on_focus_out = {
        let root_ref_c = root_ref.clone();
        let preview_active = preview_modal_data.is_some();
        let sub_active = props.is_sub_dialog_open || props.is_creating_category || (*pending_delete_file).is_some() || props.is_loading || *is_loading_preview;
        Callback::from(move |e: FocusEvent| {
            if preview_active || sub_active { return; } 
            let related = e.related_target();
            let outside = if let Some(target) = related { if let Some(root_el) = root_ref_c.cast::<web_sys::Node>() { !root_el.contains(Some(&target.unchecked_into::<web_sys::Node>())) } else { true } } else { true };
            if outside {
                let root_inner = root_ref_c.clone();
                Timeout::new(10, move || { if let Some(div) = root_inner.cast::<web_sys::HtmlElement>() { let _ = div.focus(); } }).forget();
            }
        })
    };

    let current_preview_file = if let Some(idx) = *selected_file_idx { if files.list.is_empty() { None } else { Some(files.list[idx].clone()) } } else { None };

    // --- HTMLパーツ ---
    let categories_html = {
        let idx = *selected_cat_idx;
        let area_active = *focused_area == FocusedArea::Categories && *is_root_focused;
        let editing_id = (*editing_category_id).clone();
        let categories = (*sorted_categories).clone();
        let load_files_cb = load_files.clone();
        let s_idx_state = selected_cat_idx.clone();
        let on_ren_inner = props.on_rename_category.clone();
        let eid_inner = editing_category_id.clone();
        let ein_inner = edit_name_input.clone();
        let on_del_inner = props.on_delete_category.clone();
        let edit_ref = edit_input_ref.clone();
        let is_wide = *is_wide_layout;
        let focused_area_h = focused_area.clone();

        html! {
            <div class={classes!("flex", "flex-col", if is_wide { "w-[20%]" } else { "w-[40%]" }, "border-r", "border-white/5", "bg-gray-900/50")}>
                <div class="p-4 border-b border-white/5 flex items-center justify-between">
                    <span class="text-xs font-bold text-gray-500 uppercase tracking-widest">{ i18n::t("new_category", lang) }</span>
                    <button 
                        onclick={let ic = props.on_create_category_toggle.clone(); move |_| ic.emit(true)}
                        class="p-1 hover:bg-white/10 rounded-md text-gray-400 transition-colors"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                        </svg>
                    </button>
                </div>
                <div ref={cat_list_ref} class="flex-1 overflow-y-auto custom-scrollbar p-2 space-y-1">
                    { for categories.into_iter().enumerate().map(|(i, cat)| {
                        let is_sel = idx == i;
                        let is_editing = editing_id.as_ref() == Some(&cat.id);
                        let is_active = is_sel && area_active;
                        let cid_val = cat.id.clone(); 
                        let cname_val = cat.name.clone();
                        let load_inner = load_files_cb.clone(); let s_idx_inner = s_idx_state.clone();
                        let eid_inner = eid_inner.clone(); let ein_inner = ein_inner.clone();
                        let on_ren = on_ren_inner.clone(); let on_del = on_del_inner.clone();
                        let cid_for_rename = cid_val.clone();
                        let cid_for_delete = cid_val.clone();

                        html! {
                            <div 
                                class={classes!(
                                    "group", "relative", "flex", "items-center", "px-3", "py-2", "rounded-md", "cursor-pointer", "transition-all", "duration-200",
                                    if is_sel { vec!["bg-emerald-600/20", "text-emerald-400"] } else { vec!["text-gray-400", "hover:bg-white/5", "hover:text-gray-200"] },
                                    if is_active { vec!["ring-2", "ring-emerald-500/50", "bg-emerald-600/30"] } else { vec![] }
                                )}
                                onclick={let f_area = focused_area_h.clone(); move |_| { s_idx_inner.set(i); f_area.set(FocusedArea::Categories); load_inner.emit((cid_val.clone(), cname_val.clone(), false)); }}
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" class={classes!("h-4", "w-4", "mr-3", if is_sel { "text-emerald-500" } else { "text-gray-600" })} fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                                </svg>
                                if is_editing {
                                    <input 
                                        ref={edit_ref.clone()} type="text" value={(*ein_inner).clone()}
                                        oninput={let ein = ein_inner.clone(); move |e: InputEvent| { let input: web_sys::HtmlInputElement = e.target_unchecked_into(); ein.set(input.value()); }}
                                        onblur={let eid = eid_inner.clone(); move |_| eid.set(None)}
                                        onkeydown={let eid = eid_inner.clone(); let ein = ein_inner.clone(); let cid_inner_val = cid_for_rename.clone(); let on_ren = on_ren.clone(); move |e: KeyboardEvent| { if e.key() == "Enter" { let new_name = (*ein).clone(); if !new_name.trim().is_empty() { on_ren.emit((cid_inner_val.clone(), new_name)); } eid.set(None); } else if e.key() == "Escape" { eid.set(None); } }}
                                        class="bg-gray-800 text-white text-sm border-none outline-none w-full px-1 rounded"
                                    />
                                } else {
                                    <span class="flex-1 truncate text-sm font-medium">{ if cat.name == "OTHERS" { i18n::t("OTHERS", lang) } else { cat.name.clone() } }</span>
                                    if cat.name != "OTHERS" {
                                        <div class="hidden group-hover:flex items-center space-x-1 ml-2">
                                            <button onclick={let eid = eid_inner.clone(); let ein = ein_inner.clone(); let cid = cid_for_rename.clone(); let cn = cat.name.clone(); move |e: MouseEvent| { e.stop_propagation(); eid.set(Some(cid.clone())); ein.set(cn.clone()); }} class="p-1 hover:text-emerald-400 transition-colors"><svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" /></svg></button>
                                            <button onclick={let on_del = on_del.clone(); let cid = cid_for_delete.clone(); move |e: MouseEvent| { e.stop_propagation(); on_del.emit(cid.clone()); }} class="p-1 hover:text-red-400 transition-colors"><svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg></button>
                                        </div>
                                    }
                                }
                            </div>
                        }
                    }) }
                </div>
                <div class="p-2 border-t border-white/5">
                    <button onclick={let on_ref = props.on_refresh.clone(); move |_| on_ref.emit(())} class="w-full flex items-center justify-center space-x-2 px-3 py-2 rounded-md text-xs font-bold text-gray-500 hover:bg-white/5 hover:text-gray-300 transition-all uppercase tracking-widest"><svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" /></svg><span>{ i18n::t("refresh_categories", lang) }</span></button>
                </div>
            </div>
        }
    };

    let files_html = {
        let idx_opt = *selected_file_idx;
        let area_active = *focused_area == FocusedArea::Files && *is_root_focused;
        let file_list = files.list.clone();
        let s_idx_state = selected_file_idx.clone();
        let on_ok = on_ok_click.clone();
        let active_dropdown = (*active_dropdown_file_id).clone();
        let active_dropdown_state = active_dropdown_file_id.clone();
        let is_ld_id = (*is_deleting_id).clone();
        let p_move_id = (*pending_move_file_id).clone();
        let proc_move_id = (*processing_move_id).clone();
        let categories = (*sorted_categories).clone();
        let current_cid = (*current_category_id).clone();
        let on_move = on_move_file.clone();
        let p_del_state = pending_delete_file.clone();
        let is_wide = *is_wide_layout;
        let focused_area_h = focused_area.clone();

        html! {
            <div class={classes!("flex", "flex-col", "bg-gray-900", "min-w-0", "h-full", if is_wide { "w-[30%]" } else { "w-[60%]" })}>
                <div class="p-4 border-b border-white/5 flex items-center justify-between bg-gray-950/20">
                    <div class="flex items-center space-x-2">
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-emerald-500" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" /></svg>
                        <h2 class="text-sm font-bold text-gray-200 tracking-tight">{ format!("{} ({})", if *current_category_name == "OTHERS" { i18n::t("OTHERS", lang) } else { (*current_category_name).clone() }, file_list.len()) }</h2>
                    </div>
                </div>
                <div ref={file_list_ref} class="flex-1 overflow-y-auto custom-scrollbar flex flex-col p-2">
                    if props.is_loading && file_list.is_empty() {
                        <div class="flex-1 flex flex-col items-center justify-center space-y-4">
                            <div class="w-8 h-8 border-2 border-emerald-500/30 border-t-emerald-500 rounded-full animate-spin"></div>
                        </div>
                    } else if file_list.is_empty() {
                        <div class="flex-1 flex flex-col items-center justify-center text-gray-600 space-y-4">
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-12 w-12 opacity-20" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" /></svg>
                            <p class="text-xs uppercase tracking-widest font-bold">{ i18n::t("no_files_found", lang) }</p>
                        </div>
                    } else {
                        { for file_list.into_iter().enumerate().map(|(i, file)| {
                            let is_sel = idx_opt == Some(i);
                            let is_active = is_sel && area_active;
                            let is_dropdown_open = active_dropdown.as_ref() == Some(&file.id);
                            let is_deleting = is_ld_id.as_ref() == Some(&file.id);
                            let is_moving = p_move_id.as_ref() == Some(&file.id);
                            let is_processing = proc_move_id.as_ref() == Some(&file.id);
                            
                            let s_idx_inner = s_idx_state.clone();
                            let on_ok_inner = on_ok.clone();
                            let ads_inner = active_dropdown_state.clone();
                            let file_id_inner = file.id.clone();
                            let file_name_inner = file.name.clone();
                            let p_del_inner = p_del_state.clone();
                            let categories_for_item = categories.clone();
                            let current_cid_for_item = current_cid.clone();
                            let on_move_for_item = on_move.clone();

                            html! {
                                <div 
                                    class={classes!(
                                        "group", "relative", "flex", "flex-col", "p-0", "rounded", "cursor-pointer", "transition-all", "duration-200", "border", "h-24", "min-h-[6rem]", "flex-shrink-0", "mx-1", "mb-1", "overflow-visible",
                                        if is_dropdown_open { vec!["z-50", "bg-gray-800/90", "border-emerald-500", "shadow-2xl"] }
                                        else if is_active { vec!["bg-emerald-600", "text-white", "shadow-lg", "z-10", "border-white", "ring-4", "ring-emerald-500/30", "scale-[1.01]"] } 
                                        else if is_sel { vec!["bg-emerald-600/10", "text-emerald-400/80", "border-emerald-500/30", "z-0"] }
                                        else { vec!["text-gray-400", "hover:bg-white/5", "border-white/10", "z-0"] },
                                        if is_deleting || is_moving { vec!["opacity-0", "scale-95", "translate-x-4"] } else { vec!["opacity-100", "scale-100"] }
                                    )}
                                    onclick={let f_area = focused_area_h.clone(); move |_| { s_idx_inner.set(Some(i)); f_area.set(FocusedArea::Files); }}
                                    ondblclick={move |_| on_ok_inner.emit(())}
                                >
                                    <div class="flex flex-col w-full h-full">
                                        // ファイル名表示エリア（高さを半分に、はみ出しを許可、右端フェード）
                                        <div class="px-3 h-4 flex items-center justify-between w-full flex-shrink-0 relative overflow-visible mt-1.5 mb-1">
                                            <div class="flex items-center space-x-2 overflow-hidden pr-14 w-full">
                                                <svg xmlns="http://www.w3.org/2000/svg" class={classes!("h-2.5", "w-2.5", "flex-shrink-0", if is_sel { "text-white" } else { "text-gray-600" })} fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
                                                </svg>
                                                <span class="file-name-fade whitespace-nowrap text-[11px] font-bold opacity-90 leading-none">{ &file.name }</span>
                                            </div>
                                            <div class="flex items-center space-x-0.5 absolute right-2 top-[-4px] overflow-visible">
                                                <div class="relative">
                                                    <button 
                                                        onclick={let ads = ads_inner.clone(); let fid = file_id_inner.clone(); move |e: MouseEvent| { e.stop_propagation(); if is_dropdown_open { ads.set(None); } else { ads.set(Some(fid.clone())); } }}
                                                        class={classes!("p-1", "rounded-md", "hover:bg-black/20", "transition-colors", if is_sel { "text-white" } else { "text-gray-500" })}
                                                        title="Change Category"
                                                    >
                                                        <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg>
                                                    </button>
                                                    if is_dropdown_open {
                                                        <div ref={dropdown_ref.clone()} class="absolute right-0 top-full mt-1 w-48 bg-gray-800 border border-white/10 rounded-lg shadow-2xl z-[60] py-1 animate-in fade-in zoom-in-95 duration-100">
                                                            <div class="px-3 py-1.5 text-[10px] font-bold text-gray-500 uppercase tracking-widest border-b border-white/5 mb-1">{ "Move to category" }</div>
                                                            <div class="max-h-48 overflow-y-auto custom-scrollbar">
                                                                { for categories_for_item.iter().filter(|c| c.id != current_cid_for_item).map(|c| {
                                                                    let on_mv = on_move_for_item.clone(); let fid = file_id_inner.clone(); let tcid = c.id.clone();
                                                                    let cname = c.name.clone();
                                                                    html! { <button onclick={move |e: MouseEvent| { e.stop_propagation(); on_mv.emit((fid.clone(), tcid.clone())); }} class="w-full text-left px-4 py-2 text-xs text-gray-300 hover:bg-emerald-600 hover:text-white transition-colors flex items-center space-x-2"><svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 opacity-50" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg><span>{ if cname == "OTHERS" { i18n::t("OTHERS", lang) } else { cname } }</span></button> }
                                                                }) }
                                                            </div>
                                                        </div>
                                                    }
                                                </div>
                                                <button 
                                                    onclick={let fid = file_id_inner.clone(); let fname = file_name_inner.clone(); let p_del = p_del_inner.clone(); move |e: MouseEvent| { e.stop_propagation(); p_del.set(Some((fid.clone(), fname.clone()))); }}
                                                    class={classes!("p-1", "rounded-md", "hover:bg-red-500/40", "transition-colors", if is_sel { "text-white" } else { "text-gray-500" })}
                                                    title="Delete Sheet"
                                                >
                                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                                                </button>
                                            </div>
                                        </div>
                                        // コンテンツプレビュー（冒頭3行を表示）
                                        <div class={classes!(
                                            "px-3", "pb-3", "text-xs", "font-bold", "line-clamp-3", "leading-snug", "break-all", "overflow-hidden",
                                            if !file.is_loaded { "opacity-70" } else if is_sel { "text-emerald-50" } else { "text-gray-300" }
                                        )}>
                                            if !file.is_loaded { 
                                                <div class="flex items-center space-x-2 py-1">
                                                    <div class="w-3 h-3 border-2 border-emerald-500/30 border-t-emerald-500 rounded-full animate-spin"></div>
                                                    <span class="text-[10px] uppercase tracking-widest animate-pulse font-black text-emerald-500/60">{ "Loading" }</span>
                                                </div>
                                            } else {
                                                { file.content.clone() }
                                            }
                                        </div>
                                    </div>
                                    if is_processing {
                                        <div class="absolute inset-0 z-[100] bg-gray-900/60 backdrop-blur-[1px] flex items-center justify-center rounded animate-in fade-in duration-200">
                                            <div class="bg-emerald-600 p-2 rounded-full shadow-lg border border-emerald-400">
                                                <div class="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></div>
                                            </div>
                                        </div>
                                    }
                                </div>
                            }
                        }) }
                    }
                </div>
            </div>
        }
    };

    let preview_area_html = {
        let file_opt = current_preview_file;
        let font_size = props.font_size;
        let on_change_fs = props.on_change_font_size.clone();
        let is_wide = *is_wide_layout;

        html! {
            <div ref={preview_area_ref} class={classes!("flex", "flex-col", "bg-gray-950", "overflow-hidden", "relative", if is_wide { vec!["w-[50%]", "border-l"] } else { vec!["flex-1"] }, "border-white/5")}>
                if let Some(file) = file_opt {
                    <div class="flex-1 flex flex-col min-h-0">
                        <div class="px-4 py-3 bg-gray-900/50 border-b border-white/5 flex items-center justify-between flex-shrink-0">
                            <div class="flex items-center space-x-2 min-w-0">
                                <span class="px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-400 text-[10px] font-bold uppercase tracking-tight flex-shrink-0">{ &file.lang }</span>
                                <h3 class="text-xs font-bold text-gray-300 truncate">{ &file.name }</h3>
                            </div>
                            <div class="flex items-center space-x-2 ml-4 flex-shrink-0">
                                <button onclick={let fs = font_size; let cb = on_change_fs.clone(); move |_| cb.emit(fs - 1)} class="p-1 hover:bg-white/10 rounded text-gray-500 hover:text-gray-300 transition-colors"><svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 12H4" /></svg></button>
                                <span class="text-[10px] font-mono text-gray-600 min-w-[20px] text-center">{ font_size }</span>
                                <button onclick={let fs = font_size; let cb = on_change_fs.clone(); move |_| cb.emit(fs + 1)} class="p-1 hover:bg-white/10 rounded text-gray-500 hover:text-gray-300 transition-colors"><svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" /></svg></button>
                            </div>
                        </div>
                        
                        <Preview 
                            key={file.id.clone()}
                            content={file.content.clone()} 
                            lang={file.lang.clone()}
                            on_close={Callback::from(|_| ())} // 埋め込み時は閉じない
                            font_size={font_size}
                            is_embedded={true}
                            has_more={file.is_loaded && file.loaded_bytes < file.total_size}
                            is_loading={!file.is_loaded}
                        />
                    </div>
                } else {
                    <div class="flex-1 flex flex-col items-center justify-center text-gray-800 space-y-4">
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-16 w-16 opacity-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" /></svg>
                        <p class="text-xs uppercase tracking-[0.2em] font-black opacity-10">{ "Select a file to preview" }</p>
                    </div>
                }
            </div>
        }
    };

    html! {
        <div 
            ref={root_ref}
            tabindex="0"
            onkeydown={on_keydown}
            onfocusin={on_focus_in}
            onfocusout={on_focus_out}
            class={classes!(
                "fixed", "inset-0", "z-[100]", "flex", "items-center", "justify-center", "p-4", "md:p-8", "outline-none", "pointer-events-auto",
                if *is_fading_out { "animate-backdrop-out" } else { "animate-backdrop-in" }
            )}
            onclick={|e: MouseEvent| e.stop_propagation()}
        >
            <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" onclick={handle_close.reform(|_| ())}></div>

            <div class={classes!(
                "relative", "flex", "flex-col", "bg-gray-900", "border", "border-white/10", "rounded-xl", "shadow-2xl", "h-full", "max-h-[80vh]", "overflow-hidden",
                if *is_wide_layout { vec!["w-[70vw]"] } else { vec!["w-full", "max-w-6xl"] },
                if *is_fading_out { "animate-dialog-out" } else { "animate-dialog-in" }
            )} onclick={|e: MouseEvent| e.stop_propagation()}>
                // メインコンテンツエリア (Categories + Files + Preview)
                <div class={classes!("flex-1", "flex", if *is_wide_layout { "flex-row" } else { "flex-col" }, "overflow-hidden")}>
                    if *is_wide_layout {
                        { categories_html }
                        { files_html }
                        { preview_area_html }
                    } else {
                        // 狭い画面: リスト(上70%) と プレビュー(下30%)
                        <div class="h-[70%] flex overflow-hidden border-b border-white/5 flex-shrink-0">
                            { categories_html }
                            { files_html }
                        </div>
                        <div class="h-[30%] flex flex-col overflow-hidden">
                            { preview_area_html }
                        </div>
                    }
                </div>

                // フッターエリア (横いっぱい)
                <div class="p-4 bg-gray-950/50 border-t border-white/5 flex items-center justify-between">
                    <p class="text-[10px] text-gray-500 font-bold uppercase tracking-tighter">{ i18n::t("guide_keys", lang) }</p>
                    <div class="flex space-x-2">
                        <button onclick={handle_close.reform(|_| ())} class="px-4 py-1.5 rounded-md text-xs font-bold text-gray-400 hover:bg-white/5 transition-all uppercase tracking-widest">{ i18n::t("cancel", lang) }</button>
                        <button 
                            onclick={on_ok_click.reform(|_| ())} 
                            disabled={selected_file_idx.is_none() || props.is_loading} 
                            class={classes!(
                                "px-6", "py-1.5", "rounded-md", "text-xs", "font-bold", "text-white", "transition-all", "uppercase", "tracking-widest",
                                if selected_file_idx.is_none() || props.is_loading { vec!["bg-gray-800", "text-gray-600", "cursor-not-allowed"] } else { vec!["bg-emerald-600", "hover:bg-emerald-500", "shadow-lg", "shadow-emerald-900/20"] }
                            )}
                        >
                            { i18n::t("ok", lang) }
                        </button>
                    </div>
                </div>
            </div>

            if props.is_creating_category {
                <div class="z-[210]">
                    <crate::components::dialog::InputDialog 
                        title={i18n::t("new_category", lang)} message={i18n::t("enter_category_name_message", lang)} 
                        on_confirm={let on_refresh = props.on_refresh.clone(); let create_toggle = props.on_create_category_toggle.clone(); let leaf_id = props.leaf_data_id.clone(); Callback::from(move |name: String| { let on_ref = on_refresh.clone(); let ct = create_toggle.clone(); let lid_val = leaf_id.clone(); spawn_local(async move { if let Ok(_) = crate::drive_interop::create_folder(&name, &lid_val).await { on_ref.emit(()); } ct.emit(false); }); })} 
                        on_cancel={let ct = props.on_create_category_toggle.clone(); Callback::from(move |_| ct.emit(false))} 
                    />
                </div>
            }

            if let Some((_id, name)) = (*pending_delete_file).clone() {
                <div class="z-[210]">
                    <crate::components::dialog::ConfirmDialog 
                        title={i18n::t("delete", lang)} message={format!("{}{}", i18n::t("confirm_delete_file", lang), name)} 
                        on_confirm={on_delete_file_confirm} 
                        on_cancel={let pd = pending_delete_file.clone(); Callback::from(move |_| pd.set(None))} 
                    />
                </div>
            }

            if *is_loading_preview {
                <div class="fixed inset-0 z-[400] flex flex-col items-center justify-center bg-black/40 backdrop-blur-[2px] animate-in fade-in duration-200">
                    <div class="bg-gray-900/90 border border-white/10 p-8 rounded-2xl shadow-2xl flex flex-col items-center">
                        <div class="w-12 h-12 border-4 border-blue-500/30 border-t-blue-500 rounded-full animate-spin"></div>
                    </div>
                </div>
            }

            if let Some(file) = (*preview_modal_data).clone() {
                <Preview 
                    key={file.id.clone()}
                    content={file.content.clone()} 
                    lang={file.lang.clone()}
                    on_close={handle_close_preview.clone()}
                    is_sub_dialog_open={props.is_sub_dialog_open}
                    font_size={props.font_size}
                    on_change_font_size={props.on_change_font_size.clone()}
                    is_fading_out={*is_preview_fading_out}
                    has_more={file.is_loaded && file.loaded_bytes < file.total_size}
                    close_on_space={true}
                />
            }
        </div>
    }
}
