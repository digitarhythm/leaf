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
use crate::components::dialog::LoadingOverlay;
use web_sys::AbortController;
use std::collections::{HashMap, HashSet};
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
    modified_time: f64, // 最終更新日時(ミリ秒)
    created_time: f64,  // 作成日時(ミリ秒)
}

// 並べ替えの基準
#[derive(Clone, Copy, PartialEq)]
enum SortKey { Modified, Created }

// アイコンの表示方法: Grid=3x3グリッド, List=1シート1行で縦に並べる
#[derive(Clone, Copy, PartialEq)]
enum ViewMode { Grid, List }

// ファイル一覧を指定キー(更新日/作成日)・方向(desc=降順)で並べ替える。
// 同値時はファイル名で安定させる。
fn sort_file_list(list: &mut [FilePreview], key: SortKey, desc: bool) {
    list.sort_by(|a, b| {
        let (va, vb) = match key {
            SortKey::Modified => (a.modified_time, b.modified_time),
            SortKey::Created => (a.created_time, b.created_time),
        };
        let ord = va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        if desc { ord.reverse() } else { ord }
    });
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
    pub on_move_file: Callback<(String, String)>, // (drive_id, new_category_id)
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
    pub on_sub_active_change: Callback<bool>, 
    pub font_size: i32,
    pub on_change_font_size: Callback<i32>,
    pub is_processing: bool,
    #[prop_or(0)]
    pub close_trigger: u32,
    #[prop_or_default]
    pub active_category_id: String,
    #[prop_or_default]
    pub active_drive_id: Option<String>,
}

#[derive(PartialEq, Clone, Copy)]
enum FocusedArea { Categories, Files }

const STORAGE_KEY_LAST_CAT: &str = "leaf_last_category";
// 表示形式・ソート方法の保存キー
const STORAGE_KEY_VIEW: &str = "leaf_file_view_mode";
const STORAGE_KEY_SORT: &str = "leaf_file_sort_key";
const STORAGE_KEY_MOD_DESC: &str = "leaf_file_sort_mod_desc";
const STORAGE_KEY_CRE_DESC: &str = "leaf_file_sort_cre_desc";

