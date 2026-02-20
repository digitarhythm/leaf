use yew::prelude::*;
use crate::drive_interop::{list_files, download_file, move_file};
use crate::db_interop::JSCategory;
use crate::i18n::{self, Language};
use crate::components::dialog::{InputDialog, ConfirmDialog};
use crate::components::preview::Preview;
use crate::js_interop::{render_markdown, init_mermaid};
use wasm_bindgen::{JsValue, JsCast};
use wasm_bindgen_futures::spawn_local;
use web_sys::{KeyboardEvent, AbortController};
use gloo::timers::callback::Timeout;
use gloo::events::{EventListener, EventListenerOptions};
use std::rc::Rc;
use std::collections::HashSet;

#[derive(Clone, PartialEq)]
pub struct FilePreview {
    pub id: String,
    pub name: String,
    pub content: String,
    pub total_size: u64,
    pub loaded_bytes: u64,
    pub is_markdown: bool,
    pub lang: String,
    pub is_prefetched: bool,
}

enum FileAction {
    SetFiles(Vec<FilePreview>),
    UpdateContent { id: String, content: String, loaded_bytes: u64 },
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
            FileAction::SetFiles(files) => Rc::new(FileState { list: files }),
            FileAction::UpdateContent { id, content, loaded_bytes } => {
                let mut list = self.list.clone();
                if let Some(item) = list.iter_mut().find(|f| f.id == id) {
                    if !item.is_prefetched {
                        item.content = content;
                        item.loaded_bytes = loaded_bytes;
                        item.is_prefetched = true;
                    }
                }
                Rc::new(FileState { list })
            }
            FileAction::Remove(id) => {
                let mut list = self.list.clone();
                list.retain(|f| f.id != id);
                Rc::new(FileState { list })
            }
            FileAction::Clear => Rc::new(FileState { list: Vec::new() }),
        }
    }
}

fn get_highlight_lang(filename: &str) -> Option<&str> {
    let ext = filename.split('.').last()?.to_lowercase();
    match ext.as_str() {
        "js" => Some("javascript"), "ts" => Some("typescript"), "coffee" => Some("coffee"),
        "rs" => Some("rust"), "md" | "markdown" => Some("markdown"), "html" => Some("html"),
        "css" => Some("css"), "json" => Some("json"), "py" => Some("python"),
        "sh" | "bash" | "zsh" => Some("sh"), "pl" => Some("perl"), "php" => Some("php"),
        "rb" => Some("ruby"), "cs" => Some("csharp"), "cpp" | "c" | "h" | "m" => Some("cpp"),
        "toml" => Some("toml"), "yaml" | "yml" => Some("yaml"), "xml" => Some("xml"), "sql" => Some("sql"),
        _ => None,
    }
}

#[derive(Properties, PartialEq)]
pub struct FileOpenDialogProps {
    pub on_close: Callback<()>,
    pub on_select: Callback<(String, String, String)>,
    pub leaf_data_id: String,
    pub categories: Vec<JSCategory>,
    pub on_refresh: Callback<()>,
    pub on_delete_category: Callback<String>,
    pub on_rename_category: Callback<(String, String)>,
    pub on_delete_file: Callback<(String, String)>,
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
    let selected_file_idx = use_state(|| 0usize);
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

    let root_ref = use_node_ref();
    let dropdown_ref = use_node_ref(); 
    let edit_input_ref = use_node_ref();
    let preview_area_ref = use_node_ref();
    let cat_list_ref = use_node_ref();
    let file_list_ref = use_node_ref();

    let is_sub_dialog_open = props.is_sub_dialog_open;