fn ls_get(key: &str) -> Option<String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(key).ok().flatten())
}
fn ls_set(key: &str, val: &str) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(key, val);
    }
}

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
    let dropdown_pos = use_state(|| (0.0_f64, 0.0_f64)); // ドロップダウンの固定位置 (right from viewport right, top)
    let preview_modal_data = use_state(|| None::<FilePreview>);
    let is_preview_fading_out = use_state(|| false); 
    let is_loading_preview = use_state(|| false); 
    let is_deleting_id = use_state(|| None::<String>);
    let abort_controller = use_state(|| None::<AbortController>);
    let fetching_ids = use_mut_ref(|| HashSet::<String>::new());
    let files_cache = use_mut_ref(|| HashMap::<String, Vec<FilePreview>>::new());
    let pending_delete_file = use_state(|| None::<(String, String)>);
    let pending_move_file_id = use_state(|| None::<String>);
    let processing_move_id = use_state(|| None::<String>);
    // スワイプ削除用ステート
    let swipe_file_id = use_state(|| None::<String>); // スワイプ中のファイルID
    let swipe_offset = use_state(|| 0.0_f64); // 現在のスワイプオフセット(px)
    let swipe_start_x = use_mut_ref(|| 0.0_f64); // タッチ開始X座標
    let swipe_start_y = use_mut_ref(|| 0.0_f64); // タッチ開始Y座標
    let swipe_is_horizontal = use_mut_ref(|| None::<bool>); // スワイプ方向が水平か判定済みか
    let swipe_is_dragging = use_state(|| false); // ドラッグ中フラグ（transition制御用）
    // カテゴリースワイプ用ステート
    let cat_swipe_id = use_state(|| None::<String>);
    let cat_swipe_offset = use_state(|| 0.0_f64);
    let cat_swipe_start_x = use_mut_ref(|| 0.0_f64);
    let cat_swipe_start_y = use_mut_ref(|| 0.0_f64);
    let cat_swipe_is_horizontal = use_mut_ref(|| None::<bool>);
    let cat_swipe_is_dragging = use_state(|| false);
    // 表示方法ステート(localStorageから復元、既定: グリッド)
    let view_mode = use_state(|| match ls_get(STORAGE_KEY_VIEW).as_deref() {
        Some("list") => ViewMode::List,
        _ => ViewMode::Grid,
    });
    // 並べ替えステート(localStorageから復元、既定: 更新日の降順)
    let sort_active = use_state(|| match ls_get(STORAGE_KEY_SORT).as_deref() {
        Some("created") => SortKey::Created,
        _ => SortKey::Modified,
    });
    let modified_desc = use_state(|| ls_get(STORAGE_KEY_MOD_DESC).as_deref() != Some("0")); // 既定 true=降順↓
    let created_desc = use_state(|| ls_get(STORAGE_KEY_CRE_DESC).as_deref() != Some("0"));
    // 表示形式・ソート方法の変更を localStorage へ保存
    {
        let v = *view_mode;
        use_effect_with(v, move |m| { ls_set(STORAGE_KEY_VIEW, match m { ViewMode::Grid => "grid", ViewMode::List => "list" }); || () });
    }
    {
        let s = *sort_active;
        use_effect_with(s, move |k| { ls_set(STORAGE_KEY_SORT, match k { SortKey::Modified => "modified", SortKey::Created => "created" }); || () });
    }
    {
        let d = *modified_desc;
        use_effect_with(d, move |desc| { ls_set(STORAGE_KEY_MOD_DESC, if *desc { "1" } else { "0" }); || () });
    }
    {
        let d = *created_desc;
        use_effect_with(d, move |desc| { ls_set(STORAGE_KEY_CRE_DESC, if *desc { "1" } else { "0" }); || () });
    }
    // 横画面カテゴリータブ用ステート
    let cat_edit = use_state(|| None::<(String, String)>); // カテゴリー名編集ダイアログ (id, 現在名)
    let cat_edit_input = use_state(|| String::new());
    let cat_edit_input_ref = use_node_ref();
    let root_ref = use_node_ref();
    let dropdown_ref = use_node_ref();
    let edit_input_ref = use_node_ref();
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
                    let win_h = window_c.inner_height().unwrap().as_f64().unwrap_or(0.0);
                    let screen = window_c.screen().unwrap();
                    let scr_w = screen.width().unwrap() as f64;
                    let scr_h = screen.height().unwrap() as f64;

                    let device_is_portrait = scr_w < scr_h;
                    let window_is_portrait = win_w < win_h;
                    let is_narrow_window = win_w <= (scr_w / 2.0);

                    let is_portrait = device_is_portrait || (window_is_portrait && is_narrow_window);
                    
                    is_wide_c.set(!is_portrait);
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

    // サブダイアログ状態の親への通知
    {
        let p_modal = preview_modal_data.clone();
        let p_del = pending_delete_file.clone();
        let on_sub_change = props.on_sub_active_change.clone();
        use_effect_with((p_modal, p_del), move |(m, d)| {
            on_sub_change.emit(m.is_some() || d.is_some());
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

    // カテゴリー選択時の1KB先読み処理（一行目表示用）
    let trigger_prefetch_1kb = {
        let files_reducer = files.clone();
        let fetching_ids = fetching_ids.clone();
        Callback::from(move |(file_info, signal): ((String, u64), web_sys::AbortSignal)| {
            let (file_id, total_size) = file_info;
            if fetching_ids.borrow().contains(&file_id) { return; }

            fetching_ids.borrow_mut().insert(file_id.clone());
            let reducer = files_reducer.clone();
            let ids = fetching_ids.clone();
            spawn_local(async move {
                let range = if total_size > 1024 { Some("0-1023") } else { None };
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
            let delay = 200;
            Timeout::new(delay, move || { on_close_inner.emit(()); }).forget();
        })
    };

    // Alt+M などの外部トリガーによるclose（初回マウント時はスキップ）
    {
        let handle_close_trigger = handle_close.clone();
        let close_trigger = props.close_trigger;
        let is_first_render = use_mut_ref(|| true);
        use_effect_with(close_trigger, move |_trigger| {
            if *is_first_render.borrow() {
                *is_first_render.borrow_mut() = false;
            } else {
                handle_close_trigger.emit(());
            }
            || ()
        });
    }

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
        let active_drive_id_outer = props.active_drive_id.clone();
        let files_cache = files_cache.clone();
        let sort_active_lf = sort_active.clone();
        let modified_desc_lf = modified_desc.clone();
        let created_desc_lf = created_desc.clone();
        Callback::from(move |(cat_id, cat_name, is_initial): (String, String, bool)| {
            let sort_key_now = *sort_active_lf;
            let sort_desc_now = match sort_key_now { SortKey::Modified => *modified_desc_lf, SortKey::Created => *created_desc_lf };
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

            // キャッシュにある場合はAPIコールをスキップ
            if let Some(cached) = files_cache.borrow().get(&cat_id) {
                let mut cached_list = cached.clone();
                sort_file_list(&mut cached_list, sort_key_now, sort_desc_now);
                if let Some(ref target_id) = active_drive_id_outer {
                    if let Some(idx) = cached_list.iter().position(|f| f.id == *target_id) {
                        selected_file_idx.set(Some(idx));
                    }
                } else if !cached_list.is_empty() {
                    selected_file_idx.set(Some(0));
                }
                files_reducer.dispatch(FileAction::Set(cached_list));
                if is_initial && !*is_fading_out_h { f_area_h.set(FocusedArea::Categories); }
                return;
            }

            on_loading_change.emit(true);

            let reducer_inner = files_reducer.clone();
            let sig_inner = signal.clone();
            let on_ld_inner = on_loading_change.clone();
            let cid_inner = cat_id.clone();
            let f_area_inner = f_area_h.clone();
            let is_fading_inner = is_fading_out_h.clone();
            let on_nc_inner = on_nc_c.clone();
            let active_drive_id_inner = active_drive_id_outer.clone();
            let selected_file_idx_inner = selected_file_idx.clone();
            let cache_inner = files_cache.clone();

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
                            let modified_time = js_sys::Reflect::get(&v, &JsValue::from_str("modifiedTime")).ok()
                                .and_then(|t| t.as_string())
                                .map(|s| crate::drive_interop::parse_date(&s)).unwrap_or(0.0);
                            let created_time = js_sys::Reflect::get(&v, &JsValue::from_str("createdTime")).ok()
                                .and_then(|t| t.as_string())
                                .map(|s| crate::drive_interop::parse_date(&s)).unwrap_or(0.0);
                            let ext = name.split('.').last().unwrap_or("").to_lowercase();
                            all_metadata.push(FilePreview { id, name, content: "".to_string(), total_size, loaded_bytes: 0, is_markdown: ext == "md" || ext == "markdown", lang: ext, is_loaded: false, modified_time, created_time });
                        }
                        sort_file_list(&mut all_metadata, sort_key_now, sort_desc_now);
                        // 空リストの場合はここでキャッシュに保存
                        if all_metadata.is_empty() {
                            cache_inner.borrow_mut().insert(cid_inner.clone(), Vec::new());
                        }
                        // active_drive_idに一致するファイルを自動選択、未保存時は先頭ファイルを選択
                        if let Some(ref target_id) = active_drive_id_inner {
                            if let Some(idx) = all_metadata.iter().position(|f| f.id == *target_id) {
                                selected_file_idx_inner.set(Some(idx));
                            }
                        } else if !all_metadata.is_empty() {
                            selected_file_idx_inner.set(Some(0));
                        }
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

    // 縦カテゴリータブ用: カテゴリーを選択してファイルを読み込む
    let select_category = {
        let sorted_cats = sorted_categories.clone();
        let s_idx = selected_cat_idx.clone();
        let load_files_sc = load_files.clone();
        Callback::from(move |new_idx: usize| {
            let total = sorted_cats.len();
            if new_idx >= total { return; }
            s_idx.set(new_idx);
            load_files_sc.emit((sorted_cats[new_idx].id.clone(), sorted_cats[new_idx].name.clone(), false));
        })
    };

    // ファイルリストが更新されたらバックグラウンド読み込みを開始 + キャッシュ更新
    // Categories フォーカス時は1KBプリフェッチ、Files フォーカス時は10KBプリフェッチ
    {
        let list = files.list.clone();
        let prefetch = trigger_prefetch.clone();
        let prefetch_1kb = trigger_prefetch_1kb.clone();
        let abort_ctrl = abort_controller.clone();
        let cache_ref = files_cache.clone();
        let current_cid = current_category_id.clone();
        let focus_val = *focused_area;
        use_effect_with((list, focus_val), move |(list, focus)| {
            let pf = if *focus == FocusedArea::Categories { &prefetch_1kb } else { &prefetch };
            if let Some(ctrl) = (*abort_ctrl).as_ref() {
                let signal = ctrl.signal();
                for file in list.iter() {
                    if !file.is_loaded {
                        pf.emit(((file.id.clone(), file.total_size), signal.clone()));
                    }
                }
            }
            // 全ファイルの読み込みが完了したらキャッシュに保存
            if !list.is_empty() && list.iter().all(|f| f.is_loaded) {
                let cid = (*current_cid).clone();
                if !cid.is_empty() {
                    cache_ref.borrow_mut().insert(cid, list.to_vec());
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
    // カテゴリー名編集ダイアログ表示時に入力欄へフォーカス
    {
        let edit_ref = cat_edit_input_ref.clone();
        let is_open = cat_edit.is_some();
        use_effect_with(is_open, move |open| {
            if *open { Timeout::new(20, move || { if let Some(el) = edit_ref.cast::<web_sys::HtmlInputElement>() { let _ = el.focus(); let _ = el.select(); } }).forget(); }
            || ()
        });
    }
    // 縦カテゴリータブ: 選択中タブを表示領域内へスクロール
    {
        let sel_idx = *selected_cat_idx;
        use_effect_with(sel_idx, move |idx| {
            let id = format!("cat-vtab-{}", idx);
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                if let Some(el) = doc.get_element_by_id(&id) {
                    el.scroll_into_view_with_bool(false);
                }
            }
            || ()
        });
    }
    // カテゴリー名編集ダイアログ表示中: Escapeでキャンセル(フォーカス位置に依らずwindowで捕捉)
    {
        let cat_edit_esc = cat_edit.clone();
        use_effect_with(cat_edit.is_some(), move |open| {
            let mut _listener: Option<EventListener> = None;
            if *open {
                let h = cat_edit_esc.clone();
                let window = web_sys::window().unwrap();
                let mut opts = EventListenerOptions::run_in_capture_phase();
                opts.passive = false;
                _listener = Some(EventListener::new_with_options(&window, "keydown", opts, move |e| {
                    let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                    if ke.key() == "Escape" {
                        e.prevent_default();
                        e.stop_immediate_propagation();
                        h.set(None);
                    }
                }));
            }
            move || { drop(_listener); }
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

    // ファイル選択インデックス変更時の自動スクロール
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

    // カテゴリー選択インデックス変更時の自動スクロール
    {
        let cat_list_ref_c = cat_list_ref.clone();
        let selected_idx = *selected_cat_idx;
        use_effect_with(selected_idx, move |idx| {
            if let Some(container) = cat_list_ref_c.cast::<web_sys::Element>() {
                crate::js_interop::scroll_into_view_graceful(&container, (*idx as u32) + 1, 200.0);
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
        let active_cat_id = props.active_category_id.clone();
        let active_drive_id_for_cat = props.active_drive_id.clone();
        let focused_area_c = focused_area.clone();
        let cache_ref = files_cache.clone();
        use_effect_with((refresh_trigger, cats_len), move |_| {
            // 外部リフレッシュ時はキャッシュをクリア
            cache_ref.borrow_mut().clear();
            if !sorted_cats.is_empty() {
                let cid = (*current_cid_c).clone();
                let target_idx = sorted_cats.iter().position(|c| c.id == cid).unwrap_or_else(|| {
                    if active_drive_id_for_cat.is_some() && !active_cat_id.is_empty() && active_cat_id != "__LOCAL__" {
                        if let Some(idx) = sorted_cats.iter().position(|c| c.id == active_cat_id) {
                            focused_area_c.set(FocusedArea::Files);
                            return idx;
                        }
                    }
                    let others_idx = sorted_cats.iter().position(|c| c.name == "OTHERS").unwrap_or(0);
                    if active_drive_id_for_cat.is_none() {
                        focused_area_c.set(FocusedArea::Files);
                    }
                    others_idx
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
                if !is_loading && !*is_fading_out_h {
                    if let Some(file) = files_reducer.list.get(idx) {
                    let drive_id = file.id.clone(); let title = file.name.clone(); let cat_id = (*current_cat_id).clone();
                    let on_select_inner = on_select.clone(); let on_start_inner = on_start.clone();
                    is_fading_out_h.set(true); on_start_inner.emit(());
                    let delay = 200;
                    Timeout::new(delay, move || { on_select_inner.emit((drive_id, title, cat_id)); }).forget();
                    }
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
        let h_close_preview_c = handle_close_preview.clone();
        let on_create_toggle_c = props.on_create_category_toggle.clone();
        let root_ref_for_esc = root_ref.clone();
        let on_prev_toggle_c = props.on_preview_toggle.clone();
        let is_wide_k = is_wide_layout.clone();
        let is_sub_dialog_open = props.is_sub_dialog_open;
        let is_creating_cat = props.is_creating_category;
        let is_loading_prev_val = *is_loading_preview;
        let has_pending_del = pending_delete_file.clone(); // ステートとしてキャプチャ
        let handle_close_c = handle_close.clone();
        let editing_cat_for_key = editing_category_id.clone();
        let select_category_k = select_category.clone();
        let cat_edit_k = cat_edit.clone();
        let selected_cat_for_key = selected_cat_idx.clone();
        let cats_len_for_key = sorted_categories.len();

        Callback::from(move |e: KeyboardEvent| {
            // カテゴリー名編集中／編集ダイアログ表示中はダイアログ側のキー処理をスキップ
            if (*editing_cat_for_key).is_some() || (*cat_edit_k).is_some() { return; }

            let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
            let key = ke.key();

            // Alt + カーソル上下: カテゴリー切替(タブが縦並びのため)
            if ke.alt_key() && (key == "ArrowUp" || key == "ArrowDown") {
                e.prevent_default();
                e.stop_immediate_propagation();
                let cur = *selected_cat_for_key;
                if key == "ArrowUp" {
                    if cur > 0 { select_category_k.emit(cur - 1); }
                } else if cur + 1 < cats_len_for_key {
                    select_category_k.emit(cur + 1);
                }
                return;
            }

            // サブダイアログ表示中の Escape キー処理
            if key == "Escape" {
                let p_modal = preview_modal_c.clone();
                let p_del = has_pending_del.clone();
                let is_creating = is_creating_cat;
                
                let mut handled = false;
                if p_modal.is_some() {
                    h_close_preview_c.emit(());
                    handled = true;
                } else if (*p_del).is_some() {
                    p_del.set(None);
                    handled = true;
                } else if is_creating {
                    on_create_toggle_c.emit(false);
                    handled = true;
                }
                
                if handled {
                    e.prevent_default();
                    e.stop_immediate_propagation();
                    
                    let rr = root_ref_for_esc.clone();
                    Timeout::new(10, move || {
                        if let Some(el) = rr.cast::<web_sys::HtmlElement>() { let _ = el.focus(); }
                    }).forget();
                    return;
                }
            }

            let current_focus = *focused_area_c;
            if preview_modal_c.is_some() || is_sub_dialog_open || is_creating_cat || is_loading_prev_val || (*has_pending_del).is_some() {
                return;
            }
            if *is_fading_out_cc || is_deleting_cc.is_some() { return; }
            let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
            let key = ke.key();
            // 横画面アイコングリッドの実際の列数を DOM レイアウトから測定する。
            // 1行目のアイコン(先頭要素と同じ画面Y座標を持つ要素)の個数 = 列数。
            // getBoundingClientRect().top はビューポート基準で offsetParent に依らないため、
            // グリッドでもリスト(各行が relative)でも正しく測定できる(リストは列数1になる)。
            let is_wide = *is_wide_k;
            let grid_cols = {
                let mut cols = 1usize;
                if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                    if let Ok(list) = doc.query_selector_all(".leaf-file-icon") {
                        let n = list.length();
                        if n > 0 {
                            let first_top = list.item(0)
                                .and_then(|el| el.dyn_into::<web_sys::Element>().ok())
                                .map(|el| el.get_bounding_client_rect().top());
                            if let Some(top0) = first_top {
                                let mut c = 0usize;
                                for i in 0..n {
                                    if let Some(el) = list.item(i).and_then(|el| el.dyn_into::<web_sys::Element>().ok()) {
                                        // 同一行判定(サブピクセル誤差を許容)
                                        if (el.get_bounding_client_rect().top() - top0).abs() < 1.0 { c += 1; } else { break; }
                                    }
                                }
                                if c > 0 { cols = c; }
                            }
                        }
                    }
                }
                cols
            };
            match key.as_str() {
                " " => {
                    e.prevent_default();
                    if let Some(idx) = *selected_file_idx_c {
                        if current_focus == FocusedArea::Files {
                            if let Some(file) = files_reducer.list.get(idx) {
                            let file = file.clone();
                            let file_id = file.id.clone(); let file_name = file.name.clone(); let total_size = file.total_size;
                            let p_modal = preview_modal_c.clone(); let is_ld_prev = is_loading_preview_cc.clone();
                            let is_md = file.is_markdown; let lang_c = file.lang.clone();
                            let mtime = file.modified_time; let ctime = file.created_time;
                            let is_fade = is_preview_fading_out_c.clone();

                            if file.is_loaded {
                                is_fade.set(false);
                                on_prev_toggle_c.emit(true);
                                p_modal.set(Some(FilePreview { id: file_id, name: file_name, content: file.content.clone(), total_size, loaded_bytes: file.loaded_bytes, is_markdown: is_md, lang: lang_c, is_loaded: true, modified_time: mtime, created_time: ctime }));
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
                                        p_modal.set(Some(FilePreview { id: file_id, name: file_name, content: t, total_size, loaded_bytes: b, is_markdown: is_md, lang: lang_c, is_loaded: true, modified_time: mtime, created_time: ctime }));
                                    }
                                    is_ld_prev.set(false);
                                });
                            }
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
                    if is_wide {
                        // アイコングリッド: 1行上へ
                        let len = files_reducer.list.len();
                        if len > 0 {
                            let cur = selected_file_idx_c.unwrap_or(0);
                            let new = if cur >= grid_cols { cur - grid_cols } else { cur };
                            selected_file_idx_c.set(Some(new));
                        }
                    } else if current_focus == FocusedArea::Categories {
                        if *selected_cat_idx_c > 0 { let new_idx = *selected_cat_idx_c - 1; selected_cat_idx_c.set(new_idx); load_files_cc.emit((categories_c[new_idx].id.clone(), categories_c[new_idx].name.clone(), false)); }
                    } else {
                        let cur_idx = selected_file_idx_c.unwrap_or(0);
                        if cur_idx > 0 { selected_file_idx_c.set(Some(cur_idx - 1)); }
                        else if selected_file_idx_c.is_none() && !files_reducer.list.is_empty() { selected_file_idx_c.set(Some(0)); }
                    }
                }
                "ArrowDown" => {
                    e.prevent_default();
                    if is_wide {
                        // アイコングリッド: 1行下へ。
                        // 真下にアイコンがあればそれを選択。真下が無くても次の行が存在する場合は
                        // その行の一番右(=最後のアイコン)を選択。最終行なら移動しない。
                        let len = files_reducer.list.len();
                        if len > 0 {
                            let cur = selected_file_idx_c.unwrap_or(0);
                            let below = cur + grid_cols;
                            let new = if below < len {
                                below
                            } else {
                                let cur_row = cur / grid_cols;
                                let last_row = (len - 1) / grid_cols;
                                if cur_row < last_row { len - 1 } else { cur }
                            };
                            selected_file_idx_c.set(Some(new));
                        }
                    } else if current_focus == FocusedArea::Categories {
                        if *selected_cat_idx_c + 1 < categories_c.len() { let new_idx = *selected_cat_idx_c + 1; selected_cat_idx_c.set(new_idx); load_files_cc.emit((categories_c[new_idx].id.clone(), categories_c[new_idx].name.clone(), false)); }
                    } else {
                        let cur_idx = selected_file_idx_c.unwrap_or(0);
                        if selected_file_idx_c.is_none() && !files_reducer.list.is_empty() { selected_file_idx_c.set(Some(0)); }
                        else if cur_idx + 1 < files_reducer.list.len() { selected_file_idx_c.set(Some(cur_idx + 1)); }
                    }
                }
                "ArrowLeft" => {
                    // 横画面アイコングリッド: 1つ前へ(Alt併用は上部で処理済み)
                    if is_wide {
                        e.prevent_default();
                        let len = files_reducer.list.len();
                        if len > 0 {
                            let cur = selected_file_idx_c.unwrap_or(0);
                            if cur > 0 { selected_file_idx_c.set(Some(cur - 1)); } else { selected_file_idx_c.set(Some(0)); }
                        }
                    }
                }
                "ArrowRight" => {
                    // 横画面アイコングリッド: 1つ次へ(Alt併用は上部で処理済み)
                    if is_wide {
                        e.prevent_default();
                        let len = files_reducer.list.len();
                        if len > 0 {
                            let cur = selected_file_idx_c.unwrap_or(0);
                            if cur + 1 < len { selected_file_idx_c.set(Some(cur + 1)); }
                        }
                    }
                }
                "Enter" => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Categories {
                        focused_area_c.set(FocusedArea::Files);
                        if selected_file_idx_c.is_none() && !files_reducer.list.is_empty() { selected_file_idx_c.set(Some(0)); }
                    } else {
                        on_ok_c.emit(());
                    }
                }
                "Escape" => {
                    e.prevent_default();
                    if current_focus == FocusedArea::Files {
                        focused_area_c.set(FocusedArea::Categories);
                    } else {
                        handle_close_c.emit(());
                    }
                }
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
        let on_p_move_parent = props.on_move_file.clone(); // 外側でクローン
        let cache_ref = files_cache.clone();
        Callback::from(move |(file_id, new_cat_id): (String, String)| {
            ads_state.set(None);
            let old_cid = (*cur_cid_c).clone();
            let reducer = files_reducer.clone(); let f_id = file_id.clone();
            let p_move = pending_move.clone();
            let proc_m = proc_move.clone();
            let on_p_move = on_p_move_parent.clone(); // クロージャ内で使用
            let n_cat_id = new_cat_id.clone();
            let cache = cache_ref.clone();

            proc_m.set(Some(f_id.clone()));
            spawn_local(async move {
                if let Ok(_) = move_file(&f_id, &old_cid, &n_cat_id).await {
                    // 移動元・移動先のキャッシュを無効化
                    cache.borrow_mut().remove(&old_cid);
                    cache.borrow_mut().remove(&n_cat_id);
                    proc_m.set(None);
                    p_move.set(Some(f_id.clone()));
                    on_p_move.emit((f_id.clone(), n_cat_id));
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
        let root_ref_for_del = root_ref.clone();
        let cache_ref = files_cache.clone();
        let current_cid_for_del = current_category_id.clone();
        Callback::from(move |_: ()| {
            if let Some((id, name)) = (*pending_delete_c).clone() {
                let id_for_anim = id.clone(); let id_for_parent = id.clone(); let name_for_parent = name.clone();
                let reducer = files_reducer.clone(); let on_del = on_parent_delete_c.clone();
                let is_del = is_del_id_c.clone();
                let root_ref_inner = root_ref_for_del.clone();
                
                pending_delete_c.set(None); is_del.set(Some(id_for_anim));
                // 削除対象カテゴリーのキャッシュを無効化
                cache_ref.borrow_mut().remove(&*current_cid_for_del);

                // フォーカス復帰処理
                Timeout::new(50, move || {
                    if let Some(root) = root_ref_inner.cast::<web_sys::HtmlElement>() {
                        let _ = root.focus();
                    }
                }).forget();

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
        let focused_area_h = focused_area.clone();
        let cs_id = cat_swipe_id.clone();
        let cs_off = cat_swipe_offset.clone();
        let cs_sx = cat_swipe_start_x.clone();
        let cs_sy = cat_swipe_start_y.clone();
        let cs_horiz = cat_swipe_is_horizontal.clone();
        let cs_dragging = cat_swipe_is_dragging.clone();

        html! {
            <div class={classes!("flex", "flex-col", "h-full", "w-full", "border-r", "border-white/5", "bg-gray-900/50")}>
                <div
                    class="p-4 border-b border-white/5 flex items-center justify-between bg-gray-950/20"
                >
                    <div class="flex items-center space-x-2">
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-emerald-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg>
                        <h2 class="text-sm font-bold text-gray-200 tracking-tight truncate">{ i18n::t("select_category", lang) }</h2>
                    </div>
                </div>
                <div ref={cat_list_ref.clone()} class="flex-1 overflow-y-auto custom-scrollbar p-2 space-y-1">
                    <button
                        onclick={let ic = props.on_create_category_toggle.clone(); move |_| ic.emit(true)}
                        class="mb-2 w-full flex items-center justify-center space-x-2 px-3 py-2.5 rounded-md text-sm font-bold text-emerald-400 bg-emerald-500/10 hover:bg-emerald-500/20 border border-emerald-500/20 transition-all tracking-widest"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                        </svg>
                        <span>{ i18n::t("new_category", lang) }</span>
                    </button>
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
                        let is_others = cat.name == "OTHERS";

                        // カテゴリースワイプ用
                        let this_cat_offset = if cs_id.as_ref() == Some(&cat.id) { *cs_off } else { 0.0 };
                        let this_cat_dragging = cs_id.as_ref() == Some(&cat.id) && *cs_dragging;
                        let cs_id_ts = cs_id.clone();
                        let cs_off_ts = cs_off.clone();
                        let cs_sx_ts = cs_sx.clone();
                        let cs_sy_ts = cs_sy.clone();
                        let cs_horiz_ts = cs_horiz.clone();
                        let cs_dragging_ts = cs_dragging.clone();
                        let cat_fid = cat.id.clone();

                        let cat_on_touch_start = {
                            let fid = cat_fid.clone();
                            let cs_id = cs_id_ts.clone();
                            let cs_off = cs_off_ts.clone();
                            let sx = cs_sx_ts.clone();
                            let sy = cs_sy_ts.clone();
                            let sh = cs_horiz_ts.clone();
                            let is_others = is_others;
                            Callback::from(move |e: TouchEvent| {
                                if is_others { return; }
                                let te: web_sys::TouchEvent = JsCast::unchecked_into(web_sys::Event::from(e.clone()));
                                if let Some(touch) = te.touches().get(0) {
                                    *sx.borrow_mut() = touch.client_x() as f64;
                                    *sy.borrow_mut() = touch.client_y() as f64;
                                    *sh.borrow_mut() = None;
                                    if cs_id.as_ref() != Some(&fid) {
                                        cs_id.set(Some(fid.clone()));
                                        cs_off.set(0.0);
                                    }
                                }
                            })
                        };
                        let cat_on_touch_move = {
                            let cs_id = cs_id_ts.clone();
                            let cs_off = cs_off_ts.clone();
                            let sx = cs_sx_ts.clone();
                            let sy = cs_sy_ts.clone();
                            let sh = cs_horiz_ts.clone();
                            let sd = cs_dragging_ts.clone();
                            let fid = cat_fid.clone();
                            let is_others = is_others;
                            Callback::from(move |e: TouchEvent| {
                                if is_others { return; }
                                if cs_id.as_ref() != Some(&fid) { return; }
                                let te: web_sys::TouchEvent = JsCast::unchecked_into(web_sys::Event::from(e.clone()));
                                if let Some(touch) = te.touches().get(0) {
                                    let dx = touch.client_x() as f64 - *sx.borrow();
                                    let dy = touch.client_y() as f64 - *sy.borrow();
                                    let is_h = *sh.borrow();
                                    if is_h.is_none() {
                                        if dx.abs() > 8.0 || dy.abs() > 8.0 {
                                            let horizontal = dx.abs() > dy.abs();
                                            *sh.borrow_mut() = Some(horizontal);
                                            if !horizontal { return; }
                                        } else {
                                            return;
                                        }
                                    } else if !is_h.unwrap_or(false) {
                                        return;
                                    }
                                    e.prevent_default();
                                    sd.set(true);
                                    // 左右両方向OK
                                    cs_off.set(dx);
                                }
                            })
                        };
                        let cat_on_touch_end = {
                            let cs_id = cs_id_ts.clone();
                            let cs_off = cs_off_ts.clone();
                            let sd = cs_dragging_ts.clone();
                            let fid = cat_fid.clone();
                            let on_del = on_del_inner.clone();
                            let cid_del = cid_for_delete.clone();
                            let eid_ren = eid_inner.clone();
                            let ein_ren = ein_inner.clone();
                            let cid_ren = cid_for_rename.clone();
                            let cn_ren = cat.name.clone();
                            let cl_ref = cat_list_ref.clone();
                            Callback::from(move |_: TouchEvent| {
                                sd.set(false);
                                if cs_id.as_ref() != Some(&fid) { return; }
                                let offset = *cs_off;
                                let item_width = cl_ref.cast::<web_sys::Element>()
                                    .map(|el| el.client_width() as f64)
                                    .unwrap_or(300.0);
                                let threshold = item_width / 3.0;
                                if offset < -threshold {
                                    // 左スワイプ → 削除
                                    on_del.emit(cid_del.clone());
                                    cs_id.set(None);
                                    cs_off.set(0.0);
                                } else if offset > threshold {
                                    // 右スワイプ → 名前変更
                                    eid_ren.set(Some(cid_ren.clone()));
                                    ein_ren.set(cn_ren.clone());
                                    cs_id.set(None);
                                    cs_off.set(0.0);
                                } else {
                                    // 不足 → アニメーションで戻す
                                    cs_off.set(0.0);
                                    cs_id.set(None);
                                }
                            })
                        };

                        html! {
                            <div class={classes!("relative", "overflow-hidden", "rounded-md")}>
                                // 左スワイプ: 赤い背景＋ゴミ箱アイコン
                                if this_cat_offset < 0.0 {
                                    <div class="absolute inset-0 bg-red-900/40 flex items-center justify-end pr-6 z-0">
                                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-5 h-5 text-red-400">
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
                                        </svg>
                                    </div>
                                }
                                // 右スワイプ: 青い背景＋鉛筆アイコン
                                if this_cat_offset > 0.0 {
                                    <div class="absolute inset-0 bg-blue-900/40 flex items-center justify-start pl-6 z-0">
                                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-5 h-5 text-blue-400">
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10" />
                                        </svg>
                                    </div>
                                }
                            <div
                                class={classes!(
                                    "group", "relative", "flex", "items-center", "px-3", "py-2", "rounded-md", "cursor-pointer",
                                    if is_sel { vec!["bg-emerald-600/20", "text-emerald-400"] } else { vec!["text-gray-400", "hover:bg-white/5", "hover:text-gray-200"] },
                                    if is_active { vec!["ring-2", "ring-emerald-500/50", "bg-emerald-600/30"] } else { vec![] }
                                )}
                                style={if this_cat_offset != 0.0 {
                                    if this_cat_dragging {
                                        format!("transform: translateX({}px);", this_cat_offset)
                                    } else {
                                        format!("transform: translateX({}px); transition: transform 0.2s ease-out;", this_cat_offset)
                                    }
                                } else if !this_cat_dragging {
                                    "transition: transform 0.2s ease-out;".to_string()
                                } else {
                                    String::new()
                                }}
                                onclick={let f_area = focused_area_h.clone(); let is_editing_this = is_editing; let eid_click = eid_inner.clone(); move |_| { if is_editing_this { return; } eid_click.set(None); s_idx_inner.set(i); f_area.set(FocusedArea::Categories); load_inner.emit((cid_val.clone(), cname_val.clone(), false)); }}
                                ontouchstart={cat_on_touch_start}
                                ontouchmove={cat_on_touch_move}
                                ontouchend={cat_on_touch_end}
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" class={classes!("h-4", "w-4", "mr-3", if is_sel { "text-emerald-500" } else { "text-gray-600" })} fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                                </svg>
                                if is_editing {
                                    <input
                                        ref={edit_ref.clone()} type="text" value={(*ein_inner).clone()}
                                        oninput={let ein = ein_inner.clone(); move |e: InputEvent| { let input: web_sys::HtmlInputElement = e.target_unchecked_into(); ein.set(input.value()); }}
                                        onblur={let eid = eid_inner.clone(); move |_| eid.set(None)}
                                        onkeydown={let eid = eid_inner.clone(); let ein = ein_inner.clone(); let cid_inner_val = cid_for_rename.clone(); let on_ren = on_ren.clone(); let rr = root_ref.clone(); move |e: KeyboardEvent| { if e.is_composing() { return; } if e.key() == "Enter" { let new_name = (*ein).clone(); if !new_name.trim().is_empty() { on_ren.emit((cid_inner_val.clone(), new_name)); } eid.set(None); e.prevent_default(); e.stop_propagation(); } else if e.key() == "Escape" { e.prevent_default(); e.stop_immediate_propagation(); eid.set(None); let rr = rr.clone(); Timeout::new(10, move || { if let Some(el) = rr.cast::<web_sys::HtmlElement>() { let _ = el.focus(); } }).forget(); } }}
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
                            </div> // スワイプwrapper閉じ
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
        let dropdown_pos_state = dropdown_pos.clone();
        let is_ld_id = (*is_deleting_id).clone();
        let p_move_id = (*pending_move_file_id).clone();
        let proc_move_id = (*processing_move_id).clone();
        let p_del_state = pending_delete_file.clone();
        let focused_area_h = focused_area.clone();
        let swipe_fid = swipe_file_id.clone();
        let swipe_off = swipe_offset.clone();
        let swipe_sx = swipe_start_x.clone();
        let swipe_sy = swipe_start_y.clone();
        let swipe_horiz = swipe_is_horizontal.clone();
        let swipe_dragging = swipe_is_dragging.clone();

        html! {
            <div class={classes!("flex", "flex-col", "bg-gray-900", "min-w-0", "h-full", "w-full")}>
                <div class="p-3 border-b border-white/5 flex items-center justify-between bg-gray-950/20 flex-shrink-0">
                    <div class="flex items-center space-x-2">
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-emerald-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" /></svg>
                        <h2 class="text-sm font-bold text-gray-200 tracking-tight truncate">{ format!("{} ({})", if *current_category_name == "OTHERS" { i18n::t("OTHERS", lang) } else { (*current_category_name).clone() }, file_list.len()) }</h2>
                    </div>
                </div>
                <div ref={file_list_ref.clone()} class="flex-1 overflow-y-auto custom-scrollbar flex flex-col p-2">
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

                            // スワイプ削除用
                            let this_swipe_offset = if swipe_fid.as_ref() == Some(&file.id) { *swipe_off } else { 0.0 };
                            let this_is_dragging = swipe_fid.as_ref() == Some(&file.id) && *swipe_dragging;
                            let swipe_fid_ts = swipe_fid.clone();
                            let swipe_off_ts = swipe_off.clone();
                            let swipe_sx_ts = swipe_sx.clone();
                            let swipe_sy_ts = swipe_sy.clone();
                            let swipe_horiz_ts = swipe_horiz.clone();
                            let swipe_dragging_ts = swipe_dragging.clone();
                            let fid_for_ts = file.id.clone();
                            let on_touch_start = {
                                let fid = fid_for_ts.clone();
                                let swipe_fid = swipe_fid_ts.clone();
                                let swipe_off = swipe_off_ts.clone();
                                let sx = swipe_sx_ts.clone();
                                let sy = swipe_sy_ts.clone();
                                let sh = swipe_horiz_ts.clone();
                                Callback::from(move |e: TouchEvent| {
                                    let te: web_sys::TouchEvent = JsCast::unchecked_into(web_sys::Event::from(e.clone()));
                                    if let Some(touch) = te.touches().get(0) {
                                        *sx.borrow_mut() = touch.client_x() as f64;
                                        *sy.borrow_mut() = touch.client_y() as f64;
                                        *sh.borrow_mut() = None;
                                        // 前回と違うファイルの場合、リセット
                                        if swipe_fid.as_ref() != Some(&fid) {
                                            swipe_fid.set(Some(fid.clone()));
                                            swipe_off.set(0.0);
                                        }
                                    }
                                })
                            };
                            let on_touch_move = {
                                let swipe_fid = swipe_fid_ts.clone();
                                let swipe_off = swipe_off_ts.clone();
                                let sx = swipe_sx_ts.clone();
                                let sy = swipe_sy_ts.clone();
                                let sh = swipe_horiz_ts.clone();
                                let sd = swipe_dragging_ts.clone();
                                let fid = fid_for_ts.clone();
                                Callback::from(move |e: TouchEvent| {
                                    if swipe_fid.as_ref() != Some(&fid) { return; }
                                    let te: web_sys::TouchEvent = JsCast::unchecked_into(web_sys::Event::from(e.clone()));
                                    if let Some(touch) = te.touches().get(0) {
                                        let dx = touch.client_x() as f64 - *sx.borrow();
                                        let dy = touch.client_y() as f64 - *sy.borrow();
                                        let is_h = *sh.borrow();
                                        if is_h.is_none() {
                                            if dx.abs() > 8.0 || dy.abs() > 8.0 {
                                                let horizontal = dx.abs() > dy.abs();
                                                *sh.borrow_mut() = Some(horizontal);
                                                if !horizontal { return; }
                                            } else {
                                                return;
                                            }
                                        } else if !is_h.unwrap_or(false) {
                                            return;
                                        }
                                        e.prevent_default();
                                        sd.set(true);
                                        // 左方向のみ（負の値）
                                        let offset = dx.min(0.0);
                                        swipe_off.set(offset);
                                    }
                                })
                            };
                            let on_touch_end = {
                                let swipe_fid = swipe_fid_ts.clone();
                                let swipe_off = swipe_off_ts.clone();
                                let sd = swipe_dragging_ts.clone();
                                let fid = fid_for_ts.clone();
                                let fname = file.name.clone();
                                let p_del = p_del_inner.clone();
                                let fl_ref = file_list_ref.clone();
                                Callback::from(move |_: TouchEvent| {
                                    sd.set(false);
                                    if swipe_fid.as_ref() != Some(&fid) { return; }
                                    let offset = *swipe_off;
                                    // アイテム幅をスクロールコンテナから取得
                                    let item_width = fl_ref.cast::<web_sys::Element>()
                                        .map(|el| el.client_width() as f64)
                                        .unwrap_or(300.0);
                                    let threshold = item_width / 3.0;
                                    if offset.abs() >= threshold {
                                        // 3分の1以上スワイプ → 削除確認ダイアログ
                                        p_del.set(Some((fid.clone(), fname.clone())));
                                        swipe_fid.set(None);
                                        swipe_off.set(0.0);
                                    } else {
                                        // 半分以下 → アニメーションで元に戻す
                                        swipe_off.set(0.0);
                                        swipe_fid.set(None);
                                    }
                                })
                            };

                            html! {
                                <div class={classes!(
                                    "relative", "flex-shrink-0", "mx-1", "mb-1", "overflow-hidden", "rounded",
                                    if is_deleting || is_moving { "opacity-0 scale-95 translate-x-4 transition-all duration-200" } else { "" }
                                )}>
                                    // 赤い背景＋ゴミ箱アイコン
                                    if this_swipe_offset < 0.0 {
                                        <div class="absolute inset-0 bg-red-900/40 flex items-center justify-end pr-6 z-0">
                                            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-5 h-5 text-red-400">
                                                <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
                                            </svg>
                                        </div>
                                    }
                                <div
                                    class={classes!(
                                        "group", "relative", "flex", "flex-col", "p-0", "rounded", "cursor-pointer", "border", "min-h-[8rem]", "overflow-visible",
                                        if this_swipe_offset == 0.0 { "transition-all duration-200" } else { "" },
                                        if is_dropdown_open { vec!["z-50", "bg-gray-800/90", "border-emerald-500", "shadow-2xl"] }
                                        else if is_active { vec!["bg-emerald-600", "text-white", "shadow-lg", "z-10", "border-white", "ring-4", "ring-emerald-500/30", "scale-[1.01]"] }
                                        else if is_sel { vec!["bg-emerald-600/10", "text-emerald-400/80", "border-emerald-500/30", "z-0"] }
                                        else { vec!["text-gray-400", "hover:bg-white/5", "border-white/40", "z-0"] },
                                        if is_deleting || is_moving { vec!["opacity-0", "scale-95", "translate-x-4"] } else { vec!["opacity-100", "scale-100"] }
                                    )}
                                    style={if this_swipe_offset != 0.0 {
                                        if this_is_dragging {
                                            format!("transform: translateX({}px);", this_swipe_offset)
                                        } else {
                                            format!("transform: translateX({}px); transition: transform 0.2s ease-out;", this_swipe_offset)
                                        }
                                    } else if !this_is_dragging {
                                        "transition: transform 0.2s ease-out;".to_string()
                                    } else {
                                        String::new()
                                    }}
                                    onclick={let f_area = focused_area_h.clone(); move |_| { s_idx_inner.set(Some(i)); f_area.set(FocusedArea::Files); }}
                                    ondblclick={move |_| on_ok_inner.emit(())}
                                    ontouchstart={on_touch_start}
                                    ontouchmove={on_touch_move}
                                    ontouchend={on_touch_end}
                                >
                                    <div class="flex flex-col w-full h-full">
                                        // ファイル名表示エリア（高さを半分に、はみ出しを許可、右端フェード）
                                        <div class="px-3 h-4 flex items-center justify-between w-full flex-shrink-0 relative overflow-visible mt-1.5 mb-1">
                                            <div class="flex items-center space-x-2 overflow-hidden pr-14 w-full">
                                                <svg xmlns="http://www.w3.org/2000/svg" class={classes!("h-2.5", "w-2.5", "flex-shrink-0", if is_sel { "text-white" } else { "text-gray-600" })} fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
                                                </svg>
                                                <span class={classes!("px-1", "py-0.5", "rounded", "text-[8px]", "font-black", "uppercase", "tracking-tighter", "flex-shrink-0", if is_sel { "bg-white/20 text-white" } else { "bg-emerald-500/10 text-emerald-400/80" })}>{ &file.lang }</span>
                                            </div>
                                            <div class={classes!(
                                                "flex", "items-center", "space-x-0.5", "absolute", "right-2", "top-[-4px]", "overflow-visible", "transition-opacity", "duration-200",
                                                if is_dropdown_open { "opacity-100" } else { "opacity-0 group-hover:opacity-100" }
                                            )}>
                                                <div class="relative">
                                                    <button
                                                        onclick={let ads = ads_inner.clone(); let fid = file_id_inner.clone(); let dp = dropdown_pos_state.clone(); move |e: MouseEvent| {
                                                            e.stop_propagation();
                                                            if is_dropdown_open {
                                                                ads.set(None);
                                                            } else {
                                                                // ボタンの位置を取得してドロップダウン位置を計算
                                                                let btn = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                                                                    .and_then(|el| el.closest("button").ok().flatten());
                                                                if let Some(el) = btn {
                                                                    let rect = el.get_bounding_client_rect();
                                                                    let top = rect.bottom() + 4.0;
                                                                    let right = rect.right();
                                                                    dp.set((right, top));
                                                                }
                                                                ads.set(Some(fid.clone()));
                                                            }
                                                        }}
                                                        class={classes!("p-1", "rounded-md", "hover:bg-black/20", "transition-colors", if is_sel { "text-white" } else { "text-gray-500" })}
                                                        title="Change Category"
                                                    >
                                                        <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg>
                                                    </button>
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
                                        // コンテンツプレビュー（冒頭5行を表示）
                                        <div class={classes!(
                                            "px-3", "pb-2", "text-xs", "font-bold", "line-clamp-5", "leading-snug", "break-all", "overflow-hidden", "whitespace-pre-wrap",
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
                                </div> // スワイプwrapper閉じ
                            }
                        }) }
                    }
                </div>
            </div>
        }
    };

    // 横画面用: 右半分に表示する選択シートのプレビュー（プリロード済データを使用）
    let side_preview_html = {
        let selected_file = (*selected_file_idx).and_then(|i| files.list.get(i).cloned());
        html! {
            <div class="flex flex-col bg-gray-900 min-w-0 h-full w-full">
                <div class="p-3 border-b border-white/5 flex items-center justify-between bg-gray-950/20 flex-shrink-0">
                    <div class="flex items-center space-x-2 overflow-hidden">
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-emerald-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                        </svg>
                        <h2 class="text-sm font-bold text-gray-200 tracking-tight truncate">
                            { if let Some(ref f) = selected_file { f.name.clone() } else { i18n::t("preview", lang) } }
                        </h2>
                    </div>
                </div>
                if let Some(file) = selected_file {
                    <Preview
                        key={file.id.clone()}
                        content={file.content.clone()}
                        lang={file.lang.clone()}
                        on_close={Callback::noop()}
                        is_embedded={true}
                        is_loading={!file.is_loaded}
                        has_more={file.is_loaded && file.loaded_bytes < file.total_size}
                    />
                } else {
                    <div class="flex-1 flex items-center justify-center text-gray-600">
                        <p class="text-xs uppercase tracking-widest font-bold opacity-40">{ i18n::t("preview", lang) }</p>
                    </div>
                }
            </div>
        }
    };

    // 横画面用: カテゴリータブバー(幅=画面幅/10、translateXでスクロール、peekスクロール対応)
    let category_tabs_html = {
        let cats = (*sorted_categories).clone();
        let sel = *selected_cat_idx;
        let select_cb = select_category.clone();
        let cat_edit_h = cat_edit.clone();
        let cat_edit_input_h = cat_edit_input.clone();
        let on_create_toggle = props.on_create_category_toggle.clone();
        let on_del_cat = props.on_delete_category.clone();
        html! {
            // 左端に縦並び: 幅 = 画面幅/10
            <div class="flex flex-col flex-shrink-0 h-full bg-gray-950/40 border-r-2 border-emerald-500/40 overflow-hidden" style="width: 10vw;">
                // 最上部: 新規カテゴリー作成ボタン
                <button
                    onclick={move |_| on_create_toggle.emit(true)}
                    title={i18n::t("new_category", lang)}
                    class="flex-shrink-0 w-full h-11 flex items-center justify-center gap-1 border-b border-white/10 bg-gray-950/60 text-emerald-400 hover:bg-emerald-500/20 transition-colors"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" /></svg>
                </button>
                // カテゴリータブ(縦並び・スクロール)
                <div class="flex-1 overflow-y-auto custom-scrollbar">
                    { for cats.iter().enumerate().map(|(i, cat)| {
                        let is_sel = i == sel;
                        let is_others = cat.name == "OTHERS";
                        let disp = if is_others { i18n::t("OTHERS", lang) } else { cat.name.clone() };
                        let select_cb = select_cb.clone();
                        let cat_edit_hh = cat_edit_h.clone();
                        let cat_edit_input_hh = cat_edit_input_h.clone();
                        let on_del_cat_inner = on_del_cat.clone();
                        let cid = cat.id.clone();
                        let cid_del = cat.id.clone();
                        let cname = cat.name.clone();
                        html! {
                            <div
                                id={format!("cat-vtab-{}", i)}
                                class={classes!(
                                    "group","w-full","flex","items-center","gap-1.5","px-3","py-3",
                                    "cursor-pointer","border-b","border-white/5","select-none",
                                    "transition-colors","overflow-hidden",
                                    if is_sel { vec!["bg-emerald-600","text-white","font-bold"] }
                                    else { vec!["text-gray-400","hover:bg-white/5","hover:text-gray-200"] }
                                )}
                                onclick={move |_| select_cb.emit(i)}
                                ondblclick={move |_| {
                                    if !is_others {
                                        cat_edit_input_hh.set(cname.clone());
                                        cat_edit_hh.set(Some((cid.clone(), cname.clone())));
                                    }
                                }}
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" class={classes!("h-4","w-4","flex-shrink-0", if is_sel { "text-white" } else { "text-gray-600" })} fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg>
                                <span class="flex-1 min-w-0 truncate text-xs">{ disp }</span>
                                if !is_others {
                                    <button
                                        onclick={let on_del = on_del_cat_inner.clone(); move |e: MouseEvent| { e.stop_propagation(); on_del.emit(cid_del.clone()); }}
                                        class="ml-auto hidden group-hover:flex flex-shrink-0 p-0.5 rounded hover:bg-red-600/70 text-gray-400 hover:text-white"
                                        title="Delete category"
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                                    </button>
                                }
                            </div>
                        }
                    }) }
                </div>
            </div>
        }
    };

    // ソートボタン押下:
    //  - 別のボタンへ切り替えた時は、そのボタンの現在の昇順/降順をそのまま適用(反転しない)
    //  - 既に適用中のボタンを再度押した時は、昇順/降順を反転して適用
    // 選択はファイルidで維持。
    let on_sort = {
        let sort_active = sort_active.clone();
        let modified_desc = modified_desc.clone();
        let created_desc = created_desc.clone();
        let files_reducer = files.clone();
        let selected_file_idx = selected_file_idx.clone();
        Callback::from(move |key: SortKey| {
            let already_active = *sort_active == key;
            let new_desc = match key {
                SortKey::Modified => { let d = if already_active { !*modified_desc } else { *modified_desc }; modified_desc.set(d); d }
                SortKey::Created => { let d = if already_active { !*created_desc } else { *created_desc }; created_desc.set(d); d }
            };
            sort_active.set(key);
            // 現在の選択ファイルidを覚えてから並べ替え、並べ替え後に同idの位置へ選択を移す
            let sel_id = (*selected_file_idx).and_then(|i| files_reducer.list.get(i).map(|f| f.id.clone()));
            let mut list = files_reducer.list.clone();
            sort_file_list(&mut list, key, new_desc);
            if let Some(id) = sel_id {
                if let Some(pos) = list.iter().position(|f| f.id == id) { selected_file_idx.set(Some(pos)); }
            }
            files_reducer.dispatch(FileAction::Set(list));
        })
    };

    // 横画面用: ファイルをアイコン表示(拡張子を大きく+先頭行先頭10文字をファイル名風に)
    let files_icon_grid_html = {
        let file_list = files.list.clone();
        let sel = *selected_file_idx;
        let s_idx_state = selected_file_idx.clone();
        let f_area_h = focused_area.clone();
        let on_ok = on_ok_click.clone();
        let p_del_state = pending_delete_file.clone();
        let ads_state = active_dropdown_file_id.clone();
        let dp_state = dropdown_pos.clone();
        let cat_name = (*current_category_name).clone();
        let is_loading = props.is_loading;
        // Tauri(デスクトップ)版ではファイル名ラベルの文字が小さく見えるため一回り大きくする
        let label_size_class = if crate::js_interop::is_tauri() { "text-xs" } else { "text-[9px]" };
        // ソートボタン用
        let active_key = *sort_active;
        let mod_desc = *modified_desc;
        let cre_desc = *created_desc;
        let on_sort_m = on_sort.clone();
        let on_sort_c = on_sort.clone();
        let sort_btn_class = |is_active: bool| -> Vec<&'static str> {
            if is_active { vec!["bg-emerald-600","text-white","border-emerald-500"] }
            else { vec!["bg-gray-800","text-gray-400","border-white/10","hover:bg-gray-700"] }
        };
        // 表示方法ボタン用
        let mode = *view_mode;
        let vm_grid = view_mode.clone();
        let vm_list = view_mode.clone();
        let view_btn_class = |is_active: bool| -> Vec<&'static str> {
            if is_active { vec!["bg-emerald-600","text-white","border-emerald-500"] }
            else { vec!["bg-gray-800","text-gray-400","border-white/10","hover:bg-gray-700"] }
        };
        // 1ファイル分のアイコンカード(拡張子の四角+ファイル名ラベル)。グリッド/リスト両モードで共用。
        // show_label=false でアイコン下のファイル名ラベルを非表示(リスト表示は右に本文が出るため不要)。
        // show_actions=false でアイコン内の移動/削除ボタンを非表示(リスト表示は行右上に出すため)。
        let make_card = |i: usize, file: &FilePreview, show_label: bool, show_actions: bool| -> Html {
            let is_sel = sel == Some(i);
            let ext_disp = if file.lang.is_empty() { "—".to_string() } else { file.lang.to_uppercase() };
            let label = {
                let first = file.content.lines().next().map(|l| l.trim().to_string()).filter(|l| !l.is_empty());
                match first {
                    Some(l) => l.chars().take(30).collect::<String>(),
                    None => file.name.chars().take(30).collect::<String>(),
                }
            };
            let s_idx_inner = s_idx_state.clone();
            let f_area_inner = f_area_h.clone();
            let on_ok_inner = on_ok.clone();
            let p_del_inner = p_del_state.clone();
            let ads_inner = ads_state.clone();
            let dp_inner = dp_state.clone();
            let fid = file.id.clone();
            let fname = file.name.clone();
            let is_loaded = file.is_loaded;
            html! {
                <div
                    class="leaf-file-icon group relative w-32 flex flex-col items-center cursor-pointer"
                    onclick={move |_| { s_idx_inner.set(Some(i)); f_area_inner.set(FocusedArea::Files); }}
                    ondblclick={move |_| on_ok_inner.emit(())}
                >
                    <div class={classes!(
                        "relative","w-24","h-28","rounded-lg","flex","items-center","justify-center","border-2","transition-all",
                        if is_sel { vec!["bg-emerald-600","border-white","shadow-lg","ring-4","ring-emerald-500/30"] }
                        else { vec!["bg-gray-800","border-white/20","group-hover:border-emerald-500/60"] }
                    )}>
                        <span class={classes!("text-2xl","font-black","uppercase","tracking-tighter", if is_sel { "text-white" } else { "text-emerald-400/90" })}>{ ext_disp }</span>
                        if !is_loaded {
                            <div class="absolute bottom-1 right-1 w-3 h-3 border-2 border-emerald-500/30 border-t-emerald-500 rounded-full animate-spin"></div>
                        }
                        if show_actions {
                            // 移動ボタン: 常時、左上に表示
                            <button
                                onclick={let ads = ads_inner.clone(); let fid_m = fid.clone(); let dp = dp_inner.clone(); move |e: MouseEvent| {
                                    e.stop_propagation();
                                    let btn = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).and_then(|el| el.closest("button").ok().flatten());
                                    if let Some(el) = btn { let rect = el.get_bounding_client_rect(); dp.set((rect.right(), rect.bottom() + 4.0)); }
                                    ads.set(Some(fid_m.clone()));
                                }}
                                class="absolute top-1 left-1 flex p-0.5 rounded bg-black/40 hover:bg-emerald-600 text-white"
                                title="Move"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg>
                            </button>
                            // 削除ボタン(ゴミ箱): 常時、右上に表示
                            <button
                                onclick={let p_del = p_del_inner.clone(); let fid_d = fid.clone(); let fname_d = fname.clone(); move |e: MouseEvent| { e.stop_propagation(); p_del.set(Some((fid_d.clone(), fname_d.clone()))); }}
                                class="absolute top-1 right-1 p-0.5 rounded bg-black/50 hover:bg-red-600 text-white shadow"
                                title="Delete"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                            </button>
                        }
                    </div>
                    if show_label {
                        <span class={classes!("mt-2",label_size_class,"leading-tight","text-center","max-w-full","line-clamp-2","break-all","px-2","py-0.5","rounded", if is_sel { vec!["bg-emerald-600","text-white","font-bold"] } else { vec!["text-gray-300"] })}>{ label }</span>
                    }
                </div>
            }
        };
        // リストモード用: アイコン高さに収まるだけ表示できるよう多めに先頭行を取り出す(CSSで高さ制限)
        let first_lines = |file: &FilePreview| -> String {
            file.content.lines().take(12).collect::<Vec<_>>().join("\n")
        };
        html! {
            <div class="flex flex-col h-full w-full bg-gray-900">
                <div class="p-3 border-b border-white/5 flex items-center gap-2 bg-gray-950/20 flex-shrink-0">
                    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-emerald-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" /></svg>
                    <h2 class="text-sm font-bold text-gray-200 tracking-tight truncate">{ format!("{} ({})", if cat_name == "OTHERS" { i18n::t("OTHERS", lang) } else { cat_name.clone() }, file_list.len()) }</h2>
                    // 表示方法ボタン(グリッド / リスト) + ソートボタン — ヘッダ右端
                    <div class="ml-auto flex items-center gap-1.5 flex-shrink-0">
                        // グリッド表示(3x3)
                        <button
                            onclick={move |_| vm_grid.set(ViewMode::Grid)}
                            title="Grid"
                            class={classes!("flex","items-center","justify-center","p-1.5","rounded","border","transition-colors", view_btn_class(mode == ViewMode::Grid))}
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="currentColor" viewBox="0 0 24 24"><rect x="3" y="3" width="5" height="5" rx="1"/><rect x="9.5" y="3" width="5" height="5" rx="1"/><rect x="16" y="3" width="5" height="5" rx="1"/><rect x="3" y="9.5" width="5" height="5" rx="1"/><rect x="9.5" y="9.5" width="5" height="5" rx="1"/><rect x="16" y="9.5" width="5" height="5" rx="1"/><rect x="3" y="16" width="5" height="5" rx="1"/><rect x="9.5" y="16" width="5" height="5" rx="1"/><rect x="16" y="16" width="5" height="5" rx="1"/></svg>
                        </button>
                        // リスト表示(四角+横棒が縦に3つ)
                        <button
                            onclick={move |_| vm_list.set(ViewMode::List)}
                            title="List"
                            class={classes!("flex","items-center","justify-center","p-1.5","rounded","border","transition-colors", view_btn_class(mode == ViewMode::List))}
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="currentColor" viewBox="0 0 24 24"><rect x="3" y="4" width="4" height="4" rx="1"/><rect x="9" y="5" width="12" height="2" rx="1"/><rect x="3" y="10" width="4" height="4" rx="1"/><rect x="9" y="11" width="12" height="2" rx="1"/><rect x="3" y="16" width="4" height="4" rx="1"/><rect x="9" y="17" width="12" height="2" rx="1"/></svg>
                        </button>
                        <button
                            onclick={move |_| on_sort_m.emit(SortKey::Modified)}
                            class={classes!("flex","items-center","gap-1","px-2","py-1","rounded","text-xs","font-bold","border","transition-colors", sort_btn_class(active_key == SortKey::Modified))}
                        >
                            <span>{ i18n::t("modified_date", lang) }</span>
                            <span>{ if mod_desc { "↓" } else { "↑" } }</span>
                        </button>
                        <button
                            onclick={move |_| on_sort_c.emit(SortKey::Created)}
                            class={classes!("flex","items-center","gap-1","px-2","py-1","rounded","text-xs","font-bold","border","transition-colors", sort_btn_class(active_key == SortKey::Created))}
                        >
                            <span>{ i18n::t("created_date", lang) }</span>
                            <span>{ if cre_desc { "↓" } else { "↑" } }</span>
                        </button>
                    </div>
                </div>
                <div class="flex-1 overflow-y-auto custom-scrollbar p-4">
                    if is_loading && file_list.is_empty() {
                        <div class="h-full flex items-center justify-center">
                            <div class="w-8 h-8 border-2 border-emerald-500/30 border-t-emerald-500 rounded-full animate-spin"></div>
                        </div>
                    } else if file_list.is_empty() {
                        <div class="h-full flex flex-col items-center justify-center text-gray-600 space-y-4">
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-12 w-12 opacity-20" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" /></svg>
                            <p class="text-xs uppercase tracking-widest font-bold">{ i18n::t("no_files_found", lang) }</p>
                        </div>
                    } else if mode == ViewMode::Grid {
                        // グリッド表示(3x3)
                        <div class="flex flex-wrap content-start gap-x-7 gap-y-8">
                            { for file_list.iter().enumerate().map(|(i, file)| make_card(i, file, true, true)) }
                        </div>
                    } else {
                        // リスト表示(1シート1行: 左にアイコン、右にアイコンの高さに収まるだけの行)
                        <div class="flex flex-col gap-2">
                            { for file_list.iter().enumerate().map(|(i, file)| {
                                let is_sel = sel == Some(i);
                                let lines = first_lines(file);
                                let s_idx_row = s_idx_state.clone();
                                let f_area_row = f_area_h.clone();
                                let on_ok_row = on_ok.clone();
                                let ads_row = ads_state.clone();
                                let dp_row = dp_state.clone();
                                let p_del_row = p_del_state.clone();
                                let fid_row = file.id.clone();
                                let fname_row = file.name.clone();
                                html! {
                                    <div
                                        class={classes!(
                                            "group","relative","flex","items-stretch","gap-3","p-2","rounded-lg","border","transition-colors","cursor-pointer",
                                            if is_sel { vec!["bg-emerald-600/10","border-emerald-500/50"] } else { vec!["border-white/10","hover:bg-white/5"] }
                                        )}
                                        onclick={move |_| { s_idx_row.set(Some(i)); f_area_row.set(FocusedArea::Files); }}
                                        ondblclick={move |_| on_ok_row.emit(())}
                                    >
                                        <div class="flex-shrink-0">
                                            { make_card(i, file, false, false) }
                                        </div>
                                        // アイコンの高さ(h-28=7rem)に収まるだけの行を表示。はみ出しは隠す。
                                        <div class={classes!("flex-1","min-w-0","max-h-28","overflow-hidden","text-xs","leading-snug","break-all","whitespace-pre-wrap", if is_sel { "text-emerald-50" } else { "text-gray-300" })}>
                                            { lines }
                                        </div>
                                        // 行の右上: カテゴリー変更(移動) / 削除 ボタン
                                        <div class="absolute top-1.5 right-1.5 flex items-center gap-1">
                                            <button
                                                onclick={let ads = ads_row.clone(); let fid_m = fid_row.clone(); let dp = dp_row.clone(); move |e: MouseEvent| {
                                                    e.stop_propagation();
                                                    let btn = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).and_then(|el| el.closest("button").ok().flatten());
                                                    if let Some(el) = btn { let rect = el.get_bounding_client_rect(); dp.set((rect.right(), rect.bottom() + 4.0)); }
                                                    ads.set(Some(fid_m.clone()));
                                                }}
                                                class="flex p-1 rounded bg-black/40 hover:bg-emerald-600 text-white"
                                                title="Move"
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg>
                                            </button>
                                            <button
                                                onclick={let p_del = p_del_row.clone(); let fid_d = fid_row.clone(); let fname_d = fname_row.clone(); move |e: MouseEvent| { e.stop_propagation(); p_del.set(Some((fid_d.clone(), fname_d.clone()))); }}
                                                class="p-1 rounded bg-black/50 hover:bg-red-600 text-white shadow"
                                                title="Delete"
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                                            </button>
                                        </div>
                                    </div>
                                }
                            }) }
                        </div>
                    }
                </div>
            </div>
        }
    };

    // カテゴリー名編集ダイアログ(半透明黒バック)
    let cat_edit_dialog_html = if let Some((cid, _cur)) = (*cat_edit).clone() {
        let cat_edit_h = cat_edit.clone();
        let input_h = cat_edit_input.clone();
        let on_rename = props.on_rename_category.clone();
        let value = (*cat_edit_input).clone();
        let on_cancel = { let h = cat_edit_h.clone(); Callback::from(move |_: ()| h.set(None)) };
        let on_change = {
            let on_rename = on_rename.clone();
            let input_h = input_h.clone();
            let h = cat_edit_h.clone();
            let cid = cid.clone();
            Callback::from(move |_: ()| {
                let new_name = (*input_h).clone();
                if !new_name.trim().is_empty() { on_rename.emit((cid.clone(), new_name)); }
                h.set(None);
            })
        };
        html! {
            <div class="fixed inset-0 z-[420] flex items-center justify-center bg-black/60 animate-in fade-in duration-150"
                 onclick={let c = on_cancel.clone(); move |_| c.emit(())}>
                <div class="w-[90vw] max-w-md bg-gray-900 border-2 border-emerald-500 rounded-xl shadow-2xl p-6 animate-in zoom-in-95 duration-150"
                     onclick={|e: MouseEvent| e.stop_propagation()}>
                    <h2 class="text-base font-bold text-gray-100 mb-4">{ i18n::t("edit_category_name", lang) }</h2>
                    <input
                        ref={cat_edit_input_ref.clone()}
                        type="text"
                        value={value}
                        oninput={let ih = input_h.clone(); move |e: InputEvent| { let inp: web_sys::HtmlInputElement = e.target_unchecked_into(); ih.set(inp.value()); }}
                        onkeydown={let ch = on_change.clone(); let cn = on_cancel.clone(); move |e: KeyboardEvent| {
                            if e.is_composing() { return; }
                            e.stop_propagation();
                            if e.key() == "Enter" { e.prevent_default(); ch.emit(()); }
                            else if e.key() == "Escape" { e.prevent_default(); cn.emit(()); }
                        }}
                        class="w-full bg-gray-800 text-white text-sm rounded px-3 py-2 outline-none border border-white/10 focus:border-emerald-500 mb-5"
                    />
                    <div class="flex justify-start gap-3">
                        <button onclick={let c = on_cancel.clone(); move |_| c.emit(())} class="px-5 py-2 rounded text-sm font-bold text-gray-300 hover:bg-white/5 transition-colors border border-white/10">{ i18n::t("cancel", lang) }</button>
                        <button onclick={let c = on_change.clone(); move |_| c.emit(())} class="px-5 py-2 rounded text-sm font-bold bg-emerald-600 hover:bg-emerald-500 text-white transition-colors">{ i18n::t("change", lang) }</button>
                    </div>
                </div>
            </div>
        }
    } else {
        html! {}
    };

    // カテゴリー変更ドロップダウン（fixedポジションでoverflow-hidden回避）
    let dropdown_menu_html = if let Some(ref dropdown_fid) = *active_dropdown_file_id {
        let (dd_right, dd_top) = *dropdown_pos;
        let vh = web_sys::window().map(|w| w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(800.0)).unwrap_or(800.0);
        let dropdown_left = (dd_right - 192.0).max(4.0); // w-48 = 192px
        let max_dropdown_h = 240.0;
        let use_top = if dd_top + max_dropdown_h > vh { (dd_top - max_dropdown_h - 8.0).max(4.0) } else { dd_top };
        let current_cid_for_dd = (*current_category_id).clone();
        let ads_dd = active_dropdown_file_id.clone();
        let fid_dd = dropdown_fid.clone();
        html! {
            <>
                <div class="fixed inset-0 z-[350]" onclick={let ads = ads_dd.clone(); move |_| ads.set(None)}></div>
                <div ref={dropdown_ref.clone()} class="fixed z-[360] w-48 bg-gray-800 border border-white/10 rounded-lg shadow-2xl py-1 animate-in fade-in zoom-in-95 duration-100"
                    style={format!("left: {}px; top: {}px;", dropdown_left, use_top)}
                    onclick={|e: MouseEvent| e.stop_propagation()}
                >
                    <div class="px-3 py-1.5 text-[10px] font-bold text-gray-500 uppercase tracking-widest border-b border-white/5 mb-1">{ "Move to category" }</div>
                    <div class="max-h-48 overflow-y-auto custom-scrollbar">
                        { for sorted_categories.iter().filter(|c| c.id != current_cid_for_dd).map(|c| {
                            let on_mv = on_move_file.clone(); let fid = fid_dd.clone(); let tcid = c.id.clone();
                            let cname = c.name.clone();
                            html! { <button onclick={move |e: MouseEvent| { e.stop_propagation(); on_mv.emit((fid.clone(), tcid.clone())); }} class="w-full text-left px-4 py-2 text-xs text-gray-300 hover:bg-emerald-600 hover:text-white transition-colors flex items-center space-x-2"><svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 opacity-50" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg><span>{ if cname == "OTHERS" { i18n::t("OTHERS", lang) } else { cname } }</span></button> }
                        }) }
                    </div>
                </div>
            </>
        }
    } else {
        html! {}
    };

    html! {
        <div
            ref={root_ref.clone()}
            tabindex="0"
            onkeydown={on_keydown}
            onfocusin={on_focus_in}
            onfocusout={on_focus_out}
            class={classes!(
                "fixed", "inset-0", "z-[100]", "flex", "outline-none", "pointer-events-auto",
                if *is_wide_layout { vec!["items-stretch", "justify-center"] } else { vec![] }
            )}
            onclick={|e: MouseEvent| e.stop_propagation()}
        >
            <div class={classes!(
                "absolute", "inset-0", "bg-black",
                if *is_fading_out { "animate-backdrop-out" } else { "animate-backdrop-in" }
            )} onclick={handle_close.reform(|_| ())}></div>

            <div class={classes!(
                "relative", "flex", "flex-col", "bg-gray-900", "overflow-hidden",
                "h-full", "shadow-2xl",
                "border-2", "border-emerald-500", "rounded-lg",
                if *is_wide_layout { vec!["w-full"] } else { vec!["w-full"] },
                if *is_wide_layout {
                    if *is_fading_out { "animate-slide-out" } else { "animate-slide-in" }
                } else {
                    if *is_fading_out { "animate-dialog-out" } else { "animate-dialog-in" }
                }
            )} onclick={|e: MouseEvent| e.stop_propagation()}>
                if *is_wide_layout {
                    // 横画面: 左端にカテゴリータブ(縦・幅=画面幅/10)、残り幅を半分ずつでファイルアイコンとプレビュー
                    <div class="flex flex-row flex-1 min-h-0">
                        { category_tabs_html }
                        <div class="flex flex-row flex-1 min-h-0">
                            // ファイルアイコン一覧
                            <div class="w-1/2 min-w-0 overflow-hidden border-r border-white/10 bg-gray-900">
                                { files_icon_grid_html }
                            </div>
                            // プレビュー（プリロード済データ）
                            <div class="w-1/2 min-w-0 overflow-hidden bg-gray-950">
                                { side_preview_html }
                            </div>
                        </div>
                    </div>
                } else {
                    // 縦画面: 従来通りカテゴリー一覧＋シート一覧の縦積み
                    // カテゴリー一覧
                    <div class={classes!("overflow-hidden", "min-h-0", "border-b", "border-white/5", "bg-gray-900", "p-1", "flex-1")}>
                        <div class="w-full h-full border-2 border-emerald-500 rounded-lg overflow-hidden">
                            { categories_html }
                        </div>
                    </div>
                    // シート一覧
                    <div class={classes!("flex", "flex-col", "overflow-hidden", "min-h-0", "bg-gray-950", "p-1", "flex-1")}>
                        <div class="w-full h-full border-2 border-emerald-500 rounded-lg overflow-hidden">
                            { files_html }
                        </div>
                    </div>
                }
                // フッターエリア
                <div class="bg-gray-950/50 border-t border-white/5 flex items-center justify-between p-3">
                    <div class={classes!("flex", if *is_wide_layout { vec!["flex-row", "space-x-2", "w-full", "items-center"] } else { vec!["flex-col", "space-y-2", "w-full"] })}>
                        if *is_wide_layout {
                            <div class="flex-1 flex items-center gap-3 select-none">
                                <button onclick={let ic = props.on_create_category_toggle.clone(); move |_| ic.emit(true)} class="flex items-center gap-1 px-2 py-1 rounded text-[11px] font-bold text-emerald-400 bg-emerald-500/10 hover:bg-emerald-500/20 border border-emerald-500/20 transition-all">
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" /></svg>
                                    <span>{ i18n::t("new_category", lang) }</span>
                                </button>
                                <button onclick={let on_ref = props.on_refresh.clone(); move |_| on_ref.emit(())} class="flex items-center gap-1 px-2 py-1 rounded text-[11px] font-bold text-gray-400 hover:bg-white/5 transition-all border border-white/10">
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" /></svg>
                                    <span>{ i18n::t("refresh_categories", lang) }</span>
                                </button>
                                <div class="flex items-center gap-3 text-[10px] text-gray-500 ml-2">
                                    <span class="flex items-center gap-1"><kbd class="px-1 py-0.5 bg-gray-800 rounded text-gray-400 font-mono">{"Alt+↑↓"}</kbd>{ i18n::t("key_navigate", lang) }</span>
                                    <span class="flex items-center gap-1"><kbd class="px-1 py-0.5 bg-gray-800 rounded text-gray-400 font-mono">{"Enter"}</kbd>{ i18n::t("key_confirm", lang) }</span>
                                    <span class="flex items-center gap-1"><kbd class="px-1 py-0.5 bg-gray-800 rounded text-gray-400 font-mono">{"Space"}</kbd>{ i18n::t("key_preview", lang) }</span>
                                </div>
                            </div>
                        }
                        if !*is_wide_layout {
                            <button
                                onclick={on_ok_click.reform(|_| ())}
                                disabled={selected_file_idx.is_none() || props.is_loading}
                                class={classes!(
                                    "py-2.5", "w-full", "rounded-md", "text-sm", "font-bold", "text-white", "transition-all", "uppercase", "tracking-widest",
                                    if selected_file_idx.is_none() || props.is_loading { vec!["bg-gray-800", "text-gray-600", "cursor-not-allowed"] } else { vec!["bg-emerald-600", "hover:bg-emerald-500", "shadow-lg", "shadow-emerald-900/20"] }
                                )}
                            >
                                { i18n::t("ok", lang) }
                            </button>
                            <button onclick={handle_close.reform(|_| ())} class="py-2.5 w-full rounded-md text-sm font-bold text-gray-400 hover:bg-white/5 transition-all uppercase tracking-widest border border-white/10">{ i18n::t("cancel", lang) }</button>
                        } else {
                            <button onclick={handle_close.reform(|_| ())} class="px-4 py-1.5 rounded-md text-xs font-bold text-gray-400 hover:bg-white/5 transition-all uppercase tracking-widest border border-white/10">{ i18n::t("cancel", lang) }</button>
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
                        }
                    </div>
                </div>
            </div>

            { dropdown_menu_html.clone() }

            { cat_edit_dialog_html }

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
                        on_cancel={let pd = pending_delete_file.clone(); let rr = root_ref.clone(); Callback::from(move |_| { 
                            pd.set(None); 
                            if let Some(root) = rr.cast::<web_sys::HtmlElement>() { let _ = root.focus(); }
                        })} 
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

            <LoadingOverlay is_visible={props.is_processing} message={i18n::t("synchronizing", lang)} z_index="z-[500]" />
        </div>
    }
}