    // ウィンドウサイズ監視
    {
        let is_wide = is_wide_layout.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let check_size = {
                let is_wide = is_wide.clone();
                let window = window.clone();
                move || {
                    let win_w = window.inner_width().unwrap().as_f64().unwrap_or(0.0);
                    let scr_w = window.screen().ok().and_then(|s| s.width().ok()).map(|w| w as f64).unwrap_or(1920.0);
                    is_wide.set(win_w > scr_w / 2.0);
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
        Callback::from(move |_: ()| {
            is_p_fading.set(true);
            let p_data_c = p_data.clone();
            let is_p_fading_c = is_p_fading.clone();
            Timeout::new(200, move || { p_data_c.set(None); is_p_fading_c.set(false); }).forget();
        })
    };

    // モーダルキー制御
    {
        let p_data = preview_modal_data.clone();
        let close_p = handle_close_preview.clone();
        use_effect_with((*p_data).clone(), move |preview| {
            if preview.is_none() { return Box::new(|| ()) as Box<dyn FnOnce()>; }
            let close_p_c = close_p.clone();
            let window = web_sys::window().unwrap();
            let mut opts = EventListenerOptions::run_in_capture_phase(); opts.passive = false;
            let listener = EventListener::new_with_options(&window, "keydown", opts, move |e| {
                let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                let key = ke.key(); let code = ke.code();
                if key == "Escape" || key == " " || code == "Space" { 
                    e.prevent_default(); e.stop_propagation(); e.stop_immediate_propagation(); 
                    close_p_c.emit(()); 
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

    let handle_close = {
        let on_close = props.on_close.clone();
        let is_fading_out_h = is_fading_out.clone();
        Callback::from(move |_: ()| {
            is_fading_out_h.set(true);
            let on_close_c = on_close.clone();
            Timeout::new(200, move || { on_close_c.emit(()); }).forget();
        })
    };

    let trigger_prefetch = {
        let files_reducer = files.clone();
        let abort_ctrl = abort_controller.clone();
        let cur_cid = current_category_id.clone();
        let fetching_ids = fetching_ids.clone();
        Callback::from(move |(start_idx, end_idx): (usize, usize)| {
            let current_list = &files_reducer.list;
            if current_list.is_empty() { return; }
            let end = std::cmp::min(end_idx, current_list.len());
            let signal = abort_ctrl.as_ref().map(|c| c.signal());
            let cid_at_start = (*cur_cid).clone();
            for i in start_idx..end {
                let file = &current_list[i];
                if !file.is_prefetched && !fetching_ids.borrow().contains(&file.id) {
                    let file_id = file.id.clone();
                    let file_size = file.total_size;
                    let reducer = files_reducer.clone();
                    let sig_inner = signal.clone();
                    let cid_inner = cid_at_start.clone();
                    let cur_cid_check = cur_cid.clone();
                    let fids_inner = fetching_ids.clone();
                    fetching_ids.borrow_mut().insert(file_id.clone());
                    spawn_local(async move {
                        let range = if file_size > 10240 { Some("0-10239") } else { None };
                        let res = download_file(&file_id, range, sig_inner.clone()).await;
                        fids_inner.borrow_mut().remove(&file_id);
                        if sig_inner.as_ref().map(|s| s.aborted()).unwrap_or(false) || *cur_cid_check != cid_inner { return; }
                        if let Ok(content_val) = res {
                            if !content_val.is_undefined() {
                                let safe = crate::js_interop::get_safe_chunk(&content_val);
                                let t = js_sys::Reflect::get(&safe, &JsValue::from_str("text")).unwrap().as_string().unwrap_or_default();
                                let b = js_sys::Reflect::get(&safe, &JsValue::from_str("bytes_consumed")).unwrap().as_f64().unwrap_or(0.0) as u64;
                                reducer.dispatch(FileAction::UpdateContent { id: file_id, content: t, loaded_bytes: b });
                            }
                        }
                    });
                }
            }
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
        let prefetch = trigger_prefetch.clone();
        let f_area_h = focused_area.clone();
        let on_nc_c = props.on_network_status_change.clone();
        Callback::from(move |(cat_id, cat_name, is_initial): (String, String, bool)| {
            if let Some(ctrl) = (*abort_ctrl_state).as_ref() { ctrl.abort(); }
            let new_ctrl = AbortController::new().unwrap();
            let signal = new_ctrl.signal();
            abort_ctrl_state.set(Some(new_ctrl.clone()));
            if let Some(window) = web_sys::window() { if let Ok(Some(storage)) = window.local_storage() { let _ = storage.set_item(STORAGE_KEY_LAST_CAT, &cat_id); } }
            files_reducer.dispatch(FileAction::Clear);
            fetching_ids.borrow_mut().clear();
            selected_file_idx.set(0);
            current_category_id.set(cat_id.clone());
            current_category_name.set(cat_name);
            on_loading_change.emit(true);
            let reducer_inner = files_reducer.clone();
            let sig_inner = signal.clone();
            let on_ld_inner = on_loading_change.clone();
            let cid_inner = cat_id.clone();
            let f_area_inner = f_area_h.clone();
            let is_fading_inner = is_fading_out_h.clone();
            let prefetch_inner = prefetch.clone();
            let on_nc_inner = on_nc_c.clone();
            spawn_local(async move {
                let res = list_files(&cid_inner, Some(sig_inner.clone())).await;
                if sig_inner.aborted() { return; }
                if let Ok(res_val) = res {
                    on_nc_inner.emit(true); // 成功したのでオンラインに
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
                            let lang_str = get_highlight_lang(&name).unwrap_or("").to_string();
                            all_metadata.push(FilePreview { id, name, content: "".to_string(), total_size, loaded_bytes: 0, is_markdown: ext == "md" || ext == "markdown", lang: lang_str, is_prefetched: false });
                        }
                        reducer_inner.dispatch(FileAction::SetFiles(all_metadata));
                        if is_initial && !*is_fading_inner { if !reducer_inner.list.is_empty() { f_area_inner.set(FocusedArea::Files); } else { f_area_inner.set(FocusedArea::Categories); } }
                        prefetch_inner.emit((0, 10));
                    }
                }
                on_ld_inner.emit(false);
            });
        })
    };

    {
        let list_len = files.list.len();
        let file_idx = *selected_file_idx;
        let prefetch = trigger_prefetch.clone();
        use_effect_with((list_len, file_idx), move |(len, idx)| {
            if *len > 0 { prefetch.emit((if *idx > 0 { *idx - 1 } else { 0 }, *idx + 9)); }
            || ()
        });
    }

    let on_file_scroll = {
        let prefetch = trigger_prefetch.clone();
        Callback::from(move |e: Event| {
            let el = e.target_unchecked_into::<web_sys::HtmlElement>();
            let scroll_top = el.scroll_top();
            let client_height = el.client_height();
            if client_height > 0 {
                let item_height = client_height as f64 / 8.0; 
                let first_visible = (scroll_top as f64 / item_height).floor() as usize;
                prefetch.emit((if first_visible > 0 { first_visible - 1 } else { 0 }, first_visible + 10));
            }
        })
    };

    {
        let cat_idx = *selected_cat_idx;
        let cat_ref = cat_list_ref.clone();
        use_effect_with(cat_idx, move |_| {
            if let Some(container) = cat_ref.cast::<web_sys::Element>() {
                if let Ok(Some(selected)) = container.query_selector("[data-selected='true']") {
                    let options = web_sys::ScrollIntoViewOptions::new();
                    options.set_block(web_sys::ScrollLogicalPosition::Nearest);
                    selected.scroll_into_view_with_scroll_into_view_options(&options);
                }
            }
            || ()
        });
    }
    {
        let file_idx = *selected_file_idx;
        let file_ref = file_list_ref.clone();
        use_effect_with(file_idx, move |_| {
            if let Some(container) = file_ref.cast::<web_sys::Element>() {
                if let Ok(Some(selected)) = container.query_selector("[data-selected='true']") {
                    let options = web_sys::ScrollIntoViewOptions::new();
                    options.set_block(web_sys::ScrollLogicalPosition::Nearest);
                    selected.scroll_into_view_with_scroll_into_view_options(&options);
                }
            }
            || ()
        });
    }

    {
        let p_data = preview_modal_data.clone();
        let on_toggle = props.on_preview_toggle.clone();
        use_effect_with((*p_data).clone(), move |preview| { on_toggle.emit(preview.is_some()); || () });
    }
    {
        let node_ref = preview_area_ref.clone();
        let list_len = files.list.len();
        let idx = *selected_file_idx;
        use_effect_with((list_len, idx), move |_| {
            if let Some(el) = node_ref.cast::<web_sys::Element>() { Timeout::new(100, move || { init_mermaid(&el); }).forget(); }
            || ()
        });
    }

    {
        let edit_ref = edit_input_ref.clone();
        let editing_id = editing_category_id.clone();
        use_effect_with((*editing_id).clone(), move |id| {
            if id.is_some() { Timeout::new(10, move || { if let Some(el) = edit_ref.cast::<web_sys::HtmlInputElement>() { let _ = el.focus(); let _ = el.select(); } }).forget(); }
            || ()
        });
    }
    {
        let root = root_ref.clone();
        let f_area = focused_area.clone();
        use_effect_with(root, move |r| {
            let mut _listener = None;
            if let Some(el) = r.get() {
                let f_area_c = f_area.clone();
                _listener = Some(EventListener::new(&el, "leaf-focus-recovery", move |_| { f_area_c.set(FocusedArea::Categories); }));
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
            if !is_loading && !files_reducer.list.is_empty() && !*is_fading_out_h {
                let file = &files_reducer.list[*selected_file_idx];
                let drive_id = file.id.clone(); let title = file.name.clone(); let cat_id = (*current_cat_id).clone();
                let on_select_inner = on_select.clone(); let on_start_inner = on_start.clone();
                is_fading_out_h.set(true); on_start_inner.emit(());
                Timeout::new(200, move || { on_select_inner.emit((drive_id, title, cat_id)); }).forget();
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
        let is_ld_prev_fade = is_preview_fading_out.clone();
        let is_loading_preview_cc = is_loading_preview.clone();
        let h_close_c = handle_close.clone();
        let is_sub_dialog_open = props.is_sub_dialog_open;
        let is_creating_cat = props.is_creating_category;
        let is_loading = props.is_loading;
        let is_loading_prev = *is_loading_preview;
        let has_pending_del = pending_delete_file.is_some();

        Callback::from(move |e: KeyboardEvent| {
            let current_focus = *focused_area_c;
            if preview_modal_c.is_some() || is_sub_dialog_open || is_creating_cat || is_loading || is_loading_prev || has_pending_del {
                let key = e.key();
                if key == "Tab" || key == "Enter" || key == " " || key.starts_with("Arrow") {
                    e.prevent_default();
                    e.stop_propagation();
                }
                return;
            }
            if *is_fading_out_cc || is_deleting_cc.is_some() { return; }
            let key = e.key(); let code = e.code(); let key_lower = key.to_lowercase();
            let is_m_shortcut = e.alt_key() && (code == "KeyM" || key_lower == "m" || key_lower == "µ");
            if is_m_shortcut { e.prevent_default(); e.stop_immediate_propagation(); h_close_c.emit(()); return; }
            match key.as_str() {
                " " => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Files && !files_reducer.list.is_empty() {
                        let file = &files_reducer.list[*selected_file_idx_c];
                        let file_id = file.id.clone(); let file_name = file.name.clone(); let total_size = file.total_size;
                        let p_modal = preview_modal_c.clone(); let is_ld_prev = is_loading_preview_cc.clone();
                        let is_md = file.is_markdown; let lang_c = file.lang.clone();
                        let is_fade = is_ld_prev_fade.clone();
                        is_ld_prev.set(true);
                        spawn_local(async move {
                            let range = if total_size > 102400 { Some("0-102399") } else { None };
                            if let Ok(cv) = download_file(&file_id, range, None).await {
                                let safe = crate::js_interop::get_safe_chunk(&cv);
                                let t = js_sys::Reflect::get(&safe, &JsValue::from_str("text")).unwrap().as_string().unwrap_or_default();
                                let b = js_sys::Reflect::get(&safe, &JsValue::from_str("bytes_consumed")).unwrap().as_f64().unwrap_or(0.0) as u64;
                                is_fade.set(false);
                                p_modal.set(Some(FilePreview { id: file_id, name: file_name, content: t, total_size, loaded_bytes: b, is_markdown: is_md, lang: lang_c, is_prefetched: true }));
                            }
                            is_ld_prev.set(false);
                        });
                    }
                }
                "Tab" => { e.prevent_default(); if current_focus == FocusedArea::Categories { focused_area_c.set(FocusedArea::Files); } else { focused_area_c.set(FocusedArea::Categories); } }
                "ArrowUp" => { e.prevent_default(); if current_focus == FocusedArea::Categories { if *selected_cat_idx_c > 0 { let new_idx = *selected_cat_idx_c - 1; selected_cat_idx_c.set(new_idx); load_files_cc.emit((categories_c[new_idx].id.clone(), categories_c[new_idx].name.clone(), false)); } } else if *selected_file_idx_c > 0 { selected_file_idx_c.set(*selected_file_idx_c - 1); } }
                "ArrowDown" => { e.prevent_default(); if current_focus == FocusedArea::Categories { if *selected_cat_idx_c + 1 < categories_c.len() { let new_idx = *selected_cat_idx_c + 1; selected_cat_idx_c.set(new_idx); load_files_cc.emit((categories_c[new_idx].id.clone(), categories_c[new_idx].name.clone(), false)); } } else if *selected_file_idx_c + 1 < files_reducer.list.len() { selected_file_idx_c.set(*selected_file_idx_c + 1); } }
                "Enter" => { e.prevent_default(); if current_focus == FocusedArea::Categories { focused_area_c.set(FocusedArea::Files); } else { on_ok_c.emit(()); } }
                _ => {}
            }
        })
    };

    let on_move_file = {
        let cur_cid_c = current_category_id.clone();
        let on_ld_c = props.on_loading_change.clone();
        let files_reducer = files.clone(); 
        let pending_move = pending_move_file_id.clone();
        Callback::from(move |(file_id, new_cat_id): (String, String)| {
            let old_cid = (*cur_cid_c).clone(); let on_ld = on_ld_c.clone();
            let reducer = files_reducer.clone(); let f_id = file_id.clone();
            let p_move = pending_move.clone();
            on_ld.emit(true); 
            spawn_local(async move {
                if let Ok(_) = move_file(&f_id, &old_cid, &new_cat_id).await {
                    on_ld.emit(false); p_move.set(Some(f_id.clone())); 
                    Timeout::new(200, move || { reducer.dispatch(FileAction::Remove(f_id.clone())); }).forget();
                } else { on_ld.emit(false); }
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
        let focused_area_c = focused_area.clone();
        let preview_active = preview_modal_data.is_some();
        let sub_active = props.is_sub_dialog_open || props.is_creating_category || (*pending_delete_file).is_some() || props.is_loading || *is_loading_preview;
        Callback::from(move |e: FocusEvent| {
            if preview_active || sub_active { return; } 
            let related = e.related_target();
            let outside = if let Some(target) = related { if let Some(root_el) = root_ref_c.cast::<web_sys::Node>() { !root_el.contains(Some(&target.unchecked_into::<web_sys::Node>())) } else { true } } else { true };
            if outside {
                let root_inner = root_ref_c.clone(); let f_area_inner = focused_area_c.clone();
                Timeout::new(10, move || { if let Some(div) = root_inner.cast::<web_sys::HtmlElement>() { let _ = div.focus(); f_area_inner.set(FocusedArea::Categories); } }).forget();
            }
        })
    };

    let current_preview_file = if files.list.is_empty() { None } else { Some(&files.list[*selected_file_idx]) };

    // --- HTMLパーツ ---
    let categories_html = {
        let idx = *selected_cat_idx;
        let area_active = *focused_area == FocusedArea::Categories && *is_root_focused;
        let editing_id = (*editing_category_id).clone();
        let categories = (*sorted_categories).clone();
        let load_files_cb = load_files.clone();
        let s_idx_state = selected_cat_idx.clone();
        let on_del_cb = props.on_delete_category.clone();
        let on_ren_cb = props.on_rename_category.clone();
        let eid_state = editing_category_id.clone();
        let ein_state = edit_name_input.clone();
        let edit_ref = edit_input_ref.clone();
        let is_wide = *is_wide_layout;

        html! {
            <div ref={cat_list_ref} class={classes!(
                "border-gray-700", "flex", "flex-col", "overflow-y-auto", "p-2", "bg-gray-900/30",
                if is_wide { vec!["w-[30%]", "border-r", "h-full"] } else { vec!["w-[50%]", "border-r"] }
            )}>
                <div class="flex space-x-1 mb-2 px-1">
                    <button onclick={let on_t = props.on_create_category_toggle.clone(); move |_| on_t.emit(true)} class="flex-1 p-2 rounded-[6px] bg-gray-700 hover:bg-gray-600 shadow-md transition-all text-white flex items-center justify-center space-x-1">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4"><path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m3.75 9v6m3-3H9m1.5-12H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" /></svg>
                        <span class="text-[10px] font-bold">{ i18n::t("new_category", lang) }</span>
                    </button>
                    <button onclick={let cb = props.on_refresh.clone(); move |_| cb.emit(())} class="p-2 rounded-[6px] bg-gray-700 hover:bg-gray-600 shadow-md transition-all text-white">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4"><path stroke-linecap="round" stroke-linejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99" /></svg>
                    </button>
                </div>
                <div class="flex-1 space-y-1 px-1">
                    { for categories.iter().enumerate().map(|(c_idx, cat)| {
                        let is_selected = idx == c_idx;
                        let is_focused = is_selected && area_active && editing_id.as_ref() != Some(&cat.id);
                        let is_no_cat = cat.name == "OTHERS";
                        let display_name = if is_no_cat { i18n::t("OTHERS", lang) } else { cat.name.clone() };
                        let load_files_f = load_files_cb.clone();
                        let cat_id_f = cat.id.clone();
                        let cat_name_f = cat.name.clone();
                        let s_idx_f = s_idx_state.clone();
                        let eid_f = eid_state.clone();
                        let ein_f = ein_state.clone();
                        let on_ren_f = on_ren_cb.clone();
                        let on_del_f = on_del_cb.clone();
                        let edit_ref_f = edit_ref.clone();

                        html! {
                            <div key={cat.id.clone()} data-selected={is_selected.to_string()} class={classes!("w-full", "rounded-[6px]", "transition-all", "flex", "items-center", "group/cat", "border-[3px]",
                                if is_focused { vec!["border-lime-400", "ring-1", "ring-lime-400"] } else { vec!["border-transparent"] },
                                if is_focused { vec!["bg-blue-600", "text-white"] } else if is_selected { vec!["bg-slate-600", "text-gray-200"] } else { vec!["bg-gray-700/50", "text-gray-400", "hover:bg-gray-700"] }
                            )} style="height: 48px; margin-bottom: 4px;">
                                if editing_id.as_ref() == Some(&cat.id) {
                                    <div class="flex-1 flex items-center px-2 space-x-1 h-full"><input ref={edit_ref_f} type="text" value={(*ein_f).clone()}
                                            oninput={let ein_inner = ein_f.clone(); Callback::from(move |e: InputEvent| { let input: web_sys::HtmlInputElement = e.target_unchecked_into(); ein_inner.set(input.value()); })}
                                            onkeydown={let eid_inner = eid_f.clone(); let ein_inner = ein_f.clone(); let on_ren_inner = on_ren_f.clone(); let id = cat_id_f.clone(); Callback::from(move |e: KeyboardEvent| { e.stop_propagation(); if e.key() == "Enter" && !e.is_composing() { let new_name = (*ein_inner).trim().to_string(); if !new_name.is_empty() { on_ren_inner.emit((id.clone(), new_name)); } eid_inner.set(None); } else if e.key() == "Escape" { eid_inner.set(None); } })}
                                            class="flex-1 bg-gray-900 border border-gray-600 rounded px-2 py-0.5 text-xs text-white outline-none focus:border-blue-500" /></div>
                                } else {
                                    <button onclick={let c_id = cat_id_f.clone(); let c_name = cat_name_f.clone(); move |_| { s_idx_f.set(c_idx); load_files_f.emit((c_id.clone(), c_name.clone(), false)); }} class="flex-1 text-left px-4 truncate h-full flex items-center outline-none"><span class="truncate text-xs">{ display_name }</span></button>
                                    if !is_no_cat {
                                        <div class="flex items-center opacity-0 group-hover/cat:opacity-100 transition-opacity pr-2">
                                            <button onclick={let id = cat_id_f.clone(); let name = cat_name_f.clone(); let eid_inner = eid_f.clone(); let ein_inner = ein_f.clone(); move |e: MouseEvent| { e.stop_propagation(); eid_inner.set(Some(id.clone())); ein_inner.set(name.clone()); }} class="p-1.5 text-gray-500 hover:text-blue-400 outline-none"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5"><path d="M5.433 13.917l1.262-3.155A4 4 0 017.58 9.42l6.92-6.918a2.121 2.121 0 013 3l-6.92 6.918c-.383.383-.84.685-1.343.886l-3.154 1.262a.5.5 0 01-.65-.65z" /><path d="M3.5 5.75c0-.69.56-1.25 1.25-1.25H10A.75.75 0 0010 3H4.75A2.75 2.75 0 002 5.75v9.5A2.75 2.75 0 004.75 18h9.5A2.75 2.75 0 0017 15.25V10a.75.75 0 00-1.5 0v5.25c0 .69-.56 1.25-1.25 1.25h-9.5c-.69 0-1.25-.56-1.25-1.25v-9.5z" /></svg></button>
                                            <button onclick={let id = cat_id_f.clone(); let on_del_inner = on_del_f.clone(); move |e: MouseEvent| { e.stop_propagation(); on_del_inner.emit(id.clone()); }} class="p-1.5 text-gray-500 hover:text-red-400 outline-none"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5"><path fill-rule="evenodd" d="M8.75 1A2.75 2.75 0 006 3.75v.443c-.795.077-1.584.176-2.365.298a.75.75 0 102.44 1.487l.263-.041.608 11.137A2.75 2.75 0 007.5 19h5a2.75 2.75 0 002.747-2.597l.608-11.137.263.041a.75.75 0 102.244-1.487A48.112 48.112 0 0014 4.193V3.75A2.75 2.75 0 0011.25 1h-2.5zM10 4c.84 0 1.673.025 2.5.075V3.75c0-.69-.56-1.25-1.25-1.25h-2.5c-.69 0-1.25.56-1.25 1.25v.325C8.327 4.025 9.16 4 10 4zM8.58 7.72a.75.75 0 00-1.5.06l.3 7.5a.75.75 0 101.498-.06l-.3-7.5zm4.34.06a.75.75 0 10-1.498-.06l-.3 7.5a.75.75 0 001.5.06l.3-7.5z" clip-rule="evenodd" /></svg></button>
                                        </div>
                                    }
                                }
                            </div>
                        }
                    }) }
                </div>
            </div>
        }
    };

    let sheets_html = {
        let list = files.list.clone();
        let s_idx = *selected_file_idx;
        let area_active = *focused_area == FocusedArea::Files && *is_root_focused;
        let drop_id = (*active_dropdown_file_id).clone();
        let pend_move_id = (*pending_move_file_id).clone();
        let is_del_id = (*is_deleting_id).clone();
        let s_idx_state = selected_file_idx.clone();
        let f_area_state = focused_area.clone();
        let ok_cb = on_ok_click.clone();
        let drop_state = active_dropdown_file_id.clone();
        let on_move_cb = on_move_file.clone();
        let cur_cid = (*current_category_id).clone();
        let pend_del_state = pending_delete_file.clone();
        let cats = (*sorted_categories).clone();
        let is_wide = *is_wide_layout;

        html! {
            <div class={classes!(
                "flex", "flex-col", "relative", "bg-gray-800/20", "overflow-hidden",
                if is_wide { vec!["w-[30%]", "border-r", "h-full"] } else { vec!["w-[50%]"] }
            )}>
                if *is_loading_preview { <div class="absolute inset-0 flex items-center justify-center bg-gray-800/30 z-40 backdrop-blur-[1px]"><div class="w-10 h-10 border-4 border-lime-500 border-t-transparent rounded-full animate-spin"></div></div> }
                <div ref={file_list_ref} onscroll={on_file_scroll} class="flex-1 flex flex-col h-full overflow-y-auto">
                    { for list.iter().enumerate().map(|(f_idx, file)| {
                        let is_selected = s_idx == f_idx;
                        let is_focused = is_selected && area_active;
                        let file_id = file.id.clone();
                        let file_name = file.name.clone();
                        let is_drop_open = drop_id.as_ref() == Some(&file_id);
                        let is_fading = pend_move_id.as_ref() == Some(&file_id);
                        let is_deleting = is_del_id.as_ref() == Some(&file_id);
                        let s_idx_f = s_idx_state.clone();
                        let f_area_f = f_area_state.clone();
                        let ok_f = ok_cb.clone();
                        let drop_f = drop_state.clone();
                        let move_f = on_move_cb.clone();
                        let pend_del_f = pend_del_state.clone();
                        let cur_cid_f = cur_cid.clone();
                        let cats_f = cats.clone();

                        html! {
                            <div key={file_id.clone()} data-selected={is_selected.to_string()} class={classes!("relative", "group/fileitem", "w-full", "transition-all", "duration-300", "px-1", "shrink-0",
                                if is_deleting { "h-0 opacity-0 overflow-hidden" } else { "h-[12.5%]" },
                                if is_fading { "opacity-0 scale-95" } else { "" },
                                if is_drop_open { "z-30" } else { "z-0" }
                            )}>
                                <button onclick={move |_| { s_idx_f.set(f_idx); f_area_f.set(FocusedArea::Files); }} ondblclick={move |_| ok_f.emit(())}
                                    class={classes!("w-full", "h-full", "text-left", "px-3", "py-1.5", "rounded-[6px]", "shadow-md", "transition-all", "overflow-hidden", "flex", "flex-col", "border-[3px]",
                                        if is_focused { vec!["border-lime-400", "ring-1", "ring-lime-400"] } else { vec!["border-transparent"] },
                                        if is_focused { vec!["bg-blue-600", "text-white"] } else if is_selected { vec!["bg-slate-600", "text-gray-200"] } else { vec!["bg-gray-700/50", "text-gray-400", "hover:bg-gray-700"] }
                                    )}>
                                    <div class="font-bold text-[9px] opacity-50 mb-0.5 truncate shrink-0">{ &file.name }</div>
                                    <div class="text-[9px] flex-1 whitespace-pre-wrap font-mono opacity-80 pr-8 leading-tight overflow-hidden text-ellipsis line-clamp-2">
                                        if file.is_prefetched { { &file.content } } else { { "Loading..." } }
                                    </div>
                                </button>
                                <div class="absolute top-1 right-2 flex flex-col space-y-0.5 z-20 opacity-0 group-hover/fileitem:opacity-100 transition-opacity">
                                    <div class="relative">
                                        <button onclick={let id = file_id.clone(); let drop_f2 = drop_f.clone(); move |e: MouseEvent| { e.stop_propagation(); if drop_f2.as_ref() == Some(&id) { drop_f2.set(None); } else { drop_f2.set(Some(id.clone())); } }} class="p-0.5 rounded bg-gray-600 hover:bg-gray-500 text-white border border-gray-500 transition-colors"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-3 h-3"><path d="M3 3h8v8H3V3zm0 10h8v8H3v-8zm10-10h8v8h-8V3zm0 10h8v8h-8v-8z" /></svg></button>
                                        if is_drop_open {
                                            <div ref={dropdown_ref.clone()} class="absolute right-0 top-full mt-1 w-48 bg-gray-800 border border-gray-700 rounded-md shadow-xl z-50 overflow-hidden py-1">
                                                { for cats_f.iter().map(|c| {
                                                    let move_f2 = move_f.clone(); let fid_f2 = file_id.clone(); let cat_id_f2 = c.id.clone();
                                                    let is_curr = cat_id_f2 == cur_cid_f;
                                                    let d_name = if c.name == "OTHERS" { i18n::t("OTHERS", lang) } else { c.name.clone() };
                                                    html! { <button onclick={if is_curr { Callback::from(|e: MouseEvent| e.stop_propagation()) } else { Callback::from(move |e: MouseEvent| { e.stop_propagation(); move_f2.emit((fid_f2.clone(), cat_id_f2.clone())); }) }} class={classes!("w-full", "text-left", "px-4", "py-2", "text-xs", if is_curr { "text-gray-600 cursor-default bg-gray-900/50" } else { "text-gray-300 hover:bg-blue-600 hover:text-white" })}>{ d_name }</button> }
                                                }) }
                                            </div>
                                        }
                                    </div>
                                    <button onclick={let id = file_id.clone(); let name = file_name.clone(); move |e: MouseEvent| { e.stop_propagation(); pend_del_f.set(Some((id.clone(), name.clone()))); }} class="p-0.5 rounded bg-gray-600 hover:bg-red-600 text-white border border-gray-500 transition-colors"><svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-3 h-3"><path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.112 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.112 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" /></svg></button>
                                </div>
                            </div>
                        }
                    }) }
                </div>
            </div>
        }
    };

    let preview_html = html! {
        <div class={classes!(
            "flex", "flex-col", "bg-[#0d1117]", "overflow-hidden",
            if *is_wide_layout { vec!["w-[40%]", "h-full"] } else { vec!["h-[30%]"] }
        )}>
            {
                if let Some(file) = current_preview_file {
                    let content = if file.is_markdown { file.content.clone() } else { format!("```{}\n{}\n```", file.lang, file.content) };
                    let rendered = render_markdown(&content);
                    html! {
                        <div class="flex-1 flex flex-col overflow-hidden">
                            <div class="px-4 py-1.5 border-b border-gray-800 bg-gray-900/50 flex justify-between items-center shrink-0">
                                <span class="text-xs font-bold text-gray-400 truncate mr-2">{ &file.name }</span>
                                <span class="text-[10px] text-gray-600 shrink-0 font-mono">{ format!("Preview ({}KB)", (file.loaded_bytes as f64 / 1024.0).round()) }</span>
                            </div>
                            <div ref={preview_area_ref} class="flex-1 overflow-y-auto p-4 markdown-body" style={format!("font-size: {}pt;", props.font_size)}>
                                if file.is_prefetched {
                                    { Html::from_html_unchecked(AttrValue::from(rendered)) }
                                    if file.loaded_bytes < file.total_size { <div class="mt-4 pb-4 text-center text-gray-500 font-mono text-[10px] opacity-60">{ i18n::t("omitted_below", lang) }</div> }
                                } else { <div class="flex items-center justify-center h-full"><div class="w-8 h-8 border-2 border-lime-500 border-t-transparent rounded-full animate-spin"></div></div> }
                            </div>
                        </div>
                    }
                } else { html! { <div class="flex-1 flex items-center justify-center text-gray-600 text-sm font-mono italic">{ "No file selected" }</div> } }
            }
        </div>
    };

    let is_modal_active = preview_modal_data.is_some() || (*pending_delete_file).is_some() || props.is_creating_category || is_sub_dialog_open || props.is_loading || *is_loading_preview;

    html! {
        <div class={classes!("fixed", "inset-0", "z-[100]", "flex", "items-center", "justify-center", "bg-black/60", "backdrop-blur-sm", "p-4", "outline-none",
                if *is_fading_out { "animate-backdrop-out" } else { "animate-backdrop-in" }
            )}>
            <div ref={root_ref} 
                tabindex={if is_modal_active { "-1" } else { "0" }}
                onkeydown={on_keydown}
                onfocusin={on_focus_in}
                onfocusout={on_focus_out}
                class={classes!("bg-gray-800", "border", "border-gray-700", "rounded-lg", "shadow-2xl", "overflow-hidden", "flex", "flex-col", "relative", "outline-none",
                    if is_modal_active { "pointer-events-none select-none" } else { "" },
                    if *is_fading_out { "animate-dialog-out" } else { "animate-dialog-in" }
                )} style="width: 80vw; height: 90vh;">
                <div class="px-6 py-2 border-b border-gray-700 bg-gray-900 flex justify-between items-center shrink-0">
                    <h3 class="text-lg font-bold text-white">{ i18n::t("file_selection", lang) }</h3>
                </div>
                if props.is_loading { <div class="absolute inset-0 flex items-center justify-center bg-gray-900/40 z-50 backdrop-blur-[2px]"><div class="w-12 h-12 border-4 border-lime-500 border-t-transparent rounded-full animate-spin shadow-lg"></div></div> }
                
                <div class="flex-1 flex flex-col overflow-hidden">
                    if *is_wide_layout {
                        <div class="flex-1 flex overflow-hidden">
                            { categories_html }
                            { sheets_html }
                            { preview_html }
                        </div>
                    } else {
                        <div class="flex-1 flex flex-col overflow-hidden">
                            <div class="h-[70%] flex border-b border-gray-700 overflow-hidden">
                                { categories_html }
                                { sheets_html }
                            </div>
                            { preview_html }
                        </div>
                    }
                </div>

                <div class="bg-gray-900 border-t border-gray-700 px-6 py-2 flex items-center justify-between shrink-0">
                    <div class="text-[10px] text-gray-500 font-medium">{ i18n::t("guide_keys", lang) }</div>
                    <div class="flex space-x-3">
                        <button onclick={on_ok_click.reform(|_| ())} class="px-8 py-2 bg-lime-600 hover:bg-lime-700 text-white font-bold rounded-[6px] shadow-lg transition-all">{ i18n::t("ok", lang) }</button>
                        <button onclick={handle_close.reform(|_| ())} class="px-6 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded-[6px] shadow-lg transition-all">{ i18n::t("cancel", lang) }</button>
                    </div>
                </div>
            </div>
            if props.is_creating_category {
                <InputDialog title={i18n::t("new_category", lang)} message={i18n::t("enter_category_name_message", lang)} 
                    on_confirm={
                        let on_t = props.on_create_category_toggle.clone(); let on_l = props.on_loading_change.clone();
                        let ldid = props.leaf_data_id.clone(); let on_r = props.on_refresh.clone();
                        Callback::from(move |name: String| {
                            let on_t_i = on_t.clone(); let on_l_i = on_l.clone(); let ldid_i = ldid.clone(); let on_r_i = on_r.clone();
                            if !name.trim().is_empty() { on_t_i.emit(false); on_l_i.emit(true); spawn_local(async move { if let Ok(_) = crate::drive_interop::create_folder(&name, &ldid_i).await { on_r_i.emit(()); } on_l_i.emit(false); }); } else { on_t_i.emit(false); }
                        })
                    }
                    on_cancel={let on_t = props.on_create_category_toggle.clone(); Callback::from(move |_| on_t.emit(false))}
                />
            }
            if let Some((_, _)) = (*pending_delete_file).clone() { <ConfirmDialog title={i18n::t("delete", lang)} message={i18n::t("confirm_delete_file", lang)} on_confirm={on_delete_file_confirm} on_cancel={let pending = pending_delete_file.clone(); move |_| pending.set(None)} /> }
            {
                if let Some(p) = (*preview_modal_data).clone() {
                    let content = if p.is_markdown { p.content.clone() } else { format!("```{}\n{}\n```", p.lang, p.content) };
                    let has_more = p.loaded_bytes < p.total_size;
                    let on_change_fs = props.on_change_font_size.clone();
                    let is_fade = *is_preview_fading_out;
                    let is_sub_active = is_sub_dialog_open || props.is_creating_category || (*pending_delete_file).is_some() || props.is_loading || *is_loading_preview;
                    html! { <Preview content={content} on_close={handle_close_preview} has_more={has_more} disable_space_scroll={true} is_sub_dialog_open={is_sub_active} font_size={props.font_size} on_change_font_size={on_change_fs} is_fading_out={is_fade} /> }
                } else { html! { <></> } }
            }
        </div>
    }
}
