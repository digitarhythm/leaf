use yew::prelude::*;
use crate::components::button_bar::ButtonBar;
use crate::components::status_bar::StatusBar;
use crate::components::dialog::{CustomDialog, DialogOption, ConfirmDialog, NameConflictDialog};
use crate::components::file_open_dialog::FileOpenDialog;
use crate::components::preview::Preview;
use crate::js_interop::{init_editor, set_vim_mode, get_editor_content, set_editor_content, focus_editor, set_gutter_status, set_preview_active, generate_uuid, open_local_file, save_local_file, clear_local_handle};
use crate::auth_interop::request_access_token;
use crate::db_interop::{save_sheet, save_categories, JSCategory, JSSheet};
use crate::drive_interop::{upload_file, ensure_directory_structure, list_folders, download_file, list_files, get_file_metadata, delete_file, move_file, parse_date, find_file_by_name};
use crate::i18n::{self, Language};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use gloo::events::EventListener;
use gloo::events::EventListenerOptions;
use gloo::timers::callback::Timeout;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Clone, PartialEq)]
pub struct Sheet {
    pub id: String,
    pub guid: Option<String>,
    pub category: String,
    pub title: String,
    pub content: String,
    pub is_modified: bool,
    pub drive_id: Option<String>,
    pub temp_content: Option<String>,
    pub temp_timestamp: Option<u64>,
    pub last_sync_timestamp: Option<u64>,
    pub tab_color: String,
}

#[derive(Deserialize)]
struct Config {
    google_client_id: String,
}

#[derive(Clone, PartialEq)]
struct ConflictData {
    sheet_id: String,
    title: String,
    drive_id: String,
    local_content: String,
    drive_time: u64,
    time_str: String,
    is_missing_on_drive: bool,
}

#[derive(Clone, PartialEq)]
struct NameConflictData {
    sheet_id: String,
    filename: String,
    folder_id: String,
    existing_file_id: String,
}

const VIM_MODE_KEY: &str = "leaf_vim_mode";

fn generate_random_color() -> String {
    let h = (js_sys::Math::random() * 360.0) as u32;
    let s = 40 + (js_sys::Math::random() * 30.0) as u32;
    let l = 40 + (js_sys::Math::random() * 20.0) as u32;
    format!("hsl({}, {}%, {}%)", h, s, l)
}

#[function_component(App)]
pub fn app() -> Html {
    let lang = Language::detect();
    let config_str = include_str!("../application.toml");
    let config: Config = toml::from_str(config_str).expect("Failed to parse application.toml");
    let client_id = option_env!("LEAF_CLIENTID").map(|s| s.to_string()).unwrap_or_else(|| config.google_client_id);

    let vim_mode = use_state(|| {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item(VIM_MODE_KEY).ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(true)
    });
    let sheets = use_state(|| Vec::<Sheet>::new());
    let active_sheet_id = use_state(|| None::<String>);
    let network_connected = use_state(|| true);
    let is_authenticated = use_state(|| false);
    let no_category_folder_id = use_state(|| None::<String>);
    let leaf_data_folder_id = use_state(|| None::<String>);
    let auto_save_timer = use_state(|| None::<Timeout>);
    let is_loading = use_state(|| true);
    let is_saving = use_state(|| false);
    let is_import_lock = use_state(|| false);
    let is_import_fading_out = use_state(|| false);
    let is_initial_load = use_state(|| true);
    let loading_message_key = use_state(|| "synchronizing");
    let is_fading_out = use_state(|| false);
    let is_category_dropdown_open = use_state(|| false);
    let categories = use_state(|| Vec::<JSCategory>::new());
    let db_loaded = use_state(|| false);
    let conflict_queue = use_state(|| Vec::<ConflictData>::new());
    let name_conflict_queue = use_state(|| Vec::<NameConflictData>::new());
    let fallback_queue = use_state(|| Vec::<String>::new());
    let pending_import_data = use_state(|| None::<(String, String)>); // (filename, content)
    let is_logout_confirm_visible = use_state(|| false);
    let is_file_open_dialog_visible = use_state(|| false);
    let is_dialog_preview_open = use_state(|| false);
    let is_preview_visible = use_state(|| false);
    let is_help_visible = use_state(|| false);
    let is_suppressing_changes = use_state(|| false); 
    let pending_delete_category = use_state(|| None::<String>);
    let is_install_confirm_visible = use_state(|| false);
    let is_install_manual_visible = use_state(|| false);

    let sheets_ref = use_mut_ref(|| Vec::<Sheet>::new());
    let active_id_ref = use_mut_ref(|| None::<String>);
    let no_category_id_ref = use_mut_ref(|| None::<String>);
    let is_loading_ref = use_mut_ref(|| true);
    let is_saving_ref = use_mut_ref(|| false);
    let is_suppressing_ref = use_mut_ref(|| false);
    let is_preview_ref = use_mut_ref(|| false);
    let is_file_open_ref = use_mut_ref(|| false);
    let is_help_ref = use_mut_ref(|| false);

    const STORAGE_KEY_FIRST_LAUNCH: &str = "leaf_first_launch_v1";

    // 初回起動判定
    {
        let is_auth = is_authenticated.clone();
        let is_help = is_help_visible.clone();
        use_effect_with(is_auth, move |auth| {
            if **auth {
                let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
                let first_launch = storage.as_ref().and_then(|s| s.get_item(STORAGE_KEY_FIRST_LAUNCH).ok().flatten()).is_none();
                if first_launch {
                    is_help.set(true);
                    if let Some(s) = storage { let _ = s.set_item(STORAGE_KEY_FIRST_LAUNCH, "done"); }
                }
            }
            || ()
        });
    }

    // Ref sync
    {
        let s = sheets.clone(); let aid = active_sheet_id.clone(); let ncid = no_category_folder_id.clone();
        let ld = is_loading.clone(); let sp = is_suppressing_changes.clone();
        let prev = is_preview_visible.clone(); let open = is_file_open_dialog_visible.clone(); let help = is_help_visible.clone();
        
        let r_s = sheets_ref.clone(); let r_aid = active_id_ref.clone();
        let r_ncid = no_category_id_ref.clone(); let r_ld = is_loading_ref.clone(); let r_sp = is_suppressing_ref.clone();
        let r_prev = is_preview_ref.clone(); let r_open = is_file_open_ref.clone(); let r_help = is_help_ref.clone();

        use_effect_with((((*s).clone(), (*aid).clone(), (*ncid).clone()), (*ld, *sp, *prev, *open, *help)), move |deps| {
            let ((s_val, aid_val, ncid_val), (ld_val, sp_val, prev_val, open_val, help_val)) = deps;
            *r_s.borrow_mut() = s_val.clone(); *r_aid.borrow_mut() = aid_val.clone();
            *r_ncid.borrow_mut() = ncid_val.clone(); *r_ld.borrow_mut() = *ld_val; *r_sp.borrow_mut() = *sp_val;
            *r_prev.borrow_mut() = *prev_val; *r_open.borrow_mut() = *open_val; *r_help.borrow_mut() = *help_val;
            || ()
        });
    }

    let on_login = Callback::from(|_: MouseEvent| { request_access_token(); });
    let on_toggle_vim = {
        let vim = vim_mode.clone();
        Callback::from(move |_| {
            let next = !*vim;
            vim.set(next);
            if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
                let _ = storage.set_item(VIM_MODE_KEY, if next { "true" } else { "false" });
            }
            set_vim_mode(next);
        })
    };
    let on_change_font_size = Callback::from(|delta: i32| { crate::js_interop::change_font_size(delta); });
    let on_logout = { let ic = is_logout_confirm_visible.clone(); Callback::from(move |_| { ic.set(true); }) };

    let on_refresh_cats_cb = {
        let ldid_s = leaf_data_folder_id.clone(); let cats_s = categories.clone();
        let s_state = sheets.clone(); let aid_handle = active_sheet_id.clone();
        let ifo = is_file_open_dialog_visible.clone(); let il = is_loading.clone();
        Callback::from(move |_: ()| {
            if let Some(id) = (*ldid_s).clone() {
                let cs = cats_s.clone(); let ss_inner = s_state.clone(); let aid_inner = aid_handle.clone();
                let ifod = ifo.clone(); let ild_final = il.clone();
                spawn_local(async move {
                    if let Ok(cr) = list_folders(&id).await {
                        if let Ok(fv) = js_sys::Reflect::get(&cr, &JsValue::from_str("files")) {
                            let fa = js_sys::Array::from(&fv); let mut n_cats = Vec::new();
                            for i in 0..fa.length() { let v = fa.get(i); let ci = js_sys::Reflect::get(&v, &JsValue::from_str("id")).unwrap().as_string().unwrap(); let cn = js_sys::Reflect::get(&v, &JsValue::from_str("name")).unwrap().as_string().unwrap(); n_cats.push(JSCategory { id: ci, name: cn }); }
                            if let Ok(v) = serde_wasm_bindgen::to_value(&n_cats) { let _ = save_categories(v).await; }
                            cs.set(n_cats);
                        }
                    }
                    // Sync sheets if needed (simplified)
                    let mut us = (*ss_inner).clone(); let mut q = Vec::new();
                    let mut deleted = false;
                    for s in us.iter_mut() {
                        if s.drive_id.is_some() {
                            if let Ok(_) = get_file_metadata(&s.drive_id.clone().unwrap()).await { } else { q.push(s.id.clone()); }
                        }
                    }
                    for qid in q.clone() {
                        if let Some(pos) = us.iter().position(|x| x.id == qid) {
                            let _ = crate::db_interop::delete_sheet(&qid).await; us.remove(pos); deleted = true;
                        }
                    }
                    if deleted {
                        let ser = serde_wasm_bindgen::Serializer::json_compatible();
                        for s in us.iter() {
                            let js = JSSheet { id: s.id.clone(), guid: s.guid.clone(), category: s.category.clone(), title: s.title.clone(), content: s.content.clone(), is_modified: s.is_modified, drive_id: s.drive_id.clone(), temp_content: s.temp_content.clone(), temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, tab_color: s.tab_color.clone() };
                            if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                        }
                    }
                    if us.is_empty() {
                        let nid = js_sys::Date::now().to_string();
                        let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color() };
                        us.push(ns.clone()); aid_inner.set(Some(nid.clone())); set_editor_content(""); focus_editor();
                        let js = JSSheet { id: nid, guid: None, category: "".to_string(), title: "Untitled 1".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: ns.tab_color };
                        let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                    } else if deleted { let nid = us.last().unwrap().id.clone(); aid_inner.set(Some(nid)); }
                    ss_inner.set(us.clone()); 
                    if q.is_empty() { 
                        ifod.set(true); 
                        let ild = ild_final.clone(); 
                        let aid = aid_inner.clone(); 
                        let u_final = us.clone(); 
                        let ifo_final = ifod.clone();
                        Timeout::new(350, move || { 
                            ild.set(false); 
                            ifo_final.set(false);
                            if let Some(id) = (*aid).clone() { 
                                if let Some(s) = u_final.iter().find(|x| x.id == id) { 
                                    set_editor_content(&s.content); 
                                    let mode = if s.category.is_empty() { if s.title.starts_with("Untitled") { "unsaved" } else { "local" } } else if s.drive_id.is_none() { "unsaved" } else { "none" }; 
                                    set_gutter_status(mode); 
                                } 
                            } 
                            focus_editor(); 
                        }).forget(); 
                    }
                });
            }
        })
    };

    let os_handle: Rc<RefCell<Option<Callback<bool>>>> = Rc::new(RefCell::new(None));
    let on_save_cb = {
        let r_aid = active_id_ref.clone(); let r_s = sheets_ref.clone(); let s_state = sheets.clone();
        let r_ncid = no_category_id_ref.clone(); let nc_h = network_connected.clone();
        let ild_h = is_loading.clone();
        let lock_h = is_import_lock.clone();
        let lock_fade_h = is_import_fading_out.clone();
        let ris_h = is_saving_ref.clone(); let is_saving_h = is_saving.clone();
        let fq_h = fallback_queue.clone();
        let ncq_h = name_conflict_queue.clone();
        Callback::from(move |is_manual: bool| {
            if *ris_h.borrow() { return; }
            let r_aid = r_aid.clone(); let r_s = r_s.clone(); let s_state = s_state.clone();
            let r_ncid = r_ncid.clone(); let nc_h = nc_h.clone();
            let ild_h = ild_h.clone();
            let lock_h = lock_h.clone();
            let lock_fade_h = lock_fade_h.clone();
            let ris_h = ris_h.clone(); let is_saving_h = is_saving_h.clone();
            let fq_h = fq_h.clone(); let ncq_h = ncq_h.clone();
            
            Timeout::new(0, move || {
                let aid_opt = (*r_aid.borrow()).clone();
                let rs_cb = r_s.clone();
                if let Some(id) = aid_opt {
                    let cur_c_val = get_editor_content();
                    let cur_c = if let Some(s) = cur_c_val.as_string() { s } else { 
                        ild_h.set(false); lock_h.set(false); return; 
                    };
                    let mut cur_s = (*rs_cb.borrow()).clone();
                    let is_online = *nc_h && web_sys::window().unwrap().navigator().on_line();
                    let sheet_opt = if let Some(idx) = cur_s.iter().position(|s| s.id == id) { cur_s.get_mut(idx) } else { None };
                    if let Some(sheet) = sheet_opt {
                        let is_new = sheet.drive_id.is_none();
                        if is_new && !sheet.is_modified && !is_manual { return; }
                        if !is_manual && !is_new && !sheet.is_modified && sheet.content == cur_c { return; }
                        
                        sheet.content = cur_c.clone(); sheet.is_modified = false;
                        
                        if is_manual && sheet.drive_id.is_none() && sheet.guid.is_none() {
                            sheet.guid = Some(generate_uuid());
                        }
                        
                        // カテゴリーなしの処理 (ローカルファイル または クラウド未昇格)
                        if sheet.category.is_empty() {
                            is_saving_h.set(true); // ローカル保存時は表示
                            let content_to_save = cur_c.clone();
                            let is_saving_inner = is_saving_h.clone();
                            let ild_inner = ild_h.clone();
                            let lock_inner = lock_h.clone();
                            let lock_fade_inner = lock_fade_h.clone();

                            s_state.set(cur_s.clone());
                            spawn_local(async move {
                                let result = save_local_file(&content_to_save).await;
                                let success = result.as_bool().unwrap_or(false);
                                
                                if success {
                                    is_saving_inner.set(false);
                                    ild_inner.set(false);
                                    if *lock_inner {
                                        lock_fade_inner.set(true);
                                        let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                        let il = ild_inner.clone();
                                        Timeout::new(300, move || { lf.set(false); l.set(false); il.set(false); }).forget();
                                    }
                                } else {
                                    // キャンセルまたは失敗時は何もしない
                                    is_saving_inner.set(false);
                                    ild_inner.set(false);
                                    if *lock_inner {
                                        lock_fade_inner.set(true);
                                        let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                        Timeout::new(300, move || { lf.set(false); l.set(false); }).forget();
                                    }
                                }
                            });
                            return;
                        }

                        if !is_online {
                            // IndexedDBへの一時保存時は is_saving をセットしない（サイレント保存）
                            sheet.temp_content = Some(cur_c.clone()); sheet.temp_timestamp = Some(js_sys::Date::now() as u64);
                            let js = JSSheet { id: sheet.id.clone(), guid: sheet.guid.clone(), category: sheet.category.clone(), title: sheet.title.clone(), content: cur_c.clone(), is_modified: false, drive_id: sheet.drive_id.clone(), temp_content: sheet.temp_content.clone(), temp_timestamp: sheet.temp_timestamp, last_sync_timestamp: sheet.last_sync_timestamp, tab_color: sheet.tab_color.clone() };
                            let ild_inner = ild_h.clone();
                            let lock_inner = lock_h.clone();
                            let lock_fade_inner = lock_fade_h.clone();
                            s_state.set(cur_s);
                            spawn_local(async move { 
                                let ser = serde_wasm_bindgen::Serializer::json_compatible(); 
                                if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } 
                                // フェードアウト解除
                                if *lock_inner {
                                    lock_fade_inner.set(true);
                                    let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                    let il = ild_inner.clone();
                                    Timeout::new(300, move || { lf.set(false); l.set(false); il.set(false); }).forget();
                                } else {
                                    ild_inner.set(false);
                                }
                            });
                            return;
                        }
                        
                        // 以降は Google ドライブ保存なのでフラグをセット
                        is_saving_h.set(true);
                        let ncid_val = (*r_ncid.borrow()).clone();
                        if ncid_val.is_none() { 
                            is_saving_h.set(false); ild_h.set(false); 
                            if *lock_h {
                                lock_fade_h.set(true);
                                let l = lock_h.clone(); let lf = lock_fade_h.clone();
                                Timeout::new(300, move || { lf.set(false); l.set(false); }).forget();
                            }
                            return; 
                        }
                        let target_folder_id = if sheet.category == "OTHERS" { ncid_val.unwrap() } else { sheet.category.clone() };
                        let s_clone = sheet.clone(); let s_inner = s_state.clone(); let nc_inner = nc_h.clone();
                        let fq_inner = fq_h.clone(); let rs_async = rs_cb.clone();
                        let ris_inner = ris_h.clone(); let is_saving_inner = is_saving_h.clone(); let ncq_inner = ncq_h.clone();
                        let ild_inner = ild_h.clone();
                        let lock_inner = lock_h.clone();
                        let lock_fade_inner = lock_fade_h.clone();
                        *ris_inner.borrow_mut() = true;
                        
                        spawn_local(async move {
                             let _structure = match ensure_directory_structure().await { Ok(res) => res, Err(_) => { 
                                 *ris_inner.borrow_mut() = false; is_saving_inner.set(false); 
                                 if *lock_inner {
                                     lock_fade_inner.set(true);
                                     let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                     let il = ild_inner.clone();
                                     Timeout::new(300, move || { lf.set(false); l.set(false); il.set(false); }).forget();
                                 } else {
                                     ild_inner.set(false);
                                 }
                                 return; 
                             } };
                             if !s_clone.category.is_empty() && s_clone.category != "OTHERS" {
                                 if let Err(_) = get_file_metadata(&s_clone.category).await {
                                     fq_inner.set(vec![s_clone.id.clone()]); *ris_inner.borrow_mut() = false; is_saving_inner.set(false); 
                                     
                                     // 保存に失敗（カテゴリ紛失等）したので、変更フラグを戻して再試行を可能にする
                                     let mut u_s = (*rs_async.borrow()).clone();
                                     if let Some(si) = u_s.iter_mut().find(|x| x.id == s_clone.id) { 
                                         si.is_modified = true; 
                                         let js = JSSheet { id: si.id.clone(), guid: si.guid.clone(), category: si.category.clone(), title: si.title.clone(), content: si.content.clone(), is_modified: true, drive_id: si.drive_id.clone(), temp_content: si.temp_content.clone(), temp_timestamp: si.temp_timestamp, last_sync_timestamp: si.last_sync_timestamp, tab_color: si.tab_color.clone() };
                                         let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                     }
                                     *rs_async.borrow_mut() = u_s.clone(); s_inner.set(u_s);

                                     if *lock_inner {
                                         lock_fade_inner.set(true);
                                         let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                         let il = ild_inner.clone();
                                         Timeout::new(300, move || { lf.set(false); l.set(false); il.set(false); }).forget();
                                     } else {
                                         ild_inner.set(false);
                                     }
                                     return;
                                 }
                             }
                        
                             let fname = if let Some(guid) = &s_clone.guid {
                                 format!("{}.txt", guid)
                             } else {
                                 s_clone.title.clone()
                             };
                        
                             if s_clone.drive_id.is_none() && s_clone.guid.is_none() {
                                 if let Ok(existing) = find_file_by_name(&fname, &target_folder_id).await {
                                     if !existing.is_null() && !existing.is_undefined() {
                                         if let Ok(eid) = js_sys::Reflect::get(&existing, &JsValue::from_str("id")) {
                                             if let Some(eid_str) = eid.as_string() {
                                                 let mut q = (*ncq_inner).clone();
                                                 q.push(NameConflictData {
                                                     sheet_id: s_clone.id.clone(),
                                                     filename: fname.clone(),
                                                     folder_id: target_folder_id.clone(),
                                                     existing_file_id: eid_str,
                                                 });
                                                 ncq_inner.set(q);
                                                 *ris_inner.borrow_mut() = false; is_saving_inner.set(false);
                                                 ild_inner.set(false); 
                                                 return;
                                             }
                                         }
                                     }
                                 }
                             }
                        
                             let res = upload_file(&fname, &s_clone.content, &target_folder_id, s_clone.drive_id.as_deref()).await;
                        
                             let mut n_did = s_clone.drive_id.clone(); let mut stime = s_clone.last_sync_timestamp;
                             match res {
                                 Ok(rv) => {
                                     if let Ok(iv) = js_sys::Reflect::get(&rv, &JsValue::from_str("id")) { if let Some(is) = iv.as_string() { n_did = Some(is); } }
                                     if let Ok(tv) = js_sys::Reflect::get(&rv, &JsValue::from_str("modifiedTime")) { if let Some(ts) = tv.as_string() { stime = Some(parse_date(&ts) as u64); } }
                                 },
                                 Err(_) => {
                                     nc_inner.set(false);
                                     let js = JSSheet { id: s_clone.id.clone(), guid: s_clone.guid.clone(), category: s_clone.category.clone(), title: s_clone.title.clone(), content: s_clone.content.clone(), is_modified: true, drive_id: s_clone.drive_id.clone(), temp_content: Some(s_clone.content.clone()), temp_timestamp: Some(js_sys::Date::now() as u64), last_sync_timestamp: s_clone.last_sync_timestamp, tab_color: s_clone.tab_color.clone() };
                                     let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                     
                                     let mut u_s = (*rs_async.borrow()).clone();
                                     if let Some(si) = u_s.iter_mut().find(|x| x.id == s_clone.id) { si.is_modified = true; }
                                     *rs_async.borrow_mut() = u_s.clone(); s_inner.set(u_s);

                                     *ris_inner.borrow_mut() = false; is_saving_inner.set(false); 
                                     if *lock_inner {
                                         lock_fade_inner.set(true);
                                         let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                         let il = ild_inner.clone();
                                         Timeout::new(300, move || { lf.set(false); l.set(false); il.set(false); }).forget();
                                     } else {
                                         ild_inner.set(false);
                                     }
                                     return;
                                 },
                             }
                             let js = JSSheet { id: s_clone.id.clone(), guid: s_clone.guid.clone(), category: s_clone.category.clone(), title: s_clone.title.clone(), content: s_clone.content.clone(), is_modified: false, drive_id: n_did.clone(), temp_content: None, temp_timestamp: None, last_sync_timestamp: stime, tab_color: s_clone.tab_color.clone() };
                             let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                             let mut u_s = (*rs_async.borrow()).clone();
                             if let Some(si) = u_s.iter_mut().find(|x| x.id == s_clone.id) { 
                                 si.drive_id = n_did; 
                                 // 保存中に編集された可能性を考慮し、contentは上書きしない
                                 // 保存開始時の内容(s_clone.content)と現在の内容が一致する場合のみ is_modified を false にする
                                 if si.content == s_clone.content {
                                     si.is_modified = false; 
                                 }
                                 si.temp_content = None; 
                                 si.temp_timestamp = None; 
                                 si.last_sync_timestamp = stime; 
                             }
                             *rs_async.borrow_mut() = u_s.clone(); s_inner.set(u_s);
                             set_gutter_status("none"); 
                             *ris_inner.borrow_mut() = false; is_saving_inner.set(false); 
                             if *lock_inner {
                                 lock_fade_inner.set(true);
                                 let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                 let il = ild_inner.clone();
                                 Timeout::new(300, move || { lf.set(false); l.set(false); il.set(false); }).forget();
                             } else {
                                 ild_inner.set(false);
                             }
                        });
                        s_state.set(cur_s);
                    } else {
                        ild_h.set(false); lock_h.set(false);
                    }
                } else {
                    ild_h.set(false); lock_h.set(false);
                }
            }).forget();
        })
    };
    *os_handle.borrow_mut() = Some(on_save_cb.clone());

    let on_name_conflict_cfm = {
        let ncq = name_conflict_queue.clone(); let s_state = sheets.clone();
        let rs = sheets_ref.clone(); let os = on_save_cb.clone();
        Callback::from(move |(sel, input_name): (usize, String)| {
            let mut q = (*ncq).clone(); if q.is_empty() { return; }
            let conflict = q.remove(0); let mut us = (*s_state).clone();
            if let Some(s) = us.iter_mut().find(|x| x.id == conflict.sheet_id) {
                match sel {
                    0 => { s.drive_id = Some(conflict.existing_file_id); }
                    1 => { s.guid = Some(generate_uuid()); }
                    2 => {
                        if !input_name.trim().is_empty() {
                            s.title = input_name;
                            s.guid = None;
                        } else {
                            s.guid = Some(generate_uuid());
                        }
                    }
                    _ => {}
                }
                let js = JSSheet { id: s.id.clone(), guid: s.guid.clone(), category: s.category.clone(), title: s.title.clone(), content: s.content.clone(), is_modified: s.is_modified, drive_id: s.drive_id.clone(), temp_content: s.temp_content.clone(), temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, tab_color: s.tab_color.clone() };
                let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { spawn_local(async move { let _ = save_sheet(v).await; }); }
            }
            *rs.borrow_mut() = us.clone(); s_state.set(us); ncq.set(q);
            let os_retry = os.clone(); Timeout::new(100, move || { os_retry.emit(true); }).forget();
        })
    };

    let on_new_sheet_cb = {
        let s_state = sheets.clone(); let aid_state = active_sheet_id.clone();
        let sp_state = is_suppressing_changes.clone(); let r_s = sheets_ref.clone();
        let os = on_save_cb.clone();
        Callback::from(move |_| {
            let s = s_state.clone(); let aid = aid_state.clone(); let sp = sp_state.clone();
            let rs = r_s.clone();
            let os_cb = os.clone();
            
            // 現在のシートに変更があれば保存を実行
            let aid_val = (*aid).clone();
            let mut needs_save = false;
            if let Some(id) = aid_val {
                let cur_s = (*rs.borrow()).clone();
                if let Some(sheet) = cur_s.iter().find(|x| x.id == id) {
                    let cur_c_val = get_editor_content();
                    if let Some(cur_c) = cur_c_val.as_string() {
                        // テキストが空でない、かつ変更がある場合のみ保存
                        if !cur_c.trim().is_empty() && (sheet.is_modified || sheet.content != cur_c) {
                            needs_save = true;
                        }
                    }
                }
            }
            if needs_save {
                os_cb.emit(false);
            }

            sp.set(true); 
            // 保存処理の完了を待たず、非同期で新規作成へ移行（needs_save時は少し余裕を持たせる）
            let delay = if needs_save { 100 } else { 0 };
            Timeout::new(delay, move || {
                clear_local_handle();
                let nid = js_sys::Date::now().to_string();
                let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color() };
                set_editor_content(""); set_gutter_status("unsaved");
                
                let mut current_sheets = (*rs.borrow()).clone();
                current_sheets.push(ns.clone());
                *rs.borrow_mut() = current_sheets.clone();
                s.set(current_sheets);
                aid.set(Some(nid.clone()));
                
                focus_editor(); let spr = sp.clone(); Timeout::new(500, move || { spr.set(false); }).forget();
                spawn_local(async move {
                    let js = JSSheet { id: nid, guid: None, category: "".to_string(), title: "Untitled".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: ns.tab_color };
                    let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                });
            }).forget();
        })
    };

    let on_fallback_cfm = {
        let fq = fallback_queue.clone(); let s_state = sheets.clone(); let rs = sheets_ref.clone(); let os = on_save_cb.clone();
        Callback::from(move |_: usize| {
            let mut q = (*fq).clone(); if q.is_empty() { return; }
            let sid = q.remove(0); let mut us = (*s_state).clone();
            if let Some(s) = us.iter_mut().find(|x| x.id == sid) {
                s.category = "OTHERS".to_string();
                let js = JSSheet { id: s.id.clone(), guid: s.guid.clone(), category: s.category.clone(), title: s.title.clone(), content: s.content.clone(), is_modified: s.is_modified, drive_id: s.drive_id.clone(), temp_content: s.temp_content.clone(), temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, tab_color: s.tab_color.clone() };
                let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { spawn_local(async move { let _ = save_sheet(v).await; }); }
            }
            *rs.borrow_mut() = us.clone(); s_state.set(us); fq.set(q); os.emit(true); 
        })
    };

    let on_delete_category_cb = {
        let pending = pending_delete_category.clone();
        Callback::from(move |id: String| { pending.set(Some(id)); })
    };

    let on_delete_category_cfm = {
        let pending = pending_delete_category.clone(); let il = is_loading.clone(); let ifo = is_fading_out.clone();
        let lmk = loading_message_key.clone(); let on_refresh = on_refresh_cats_cb.clone();
        let s_state = sheets.clone(); let rs = sheets_ref.clone(); let ncid_state = no_category_folder_id.clone();
        Callback::from(move |_: usize| {
            if let Some(tcid) = (*pending).clone() {
                let pending_inner = pending.clone(); let il_inner = il.clone(); let ifo_inner = ifo.clone();
                let lmk_inner = lmk.clone(); let on_refresh_inner = on_refresh.clone();
                let ss = s_state.clone(); let rs_inner = rs.clone(); let ncid_s = ncid_state.clone();
                lmk_inner.set("synchronizing"); il_inner.set(true); ifo_inner.set(false);
                spawn_local(async move {
                    let structure = match ensure_directory_structure().await { Ok(res) => res, Err(_) => { return; } };
                    let ncid = js_sys::Reflect::get(&structure, &JsValue::from_str("othersId")).unwrap().as_string().unwrap();
                    ncid_s.set(Some(ncid.clone()));
                    if let Ok(fr) = list_files(&tcid, None).await {
                        if let Ok(fv) = js_sys::Reflect::get(&fr, &JsValue::from_str("files")) {
                            let fa = js_sys::Array::from(&fv);
                            for i in 0..fa.length() {
                                let fm = fa.get(i); let fid = js_sys::Reflect::get(&fm, &JsValue::from_str("id")).unwrap().as_string().unwrap();
                                let _ = move_file(&fid, &tcid, &ncid).await;
                            }
                        }
                    }
                    let _ = delete_file(&tcid).await;
                    let mut us = (*ss).clone(); let mut changed = false;
                    for s in us.iter_mut() { if s.category == tcid { s.category = ncid.clone(); changed = true; } }
                    if changed {
                        let ser = serde_wasm_bindgen::Serializer::json_compatible();
                        for s in us.iter() {
                            let js = JSSheet { id: s.id.clone(), guid: s.guid.clone(), category: s.category.clone(), title: s.title.clone(), content: s.content.clone(), is_modified: s.is_modified, drive_id: s.drive_id.clone(), temp_content: s.temp_content.clone(), temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, tab_color: s.tab_color.clone() };
                            if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                        }
                        *rs_inner.borrow_mut() = us.clone(); ss.set(us);
                    }
                    pending_inner.set(None); on_refresh_inner.emit(()); ifo_inner.set(true); 
                    let ifo_final = ifo_inner.clone();
                    Timeout::new(300, move || { 
                        il_inner.set(false);
                        ifo_final.set(false);
                    }).forget();
                });
            }
        })
    };

    let on_rename_category_cb = {
        let il = is_loading.clone(); let ifo = is_fading_out.clone();
        let lmk = loading_message_key.clone(); let on_refresh = on_refresh_cats_cb.clone();
        Callback::from(move |(id, new_name): (String, String)| {
            let il_inner = il.clone(); let ifo_inner = ifo.clone();
            let lmk_inner = lmk.clone(); let on_refresh_inner = on_refresh.clone();
            lmk_inner.set("synchronizing"); il_inner.set(true); ifo_inner.set(false);
            spawn_local(async move {
                if let Ok(_) = crate::drive_interop::rename_folder(&id, &new_name).await {
                    on_refresh_inner.emit(());
                }
                ifo_inner.set(true); 
                let ifo_final = ifo_inner.clone();
                Timeout::new(300, move || { 
                    il_inner.set(false);
                    ifo_final.set(false);
                }).forget();
            });
        })
    };

    let on_conf_cfm = {
        let cq = conflict_queue.clone(); let ss = sheets.clone(); let il = is_loading.clone();
        let ifo = is_fading_out.clone(); let ncid = no_category_folder_id.clone();
        let aid = active_sheet_id.clone();
        Callback::from(move |sel: usize| {
            let mut q = (*cq).clone(); if q.is_empty() { return; } let conf = q.remove(0);
            let ss_inner = ss.clone(); let qs = cq.clone(); let ifod = ifo.clone();
            let fid_opt = ncid.as_ref().map(|s| s.clone()); let aid_v = (*aid).clone();
            let aid_inner = aid.clone(); let ild_final = il.clone();
            spawn_local(async move {
                let mut us = (*ss_inner).clone(); let mut deleted = false;
                if let Some(pos) = us.iter().position(|x| x.id == conf.sheet_id) {
                    let s = &mut us[pos];
                    match sel {
                        0 => { if let Ok(dv) = download_file(&conf.drive_id, None, None).await { if let Some(t) = dv.as_string() { s.content = t.clone(); s.temp_content = None; s.temp_timestamp = None; s.last_sync_timestamp = Some(conf.drive_time); s.is_modified = false; if Some(s.id.clone()) == aid_v { set_editor_content(&t); } } } },
                        1 => { 
                            if let Some(fid) = fid_opt { 
                                let did = if conf.is_missing_on_drive { None } else { Some(conf.drive_id.as_str()) };
                                if let Ok(rv) = upload_file(&format!("{}.txt", s.guid.as_ref().unwrap_or(&generate_uuid())), &s.content, &fid, did).await {
                                    if let Ok(iv) = js_sys::Reflect::get(&rv, &JsValue::from_str("id")) { if let Some(is) = iv.as_string() { s.drive_id = Some(is); } }
                                    if let Ok(tv) = js_sys::Reflect::get(&rv, &JsValue::from_str("modifiedTime")) { if let Some(ts) = tv.as_string() { s.last_sync_timestamp = Some(crate::drive_interop::parse_date(&ts) as u64); } }
                                    s.temp_content = None; s.temp_timestamp = None; s.is_modified = false; 
                                }
                            } 
                        },
                        2 => { if let Some(fid) = fid_opt { let ng = generate_uuid(); let _ = upload_file(&format!("{}.txt", ng), &s.content, &fid, None).await; s.guid = Some(ng); s.temp_content = None; s.temp_timestamp = None; s.last_sync_timestamp = Some(js_sys::Date::now() as u64); s.is_modified = false; s.tab_color = generate_random_color(); } },
                        3 => { let _ = crate::db_interop::delete_sheet(&s.id).await; us.remove(pos); deleted = true; },
                        _ => {}
                    }
                    if !deleted {
                        let ds = &us[pos];
                        let js = JSSheet { id: ds.id.clone(), guid: ds.guid.clone(), category: ds.category.clone(), title: ds.title.clone(), content: ds.content.clone(), is_modified: ds.is_modified, drive_id: ds.drive_id.clone(), temp_content: ds.temp_content.clone(), temp_timestamp: ds.temp_timestamp, last_sync_timestamp: ds.last_sync_timestamp, tab_color: ds.tab_color.clone() };
                        let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                    }
                }
                if us.is_empty() {
                    let nid = js_sys::Date::now().to_string();
                    let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color() };
                    us.push(ns.clone()); aid_inner.set(Some(nid.clone())); set_editor_content(""); focus_editor();
                    let js = JSSheet { id: nid, guid: None, category: "".to_string(), title: "Untitled 1".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: ns.tab_color };
                    let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                } else if deleted { let nid = us.last().unwrap().id.clone(); aid_inner.set(Some(nid)); }
                ss_inner.set(us.clone()); qs.set(q.clone());
                if q.is_empty() { 
                    ifod.set(true); 
                    let ild = ild_final.clone(); 
                    let aid = aid_inner.clone(); 
                    let u_final = us.clone(); 
                    let ifo_final = ifod.clone();
                    Timeout::new(350, move || { 
                        ild.set(false); 
                        ifo_final.set(false);
                        if let Some(id) = (*aid).clone() { 
                            if let Some(s) = u_final.iter().find(|x| x.id == id) { 
                                set_editor_content(&s.content); 
                                let mode = if s.category.is_empty() { if s.title.starts_with("Untitled") { "unsaved" } else { "local" } } else if s.drive_id.is_none() { "unsaved" } else { "none" }; 
                                set_gutter_status(mode); 
                            } 
                        } 
                        focus_editor(); 
                    }).forget(); 
                }
            });
        })
    };

    let on_file_sel_cb = {
        let aid = active_sheet_id.clone(); let iv = is_file_open_dialog_visible.clone();
        let ss = sheets.clone(); let il = is_loading.clone(); let ifo = is_fading_out.clone();
        let rs = sheets_ref.clone(); let sp = is_suppressing_changes.clone();
        let os = on_save_cb.clone();
        let lmk = loading_message_key.clone();
        Callback::from(move |(did, title, cat_id): (String, String, String)| {
            let aid_val = (*aid).clone();
            let mut needs_save = false;
            if let Some(id) = aid_val {
                let cur_s = (*rs.borrow()).clone();
                if let Some(sheet) = cur_s.iter().find(|x| x.id == id) {
                    let cur_c_val = get_editor_content();
                    if let Some(cur_c) = cur_c_val.as_string() {
                        if !cur_c.trim().is_empty() && (sheet.is_modified || sheet.content != cur_c) {
                            needs_save = true;
                        }
                    }
                }
            }
            if needs_save {
                os.emit(false);
            }

            iv.set(false); 
            lmk.set("synchronizing"); il.set(true); ifo.set(false); sp.set(true); 
            let ss_inner = ss.clone(); let aid_inner = aid.clone(); let sp_inner = sp.clone();
            let il_inner = il.clone(); let ifo_inner = ifo.clone(); let rs_inner = rs.clone();
            spawn_local(async move {
                if let Ok(cv) = download_file(&did, None, None).await {
                    if let Some(c) = cv.as_string() {
                        let mut cs = (*rs_inner.borrow()).clone();
                        let tidx = if cs.len() == 1 && cs[0].drive_id.is_none() { Some(0) } else { None };
                        let guid = if title.ends_with(".txt") { Some(title.replace(".txt", "")) } else { Some(title.clone()) };
                        let nid = if let Some(idx) = tidx { cs[idx].id.clone() } else { js_sys::Date::now().to_string() };
                        let ns = Sheet { id: nid.clone(), guid: guid.clone(), category: cat_id.clone(), title: title.clone(), content: c.clone(), is_modified: false, drive_id: Some(did.clone()), temp_content: None, temp_timestamp: None, last_sync_timestamp: Some(js_sys::Date::now() as u64), tab_color: if let Some(idx) = tidx { cs[idx].tab_color.clone() } else { generate_random_color() } };
                        set_editor_content(&c); set_gutter_status("none");
                        if let Some(idx) = tidx { cs[idx] = ns.clone(); } else { cs = vec![ns.clone()]; }
                        *rs_inner.borrow_mut() = cs.clone(); ss_inner.set(cs); aid_inner.set(Some(nid.clone()));
                        focus_editor(); 
                        
                        let js = JSSheet { id: nid, guid, category: cat_id, title, content: c, is_modified: false, drive_id: Some(did), temp_content: None, temp_timestamp: None, last_sync_timestamp: Some(js_sys::Date::now() as u64), tab_color: ns.tab_color };
                        let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                        
                        // テキストがセットされた後、描画を待ってからフェードアウト開始
                        Timeout::new(50, move || {
                            ifo_inner.set(true);
                            let ifo_final = ifo_inner.clone();
                            Timeout::new(300, move || {
                                il_inner.set(false);
                                sp_inner.set(false);
                                ifo_final.set(false);
                            }).forget();
                        }).forget();
                    }
                } else { 
                    il_inner.set(false); sp_inner.set(false); 
                }
            });
        })
    };

    let lock_for_import = is_import_lock.clone();
    let il_for_import = is_loading.clone();
    let ifo_for_import = is_fading_out.clone();
    let lock_fade_for_import = is_import_fading_out.clone();
    let on_import_cb = {
        let s_state = sheets.clone(); let aid_state = active_sheet_id.clone();
        let sp_state = is_suppressing_changes.clone(); let r_s = sheets_ref.clone();
        let lock_h = lock_for_import;
        let il_h = il_for_import;
        let ifo_h = ifo_for_import;
        let lock_fade_h = lock_fade_for_import;
        let os = on_save_cb.clone();
        Callback::from(move |_| {
            let aid_val = (*aid_state).clone();
            let mut needs_save = false;
            if let Some(id) = aid_val {
                let cur_s = (*r_s.borrow()).clone();
                if let Some(sheet) = cur_s.iter().find(|x| x.id == id) {
                    let cur_c_val = get_editor_content();
                    if let Some(cur_c) = cur_c_val.as_string() {
                        if !cur_c.trim().is_empty() && (sheet.is_modified || sheet.content != cur_c) {
                            needs_save = true;
                        }
                    }
                }
            }
            if needs_save {
                os.emit(false);
            }

            let s_state_c = s_state.clone(); let aid_state_c = aid_state.clone();
            let sp_state_c = sp_state.clone(); let r_s_c = r_s.clone();
            let lock_cb = lock_h.clone();
            let il_cb = il_h.clone();
            let ifo_cb = ifo_h.clone();
            let lock_fade_cb = lock_fade_h.clone();
            
            spawn_local(async move {
                let res = open_local_file().await;
                if res.is_null() || res.is_undefined() { return; }
                
                if let (Some(name), Some(content)) = (
                    js_sys::Reflect::get(&res, &JsValue::from_str("name")).ok().and_then(|v| v.as_string()),
                    js_sys::Reflect::get(&res, &JsValue::from_str("content")).ok().and_then(|v| v.as_string())
                ) {
                    ifo_cb.set(false); lock_fade_cb.set(false);
                    il_cb.set(true); lock_cb.set(true);
                    
                    let nid = js_sys::Date::now().to_string();
                    let ns = Sheet { 
                        id: nid.clone(), 
                        guid: None, 
                        category: "".to_string(), // カテゴリーなし
                        title: name.clone(), 
                        content: content.clone(), 
                        is_modified: false,
                        drive_id: None, 
                        temp_content: None, 
                        temp_timestamp: None, 
                        last_sync_timestamp: None, 
                        tab_color: generate_random_color() 
                    };
                    
                    sp_state_c.set(true);
                    *r_s_c.borrow_mut() = vec![ns.clone()];
                    s_state_c.set(vec![ns.clone()]);
                    aid_state_c.set(Some(nid.clone()));
                    
                    set_editor_content(&content);
                    set_gutter_status("local");
                    crate::js_interop::set_editor_mode(&name);
                    
                    let js = JSSheet { 
                        id: nid, guid: None, category: "".to_string(), title: name, content: content, 
                        is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, 
                        last_sync_timestamp: None, tab_color: ns.tab_color 
                    };
                    let ser = serde_wasm_bindgen::Serializer::json_compatible();
                    if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                    
                    // テキストセット後の描画待ち
                    Timeout::new(50, move || {
                        ifo_cb.set(true);
                        let il = il_cb.clone(); let sp = sp_state_c.clone(); let ifo_final = ifo_cb.clone();
                        Timeout::new(300, move || {
                            il.set(false);
                            sp.set(false);
                            ifo_final.set(false);
                        }).forget();
                    }).forget();
                    lock_cb.set(false); 
                }
            });
        })
    };

    let on_change_category_cb = {
        let s_state = sheets.clone(); let aid_state = active_sheet_id.clone();
        let ncid = no_category_folder_id.clone(); let il = is_loading.clone();
        let ifo = is_fading_out.clone(); let lmk = loading_message_key.clone();
        let r_s = sheets_ref.clone();
        let os = on_save_cb.clone();
        Callback::from(move |new_cat_id: String| {
            let aid = (*aid_state).clone();
            if let Some(id) = aid {
                let current_sheets = (*s_state).clone();
                if let Some(pos) = current_sheets.iter().position(|s| s.id == id) {
                    let mut sheet = current_sheets[pos].clone();
                    let mut old_cat_id = sheet.category.clone();
                    let file_id_opt = sheet.drive_id.clone();
                    
                    if old_cat_id == "OTHERS" { if let Some(real_id) = (*ncid).clone() { old_cat_id = real_id; } }
                    if old_cat_id == new_cat_id { return; }
                    
                    let s_state_inner = s_state.clone(); let il_inner = il.clone(); let ifo_inner = ifo.clone();
                    let lmk_inner = lmk.clone(); let r_s_inner = r_s.clone();
                    let os_inner = os.clone();

                    // カテゴリーなしからの昇格
                    if old_cat_id.is_empty() && !new_cat_id.is_empty() {
                        sheet.guid = Some(generate_uuid());
                        clear_local_handle();
                        sheet.category = new_cat_id;
                        let mut us = current_sheets; us[pos] = sheet;
                        *r_s_inner.borrow_mut() = us.clone(); s_state_inner.set(us);
                        // 昇格時は即座に保存（アップロード）を実行
                        Timeout::new(0, move || { os_inner.emit(true); }).forget();
                        return;
                    }

                    if let Some(fid) = file_id_opt {
                        lmk_inner.set("synchronizing"); il_inner.set(true); ifo_inner.set(false);
                        spawn_local(async move {
                            if let Ok(_) = move_file(&fid, &old_cat_id, &new_cat_id).await {
                                let mut us = (*s_state_inner).clone();
                                if let Some(s) = us.iter_mut().find(|x| x.id == id) {
                                    s.category = new_cat_id.clone();
                                    let js = JSSheet { id: s.id.clone(), guid: s.guid.clone(), category: s.category.clone(), title: s.title.clone(), content: s.content.clone(), is_modified: s.is_modified, drive_id: s.drive_id.clone(), temp_content: s.temp_content.clone(), temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, tab_color: s.tab_color.clone() };
                                    let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                    if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                }
                                *r_s_inner.borrow_mut() = us.clone(); s_state_inner.set(us);
                            }
                            ifo_inner.set(true); 
                            let ifo_final = ifo_inner.clone();
                            Timeout::new(300, move || { 
                                il_inner.set(false);
                                ifo_final.set(false);
                            }).forget();
                        });
                    } else {
                        let mut us = current_sheets; us[pos].category = new_cat_id;
                        let s = &us[pos];
                        let js = JSSheet { id: s.id.clone(), guid: s.guid.clone(), category: s.category.clone(), title: s.title.clone(), content: s.content.clone(), is_modified: s.is_modified, drive_id: s.drive_id.clone(), temp_content: s.temp_content.clone(), temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, tab_color: s.tab_color.clone() };
                        spawn_local(async move { let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } });
                        *r_s_inner.borrow_mut() = us.clone(); s_state_inner.set(us);
                    }
                }
            }
        })
    };

    let on_change_extension_cb = {
        let s_state = sheets.clone(); let aid_state = active_sheet_id.clone();
        let il = is_loading.clone(); let ifo = is_fading_out.clone();
        let lmk = loading_message_key.clone(); let r_s = sheets_ref.clone();
        Callback::from(move |new_ext: String| {
            let aid = (*aid_state).clone();
            if let Some(id) = aid {
                let current_sheets = (*s_state).clone();
                if let Some(pos) = current_sheets.iter().position(|s| s.id == id) {
                    let sheet = current_sheets[pos].clone();
                    if sheet.drive_id.is_none() { return; } // Google Drive保存済みのみ

                    let old_name = sheet.title.clone();
                    let name_parts: Vec<&str> = old_name.split('.').collect();
                    let base_name = if name_parts.len() > 1 {
                        name_parts[..name_parts.len()-1].join(".")
                    } else {
                        old_name.clone()
                    };
                    let new_name = format!("{}.{}", base_name, new_ext);
                    
                    if old_name == new_name { return; }

                    let s_state_inner = s_state.clone(); let il_inner = il.clone(); 
                    let ifo_inner = ifo.clone(); let lmk_inner = lmk.clone(); 
                    let r_s_inner = r_s.clone();
                    let drive_id = sheet.drive_id.clone().unwrap();

                    lmk_inner.set("synchronizing"); il_inner.set(true); ifo_inner.set(false);
                    spawn_local(async move {
                        // ファイル名変更も rename_folder (PATCH {name}) と同じ
                        if let Ok(_) = crate::drive_interop::rename_folder(&drive_id, &new_name).await {
                            let mut us = (*s_state_inner).clone();
                            if let Some(s) = us.iter_mut().find(|x| x.id == id) {
                                s.title = new_name.clone();
                                let js = JSSheet { id: s.id.clone(), guid: s.guid.clone(), category: s.category.clone(), title: s.title.clone(), content: s.content.clone(), is_modified: s.is_modified, drive_id: s.drive_id.clone(), temp_content: s.temp_content.clone(), temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, tab_color: s.tab_color.clone() };
                                let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                crate::js_interop::set_editor_mode(&new_name);
                            }
                            *r_s_inner.borrow_mut() = us.clone(); s_state_inner.set(us);
                        }
                        ifo_inner.set(true); 
                        let ifo_final = ifo_inner.clone();
                        Timeout::new(300, move || { 
                            il_inner.set(false); 
                            ifo_final.set(false);
                        }).forget();
                    });
                }
            }
        })
    };

    let on_confirm_import = {
        let pending = pending_import_data.clone();
        let s_state = sheets.clone(); let aid_state = active_sheet_id.clone();
        let sp_state = is_suppressing_changes.clone(); let r_s = sheets_ref.clone();
        let ncid = no_category_folder_id.clone();
        let os_cb_outer = on_save_cb.clone();
        Callback::from(move |convert: bool| {
            if let Some((filename, text)) = (*pending).clone() {
                let nid = js_sys::Date::now().to_string();
                let cat_id = (*ncid).clone().unwrap_or_else(|| "OTHERS".to_string());
                let mut final_text = text;
                if convert {
                    final_text = final_text.replace("\r\n", "\n");
                }
                
                let ns = Sheet { id: nid.clone(), guid: None, category: cat_id.clone(), title: filename.clone(), content: final_text.clone(), is_modified: true, drive_id: None, temp_content: Some(final_text.clone()), temp_timestamp: Some(js_sys::Date::now() as u64), last_sync_timestamp: None, tab_color: generate_random_color() };
                sp_state.set(true); *r_s.borrow_mut() = vec![ns.clone()];
                s_state.set(vec![ns.clone()]); aid_state.set(Some(nid.clone()));
                set_editor_content(&final_text); set_gutter_status("unsaved"); 
                crate::js_interop::set_editor_mode(&filename); // モード適用
                focus_editor(); 
                let spr = sp_state.clone(); Timeout::new(100, move || { spr.set(false); }).forget();
                
                let os_cb = os_cb_outer.clone();
                spawn_local(async move {
                    let js = JSSheet { id: nid, guid: None, category: cat_id, title: filename, content: final_text, is_modified: true, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: ns.tab_color };
                    let ser = serde_wasm_bindgen::Serializer::json_compatible();
                    if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                    os_cb.emit(true);
                });
            }
            pending.set(None);
        })
    };

    let on_open_dialog = { let iv = is_file_open_dialog_visible.clone(); let sp = is_suppressing_changes.clone(); Callback::from(move |_| { sp.set(true); iv.set(true); }) };
    let on_help_cb = { let ih = is_help_visible.clone(); Callback::from(move |_| { ih.set(true); }) };

    // --- Effects ---

    {
        let is_auth = is_authenticated.clone();
        let is_ld = is_loading.clone(); let is_fo = is_fading_out.clone(); let is_init = is_initial_load.clone();
        use_effect_with((), move |_| {
            let is_auth_c = is_auth.clone();
            let t = Timeout::new(1500, move || { 
                if !*is_auth_c {
                    is_fo.set(true); let ild = is_ld.clone(); let is_init_inner = is_init.clone(); let ifo_inner = is_fo.clone();
                    Timeout::new(300, move || { ild.set(false); is_init_inner.set(false); ifo_inner.set(false); }).forget();
                }
            });
            move || { drop(t); }
        });
    }

    {
        let s_handle = sheets.clone(); let aid_handle = active_sheet_id.clone(); let cats_handle = categories.clone();
        let rs = sheets_ref.clone(); let db_loaded_init = db_loaded.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                if let Err(_) = crate::db_interop::init_db("LeafDB").await { gloo::console::error!("DB init failed"); }
                if let Ok(c_val) = crate::db_interop::load_categories().await {
                    if let Ok(loaded_cats) = serde_wasm_bindgen::from_value::<Vec<JSCategory>>(c_val) { cats_handle.set(loaded_cats); }
                }
                let mut initial = true;
                if let Ok(val) = crate::db_interop::load_sheets().await {
                    if let Ok(loaded) = serde_wasm_bindgen::from_value::<Vec<JSSheet>>(val) {
                        if !loaded.is_empty() {
                            let mapped: Vec<Sheet> = loaded.into_iter().map(|s| Sheet {
                                id: s.id, guid: s.guid, category: s.category, title: s.title, content: s.temp_content.clone().unwrap_or(s.content),
                                is_modified: s.temp_timestamp.is_some(), drive_id: s.drive_id, temp_content: s.temp_content, temp_timestamp: s.temp_timestamp,
                                last_sync_timestamp: s.last_sync_timestamp, tab_color: if s.tab_color.is_empty() { generate_random_color() } else { s.tab_color },
                            }).collect();
                            let last_id = mapped.last().map(|s| s.id.clone());
                            *rs.borrow_mut() = mapped.clone(); s_handle.set(mapped); aid_handle.set(last_id); initial = false;
                        }
                    }
                }
                if initial {
                    let nid = js_sys::Date::now().to_string();
                    let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color() };
                    *rs.borrow_mut() = vec![ns.clone()]; s_handle.set(vec![ns]); aid_handle.set(Some(nid));
                }
                db_loaded_init.set(true);
            });
            || ()
        });
    }

    {
        let is_auth = is_authenticated.clone(); let ncid = no_category_folder_id.clone();
        let ldid = leaf_data_folder_id.clone(); let cats_init = categories.clone();
        let client_id = client_id.clone(); let s_state = sheets.clone(); let rs = sheets_ref.clone();
        let ild_h = is_loading.clone(); let ifo_h = is_fading_out.clone(); let is_init_h = is_initial_load.clone();
        use_effect_with((), move |_| {
            let is_auth_cb = is_auth.clone(); let ncid_cb = ncid.clone(); let ldid_cb = ldid.clone();
            let cats_cb = cats_init.clone(); let s_state_cb = s_state.clone(); let rs_cb = rs.clone();
            let ild_cb = ild_h.clone(); let ifo_cb = ifo_h.clone(); let is_init_cb = is_init_h.clone();
            let callback = Closure::wrap(Box::new(move |_token: String| {
                let is_auth_inner = is_auth_cb.clone();
                if !*is_auth_inner {
                    is_auth_inner.set(true);
                    let ncid_i = ncid_cb.clone(); let ldid_i = ldid_cb.clone(); let cats_i = cats_cb.clone();
                    let s_inner = s_state_cb.clone(); let rs_inner = rs_cb.clone();
                    let ild_inner = ild_cb.clone(); let ifo_inner = ifo_cb.clone(); let is_init_inner = is_init_cb.clone();
                    let is_auth_err = is_auth_inner.clone();
                    spawn_local(async move {
                        match ensure_directory_structure().await {
                            Ok(res) => {
                                if let Ok(id_val) = js_sys::Reflect::get(&res, &JsValue::from_str("othersId")) {
                                    if let Some(id) = id_val.as_string() {
                                        ncid_i.set(Some(id.clone()));
                                        let mut us = (*s_inner).clone(); let mut changed = false;
                                        for s in us.iter_mut() { if s.category == "OTHERS" { s.category = id.clone(); changed = true; } }
                                        if changed { *rs_inner.borrow_mut() = us.clone(); s_inner.set(us); }
                                    }
                                }
                                if let Ok(id_val) = js_sys::Reflect::get(&res, &JsValue::from_str("leafDataId")) {
                                    if let Some(id) = id_val.as_string() {
                                        ldid_i.set(Some(id.clone()));
                                        let c_state = cats_i.clone();
                                        spawn_local(async move {
                                            if let Ok(c_res) = list_folders(&id).await {
                                                if let Ok(f_val) = js_sys::Reflect::get(&c_res, &JsValue::from_str("files")) {
                                                    let f_arr = js_sys::Array::from(&f_val); let mut n_cats = Vec::new();
                                                    for i in 0..f_arr.length() {
                                                        let v = f_arr.get(i);
                                                        let ci = js_sys::Reflect::get(&v, &JsValue::from_str("id")).unwrap().as_string().unwrap();
                                                        let cn = js_sys::Reflect::get(&v, &JsValue::from_str("name")).unwrap().as_string().unwrap();
                                                        n_cats.push(JSCategory { id: ci, name: cn });
                                                    }
                                                    if let Ok(v) = serde_wasm_bindgen::to_value(&n_cats) { let _ = save_categories(v).await; }
                                                    c_state.set(n_cats);
                                                }
                                            }
                                            ifo_inner.set(true);
                                            let ifo_final = ifo_inner.clone();
                                            Timeout::new(300, move || { 
                                                ild_inner.set(false); 
                                                is_init_inner.set(false); 
                                                ifo_final.set(false);
                                            }).forget();
                                        });
                                    }
                                }
                            },
                            Err(_) => { 
                                is_auth_err.set(false); 
                                ifo_inner.set(true); 
                                let ifo_final = ifo_inner.clone();
                                Timeout::new(300, move || { 
                                    ild_inner.set(false); 
                                    is_init_inner.set(false); 
                                    ifo_final.set(false);
                                }).forget();
                            },
                        }
                    });
                }
            }) as Box<dyn FnMut(String)>);
            crate::auth_interop::init_google_auth(&client_id, &callback); callback.forget(); || ()
        });
    }

    {
        let os = on_save_cb.clone(); let on = on_new_sheet_cb.clone();
        let oi = on_import_cb.clone(); 
        let ip = is_preview_visible.clone();
        let iv = is_file_open_dialog_visible.clone();
        let ih = is_help_visible.clone();
        let r_prev = is_preview_ref.clone();
        let r_open = is_file_open_ref.clone();
        let r_help = is_help_ref.clone();
        let is_auth = is_authenticated.clone(); let ast = auto_save_timer.clone(); let s_init = sheets.clone(); 
        let v_init = vim_mode.clone(); let ncid = no_category_folder_id.clone();
        let sp_init = is_suppressing_changes.clone(); let r_s = sheets_ref.clone(); let r_aid = active_id_ref.clone();
        let db_ready = db_loaded.clone();
        use_effect_with((is_auth, ncid.clone(), db_ready), move |deps| {
            let (auth, _, ready) = deps;
            if **auth && **ready {
                let os_i = os.clone(); let on_i = on.clone();
                let oi_i = oi.clone();
                let ip_i = ip.clone(); 
                let iv_i = iv.clone();
                let ih_i = ih.clone();
                let s_state = s_init.clone();
                let r_prev_i = r_prev.clone();
                let r_open_i = r_open.clone();
                let r_help_i = r_help.clone();
                let timer = ast.clone(); let vim_val = *v_init; 
                let sp_cb = sp_init.clone(); let r_s_i = r_s.clone(); let r_aid_i = r_aid.clone();
                let callback = Closure::wrap(Box::new(move |cmd: String| {
                    if cmd == "save" { os_i.emit(true); }
                    else if cmd == "new_sheet" { on_i.emit(()); }
                    else if cmd == "open" { 
                        let val = !*r_open_i.borrow();
                        iv_i.set(val); 
                        sp_cb.set(val); 
                    }
                    else if cmd == "import" { oi_i.emit(()); }
                    else if cmd == "preview" { 
                        let cur_c_val = get_editor_content();
                        let is_empty = cur_c_val.as_string().map(|s| s.trim().is_empty()).unwrap_or(true);
                        if !*r_prev_i.borrow() && is_empty {
                            return;
                        }
                        ip_i.set(!*r_prev_i.borrow()); 
                    }
                    else if cmd == "help" { ih_i.set(!*r_help_i.borrow()); }
                    else if cmd == "change" {
                        if *sp_cb { return; }
                        let cur_c_val = get_editor_content();
                        let cur_c = if let Some(s) = cur_c_val.as_string() { s } else { return; };
                        let aid = (*r_aid_i.borrow()).clone();
                        if let Some(id) = aid {
                            let mut cur_s = (*r_s_i.borrow()).clone();
                            let mut trigger_drive_sync = false; let mut needs_upd = false;
                            if let Some(sheet) = cur_s.iter_mut().find(|s| s.id == id) {
                                if sheet.content != cur_c { 
                                    sheet.content = cur_c.clone(); 
                                    sheet.is_modified = true; 
                                    needs_upd = true; 
                                    
                                    // IndexedDBへ即座に非同期保存
                                    let js = JSSheet { 
                                        id: sheet.id.clone(), guid: sheet.guid.clone(), category: sheet.category.clone(), 
                                        title: sheet.title.clone(), content: sheet.content.clone(), is_modified: true, 
                                        drive_id: sheet.drive_id.clone(), temp_content: sheet.temp_content.clone(), 
                                        temp_timestamp: sheet.temp_timestamp, last_sync_timestamp: sheet.last_sync_timestamp, 
                                        tab_color: sheet.tab_color.clone() 
                                    };
                                    spawn_local(async move {
                                        let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                        if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                    });
                                }
                                // ドライブ同期対象: カテゴリーが設定されている(クラウド) または ローカルファイル(Untitledでない)
                                trigger_drive_sync = !sheet.category.is_empty() || (sheet.category.is_empty() && !sheet.title.starts_with("Untitled"));

                                // 未保存シート（Untitled 且つ カテゴリーなし）の場合、即座に強制保存（クラウドへ）
                                if sheet.category.is_empty() && sheet.title.starts_with("Untitled") && needs_upd {
                                    let osa = os_i.clone();
                                    osa.emit(true);
                                }
                            }
                            if needs_upd { *r_s_i.borrow_mut() = cur_s.clone(); s_state.set(cur_s); }
                            if trigger_drive_sync && needs_upd { 
                                let osa = os_i.clone(); 
                                timer.set(Some(Timeout::new(1000, move || { osa.emit(false); }))); 
                            }
                        }
                    }
                }) as Box<dyn FnMut(String)>);
                init_editor("editor", &callback); set_vim_mode(vim_val); callback.forget();
            }
            || ()
        });
    }

    {
        let aid = active_sheet_id.clone(); let is_ld = is_loading.clone();
        let s_handle = sheets.clone(); let db_ready = db_loaded.clone();
        use_effect_with((aid, is_ld, s_handle, db_ready), move |deps| {
            let (aid_val, ld_val, s_val, ready_val) = deps;
            if **ready_val && !**ld_val { 
                if let Some(id) = &**aid_val { 
                    if let Some(s) = s_val.iter().find(|x| x.id == *id) { 
                        set_editor_content(&s.content); 
                        let mode = if s.category.is_empty() {
                            if s.title.starts_with("Untitled") { "unsaved" } else { "local" }
                        } else if s.drive_id.is_none() {
                            "unsaved"
                        } else {
                            "none"
                        };
                        set_gutter_status(mode);
                        crate::js_interop::set_editor_mode(&s.title);
                        focus_editor(); 
                    } 
                } 
            }
            || ()
        });
    }

    {
        let is_preview = is_preview_visible.clone();
        use_effect_with(*is_preview, move |visible| { set_preview_active(*visible); || () });
    }

    {
        let is_auth = is_authenticated.clone();
        let is_ld = is_loading.clone();
        let is_file_open = is_file_open_dialog_visible.clone();
        let is_prev = is_preview_visible.clone();
        let is_help = is_help_visible.clone();
        let is_logout_conf = is_logout_confirm_visible.clone();
        let has_del = pending_delete_category.clone();
        let has_conf = conflict_queue.clone();
        let has_nc = name_conflict_queue.clone();
        let has_fall = fallback_queue.clone();
        let has_imp = pending_import_data.clone();
        let is_imp_lock = is_import_lock.clone();
        let is_drop = is_category_dropdown_open.clone();
        let last_obscured = use_state(|| true);

        use_effect_with(
            ((is_auth, is_ld, is_file_open, is_prev, is_help, is_logout_conf), (has_del, has_conf, has_nc, has_fall, has_imp, is_imp_lock, is_drop)),
            move |deps| {
                let ((auth, ld, file_open, prev, help, logout_conf), (del, conf, nc, fall, imp, imp_lock, drop_open)) = deps;
                let obscured = !**auth || **ld || **file_open || **prev || **help || **logout_conf || (*del).is_some() || !(*conf).is_empty() || !(*nc).is_empty() || !(*fall).is_empty() || (*imp).is_some() || **imp_lock || **drop_open;
                if *last_obscured && !obscured {
                    focus_editor();
                }
                last_obscured.set(obscured);
                || ()
            }
        );
    }

    {
                let is_auth = is_authenticated.clone(); let is_file_open = is_file_open_dialog_visible.clone();
                let is_preview = is_preview_visible.clone(); 
                let is_help = is_help_visible.clone();
                let pending_del = pending_delete_category.clone();
                let conflicts = conflict_queue.clone(); let fallbacks = fallback_queue.clone(); let sp = is_suppressing_changes.clone();
                let pending_imp = pending_import_data.clone();
                let is_logout_conf = is_logout_confirm_visible.clone();
                let ncq_esc = name_conflict_queue.clone();
                let is_imp_lock = is_import_lock.clone();
                let oi_cb = on_import_cb.clone();
                let is_drop_ev = is_category_dropdown_open.clone();
                let is_dialog_prev = is_dialog_preview_open.clone();
                let is_ld_ev = is_loading.clone();
                let is_fo_ev = is_fading_out.clone();
                
                use_effect_with((*is_auth, (*is_file_open, *is_preview, *is_help, *is_logout_conf, *is_imp_lock, *is_drop_ev, *is_dialog_prev, *is_ld_ev, *is_fo_ev), ((*pending_del).is_some(), !(*conflicts).is_empty(), !(*fallbacks).is_empty(), (*pending_imp).is_some(), !(*ncq_esc).is_empty())), move |deps| {
                    let (auth, (file_open, preview, help, logout_conf, imp_lock, drop_open, dialog_prev, is_loading, is_fading_out), (has_del, has_conf, has_fall, has_imp, has_nc)) = *deps;
                    if !auth { return Box::new(|| ()) as Box<dyn FnOnce()>; }
                    
                    let window = web_sys::window().unwrap();
                    let is_file_open_c = is_file_open.clone(); 
                    let is_preview_c = is_preview.clone();
                    let is_help_c = is_help.clone();
                    let pending_del_c = pending_del.clone(); 
                    let conflicts_c = conflicts.clone();
                    let fallbacks_c = fallbacks.clone(); 
                    let sp_c = sp.clone();
                    let pending_imp_c = pending_imp.clone();
                    let is_logout_conf_c = is_logout_conf.clone();
                    let ncq_esc_c = ncq_esc.clone();
                    let oi_c = oi_cb.clone();
                    let is_drop_c = is_drop_ev.clone();
                    
                    let mut opts = EventListenerOptions::run_in_capture_phase();
                    opts.passive = false;
                    let listener = EventListener::new_with_options(&window, "keydown", opts, move |e| {
                        let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                        let key = ke.key();
                        let code = ke.code();
                        
                        let is_dialog_open = file_open || preview || help || has_del || has_conf || has_fall || has_imp || logout_conf || has_nc || drop_open || is_loading || is_fading_out;
                        let is_overlay_active = is_dialog_open || imp_lock;

                        if is_loading || is_fading_out {
                            e.prevent_default(); e.stop_immediate_propagation();
                            return;
                        }

                        if ke.alt_key() {
                            let key_lower = key.to_lowercase();
                            // デバッグログ: Altキー押下時の全イベントを記録
                            gloo::console::debug!(format!("[Leaf-KEYS] Alt detected: key='{}', code='{}'", key, code));

                            if !is_overlay_active {
                                let is_l = code == "KeyL" || key_lower == "l" || key_lower == "¬";
                                let is_m = code == "KeyM" || key_lower == "m" || key_lower == "µ";
                                let is_h = code == "KeyH" || key_lower == "h" || key_lower == "˙";
                                let is_o = code == "KeyO" || key_lower == "o" || key_lower == "ø";
                                let is_f = code == "KeyF" || key_lower == "f" || key_lower == "ƒ";
                                let is_s = code == "KeyS" || key_lower == "s" || key_lower == "ß";
                                let is_n = code == "KeyN" || key_lower == "n" || key_lower == "˜";

                                if is_l {
                                    e.prevent_default(); e.stop_immediate_propagation();
                                    // テキストが空の場合はプレビューを開かない
                                    let cur_c_val = get_editor_content();
                                    let is_empty = cur_c_val.as_string().map(|s| s.trim().is_empty()).unwrap_or(true);
                                    
                                    if !*is_preview_c && is_empty {
                                        gloo::console::log!("[Leaf-KEYS] Preview suppressed: content is empty");
                                        return;
                                    }

                                    gloo::console::log!("[Leaf-KEYS] Toggling Preview (Alt+L)");
                                    is_preview_c.set(!*is_preview_c);
                                    return;
                                }
                                if is_m {
                                    e.prevent_default(); e.stop_immediate_propagation();
                                    gloo::console::log!("[Leaf-KEYS] Toggling Sheet Selection (Alt+M)");
                                    let val = !*is_file_open_c; is_file_open_c.set(val); sp_c.set(val);
                                    return;
                                }
                                if is_h {
                                    e.prevent_default(); e.stop_immediate_propagation();
                                    gloo::console::log!("[Leaf-KEYS] Toggling Help (Alt+H)");
                                    is_help_c.set(!*is_help_c);
                                    return;
                                }
                                
                                if is_o { e.prevent_default(); e.stop_immediate_propagation(); oi_c.emit(()); return; }
                                if is_f { e.prevent_default(); e.stop_immediate_propagation(); crate::js_interop::focus_editor(); crate::js_interop::exec_editor_command("find"); return; }
                                if is_s { e.prevent_default(); e.stop_immediate_propagation(); crate::js_interop::exec_editor_command("saveSheet"); return; }
                                if is_n { e.prevent_default(); e.stop_immediate_propagation(); crate::js_interop::exec_editor_command("newSheet"); return; }
                            }
                        }

                        if is_overlay_active {
                            let target = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok());
                            let is_target_in_editor = target.as_ref().map(|t| t.closest("#editor").unwrap_or(None).is_some()).unwrap_or(false);
                            let is_target_body = target.as_ref().map(|t| t.tag_name().to_lowercase() == "body").unwrap_or(false);

                            if key == "Escape" {
                                if dialog_prev { return; }

                                e.stop_immediate_propagation();
                                e.prevent_default();
                                if drop_open { is_drop_c.set(false); }
                                else if logout_conf { is_logout_conf_c.set(false); }
                                else if has_nc { ncq_esc_c.set(Vec::new()); }
                                else if has_del { pending_del_c.set(None); }
                                else if has_conf { conflicts_c.set(Vec::new()); }
                                else if has_fall { fallbacks_c.set(Vec::new()); }
                                else if has_imp { pending_imp_c.set(None); }
                                else if preview { is_preview_c.set(false); }
                                else if help { is_help_c.set(false); }
                                else if file_open { is_file_open_c.set(false); sp_c.set(false); }
                                
                                focus_editor();
                                return;
                            }

                            if is_dialog_open {
                                if is_target_in_editor || is_target_body {
                                    if ke.alt_key() || ke.ctrl_key() || ke.meta_key() || key.len() == 1 {
                                        e.stop_immediate_propagation();
                                        e.prevent_default();
                                    }
                                }
                            } else if imp_lock {
                                if is_target_in_editor || is_target_body {
                                    e.stop_immediate_propagation();
                                    e.prevent_default();
                                }
                            }
                        }
                    });
                    Box::new(move || drop(listener)) as Box<dyn FnOnce()>
        });
    }

    let current_cat = active_sheet_id.as_ref().and_then(|id| sheets.iter().find(|s| s.id == *id)).map(|s| s.category.clone()).unwrap_or_else(|| (*no_category_folder_id).clone().unwrap_or_else(|| "NO_CATEGORY".to_string()));
    
    let (current_cat_name, current_file_name, current_file_ext) = if let Some(aid) = active_sheet_id.as_ref() {
        let rs = sheets_ref.borrow();
        if let Some(sheet) = rs.iter().find(|s| s.id == *aid) {
            let cn = if sheet.category.is_empty() {
                "".to_string()
            } else {
                categories.iter()
                    .find(|c| c.id == sheet.category)
                    .map(|c| if c.name == "OTHERS" { i18n::t("OTHERS", lang) } else { c.name.clone() })
                    .unwrap_or_else(|| i18n::t("OTHERS", lang))
            };
            let mut file_name = sheet.title.clone();
            let mut file_ext = file_name.split('.').last().unwrap_or("txt").to_string();
            if file_name.starts_with("Untitled") {
                file_name = "----".to_string();
                file_ext = "txt".to_string();
            }
            (cn, file_name, file_ext)
        } else {
            ("".to_string(), "".to_string(), "txt".to_string())
        }
    } else {
        ("".to_string(), "".to_string(), "txt".to_string())
    };

    let is_current_new_sheet = if let Some(aid) = active_sheet_id.as_ref() {
        let rs = sheets_ref.borrow();
        rs.iter().find(|s| s.id == *aid).map(|s| s.title.starts_with("Untitled")).unwrap_or(false)
    } else {
        false
    };

    html! {
        <div class="relative h-screen w-screen overflow-hidden bg-gray-950" key="app-root">
            <main 
                key="main-editor-surface"
                class={classes!(
                    "absolute", "inset-0", "flex", "flex-col", "text-white", "transition-opacity", "duration-300",
                    if !*is_authenticated { "opacity-0" } else { "opacity-100" }
                )}
            >
                <ButtonBar 
                    key="top-button-bar"
                    on_new_sheet={on_new_sheet_cb.clone()} 
                    on_open={on_open_dialog} 
                    on_import={on_import_cb} 
                    on_change_font_size={on_change_font_size} 
                    on_change_category={on_change_category_cb} 
                    on_help={on_help_cb}
                    on_logout={on_logout}
                    current_category={current_cat} 
                    categories={(*categories).clone()} 
                    is_new_sheet={is_current_new_sheet}
                    is_dropdown_open={*is_category_dropdown_open}
                    on_toggle_dropdown={let id = is_category_dropdown_open.clone(); Callback::from(move |v| id.set(v))}
                />
                <div 
                    id="editor" 
                    key="ace-editor-fixed-node" 
                    class="flex-1 bg-gray-950 z-10" 
                    style="width: 100%; min-height: 0;"
                ></div>
                <StatusBar 
                    key="bottom-status-bar" 
                    network_status={*network_connected} 
                    is_saving={*is_saving}
                    vim_mode={*vim_mode}
                    on_toggle_vim={on_toggle_vim}
                    category_name={current_cat_name}
                    file_name={current_file_name}
                    file_extension={current_file_ext}
                    on_change_extension={on_change_extension_cb}
                    version={env!("CARGO_PKG_VERSION").to_string()} 
                />
            </main>

            <div id="overlays-layer" class="pointer-events-none fixed inset-0 z-[100]">
                if !*is_authenticated {
                    <div class="pointer-events-auto fixed inset-0 flex items-center justify-center bg-gray-900 overflow-y-auto p-4">
                        <div class="text-center max-w-2xl">
                            <img src="icon.svg" class="mx-auto mb-8 shadow-2xl" style="width: 15vmin; height: 15vmin;" alt="Leaf Icon" />
                            <h1 class="text-4xl font-extrabold text-white mb-6 tracking-tight">
                                { i18n::t("welcome_headline", lang) }
                            </h1>
                            <div class="mb-10 text-gray-300 text-sm leading-relaxed whitespace-pre-wrap opacity-80 bg-gray-800/30 p-6 rounded-lg border border-white/5 shadow-inner text-left">
                                { Html::from_html_unchecked(i18n::t("app_policy_description", lang).into()) }
                            </div>
                            <button onclick={on_login} class="bg-blue-600 hover:bg-blue-700 text-white font-bold py-3 px-8 rounded-md transition-colors shadow-lg text-lg">
                                { i18n::t("signin_with_google", lang) }
                            </button>
                            <div class="mt-6 text-gray-500 text-xs">{ i18n::t("login_required", lang) }</div>
                        </div>
                    </div>
                }

                if *is_file_open_dialog_visible {
                    if let Some(ldid) = (*leaf_data_folder_id).clone() {
                        <div class="pointer-events-auto">
                            <FileOpenDialog 
                                on_close={
                                    let iv = is_file_open_dialog_visible.clone(); 
                                    let sp = is_suppressing_changes.clone(); 
                                    let aid = active_id_ref.clone();
                                    let rs = sheets_ref.clone();
                                    let s_state = sheets.clone();
                                    move |_| { 
                                        iv.set(false); sp.set(false); focus_editor(); 
                                        
                                        // 現在のシートがドライブに存在するか非同期で確認
                                        let aid_val = (*aid.borrow()).clone();
                                        let rs_c = rs.clone();
                                        let s_state_c = s_state.clone();
                                        if let Some(id) = aid_val {
                                            let sheets_list = (*rs_c.borrow()).clone();
                                            if let Some(sheet) = sheets_list.iter().find(|s| s.id == id) {
                                                // ローカルファイル（カテゴリー空）でない場合のみ、ドライブ上の存在を確認
                                                if !sheet.category.is_empty() {
                                                    if let Some(did) = sheet.drive_id.clone() {
                                                        let sheet_id = id.clone();
                                                        spawn_local(async move {
                                                            if let Err(_) = crate::drive_interop::get_file_metadata(&did).await {
                                                                // ファイルが見つからない場合、未保存シートへダウングレード
                                                                let mut us = (*rs_c.borrow()).clone();
                                                                if let Some(s) = us.iter_mut().find(|x| x.id == sheet_id) {
                                                                    s.drive_id = None;
                                                                    s.category = "OTHERS".to_string();
                                                                    s.is_modified = true;
                                                                    set_gutter_status("unsaved");
                                                                    
                                                                    let js = JSSheet { 
                                                                        id: s.id.clone(), guid: s.guid.clone(), category: s.category.clone(), 
                                                                        title: s.title.clone(), content: s.content.clone(), is_modified: true, 
                                                                        drive_id: None, temp_content: s.temp_content.clone(), 
                                                                        temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, 
                                                                        tab_color: s.tab_color.clone() 
                                                                    };
                                                                    let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                                                    if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                                                }
                                                                *rs_c.borrow_mut() = us.clone(); s_state_c.set(us);
                                                            }
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } 
                                on_select={on_file_sel_cb} 
                                leaf_data_id={ldid} 
                                categories={(*categories).clone()} 
                                on_refresh={on_refresh_cats_cb}
                                on_delete_category={on_delete_category_cb}
                                on_rename_category={on_rename_category_cb}
                                on_start_processing={let lmk = loading_message_key.clone(); move |_| { lmk.set("synchronizing"); }} 
                                on_preview_toggle={let idp = is_dialog_preview_open.clone(); Callback::from(move |v| idp.set(v))}
                            />
                        </div>
                    }
                }

                if let Some(preview) = if *is_preview_visible {
                    let aid = (*active_sheet_id).clone();
                    let c = if let Some(id) = aid { sheets.iter().find(|s| s.id == id).map(|s| s.content.clone()).unwrap_or_default() } else { "".to_string() };
                    let iv = is_preview_visible.clone();
                    Some(html! { <Preview content={c} on_close={Callback::from(move |_| { iv.set(false); focus_editor(); })} /> })
                } else if *is_help_visible {
                    let ih = is_help_visible.clone();
                    let c = i18n::t("help_shortcuts", lang);
                    let is_conf = is_install_confirm_visible.clone();
                    let is_man = is_install_manual_visible.clone();
                    let ih_for_install = ih.clone();
                    let on_install = Callback::from(move |_: ()| {
                        ih_for_install.set(false);
                        if crate::js_interop::can_install_pwa() {
                            is_conf.set(true);
                        } else {
                            is_man.set(true);
                        }
                    });
                    Some(html! { <Preview content={c} on_close={Callback::from(move |_| { ih.set(false); focus_editor(); })} on_install={on_install} /> })
                } else { None } { <div class="pointer-events-auto">{ preview }</div> }

                if *is_install_confirm_visible {
                    <div class="pointer-events-auto">
                        <ConfirmDialog 
                            title={i18n::t("install_title", lang)}
                            message={i18n::t("install_confirm", lang)}
                            on_confirm={
                                let ic = is_install_confirm_visible.clone();
                                move |_| {
                                    ic.set(false);
                                    spawn_local(async move {
                                        crate::js_interop::trigger_pwa_install().await;
                                    });
                                }
                            }
                            on_cancel={let ic = is_install_confirm_visible.clone(); move |_| ic.set(false)}
                        />
                    </div>
                }

                if *is_install_manual_visible {
                    <div class="pointer-events-auto">
                        <ConfirmDialog 
                            title={i18n::t("install_manual_title", lang)}
                            message={i18n::t("install_manual_message", lang)}
                            ok_label={"OK"}
                            on_confirm={let im = is_install_manual_visible.clone(); move |_| im.set(false)}
                            on_cancel={let im = is_install_manual_visible.clone(); move |_| im.set(false)}
                        />
                    </div>
                }

                if let Some(del_diag) = if let Some(_) = *pending_delete_category {
                    let title = i18n::t("delete", lang); let message = i18n::t("confirm_delete_category", lang);
                    let pending = pending_delete_category.clone(); let on_cfm = on_delete_category_cfm.clone();
                    Some(html! { <ConfirmDialog title={title} message={message} on_confirm={move |_| { on_cfm.emit(1); }} on_cancel={move |_| { pending.set(None); }} /> })
                } else { None } { <div class="pointer-events-auto">{ del_diag }</div> }

                if let Some(conf_diag) = if !conflict_queue.is_empty() {
                    let conflict = conflict_queue.first().unwrap();
                    let title = if conflict.is_missing_on_drive { i18n::t("file_not_found", lang) } else { i18n::t("conflict_detected", lang) };
                    let message = if conflict.is_missing_on_drive { i18n::t("missing_file_message", lang).replace("{}", &conflict.title) } else { i18n::t("conflict_message", lang).replace("{}", &conflict.title) };
                    let options = if conflict.is_missing_on_drive { vec![DialogOption { id: 1, label: i18n::t("opt_reupload", lang) }, DialogOption { id: 3, label: i18n::t("opt_delete_local", lang) }] } else { vec![DialogOption { id: 0, label: i18n::t("opt_load_drive", lang) }, DialogOption { id: 1, label: i18n::t("opt_overwrite_drive", lang) }, DialogOption { id: 2, label: i18n::t("opt_save_new", lang) }] };
                    let cq = conflict_queue.clone(); let on_cfm = on_conf_cfm.clone();
                    Some(html! { <CustomDialog title={title} message={message} options={options} on_confirm={on_cfm} on_cancel={let cq = cq.clone(); Some(Callback::from(move |_| { cq.set(Vec::new()); }))} /> })
                } else { None } { <div class="pointer-events-auto">{ conf_diag }</div> }

                if let Some(fb_alert) = if let Some(_) = fallback_queue.first() {
                    let fq = fallback_queue.clone(); let on_cfm = on_fallback_cfm.clone();
                    Some(html! { <CustomDialog title={i18n::t("category_not_found_title", lang)} message={i18n::t("category_not_found_fallback", lang)} options={vec![DialogOption { id: 0, label: "OK".to_string() }]} on_confirm={on_cfm} on_cancel={let fq = fq.clone(); Some(Callback::from(move |_| { fq.set(Vec::new()); }))} /> })
                } else { None } { <div class="pointer-events-auto">{ fb_alert }</div> }

                if let Some(nc_diag) = if !name_conflict_queue.is_empty() {
                    let conflict = name_conflict_queue.first().unwrap();
                    let title = i18n::t("filename_conflict", lang);
                    let message = i18n::t("filename_conflict_message", lang).replace("{}", &conflict.filename);
                    let on_cfm = on_name_conflict_cfm.clone();
                    let ncq = name_conflict_queue.clone();
                    let labels = vec![i18n::t("opt_nc_overwrite", lang), i18n::t("opt_nc_new_guid", lang), i18n::t("opt_nc_rename", lang)];
                    Some(html! { <NameConflictDialog title={title} message={message} current_name={conflict.filename.clone()} labels={labels} on_confirm={on_cfm} on_cancel={move |_| { ncq.set(Vec::new()); }} /> })
                } else { None } { <div class="pointer-events-auto">{ nc_diag }</div> }

                if let Some(import_diag) = if let Some(_) = (*pending_import_data).clone() {
                    let on_cfm = on_confirm_import.clone();
                    let pending = pending_import_data.clone();
                    Some(html! { <ConfirmDialog title={i18n::t("confirm_conversion", lang)} message={i18n::t("confirm_conversion", lang)} on_confirm={let on_c = on_cfm.clone(); move |_| on_c.emit(true)} on_cancel={move |_| pending.set(None)} /> })
                } else { None } { <div class="pointer-events-auto">{ import_diag }</div> }

                if *is_import_lock {
                    <div class={classes!(
                        "pointer-events-auto", "fixed", "inset-0", "bg-black/50", "backdrop-blur-md", "z-[90]", "transition-opacity", "duration-300",
                        "flex", "items-center", "justify-center",
                        if *is_import_fading_out { "opacity-0" } else { "opacity-100" }
                    )}>
                        <div class="flex flex-col items-center">
                            <div class="w-12 h-12 border-4 border-lime-500 border-t-transparent rounded-full animate-spin"></div>
                            <p class="mt-4 text-white font-bold text-lg animate-pulse">{ i18n::t("synchronizing", lang) }</p>
                        </div>
                    </div>
                }

                if *is_loading {
                    <div class={classes!("fixed", "inset-0", "z-[200]", "flex", "items-center", "justify-center", "bg-gray-900", "transition-opacity", "duration-300", "pointer-events-auto", if *is_fading_out { "opacity-0" } else { "opacity-100" } )}>
                        <div class="flex flex-col items-center">
                            if *is_initial_load { <img src="icon.svg" class="mb-8 shadow-2xl animate-in fade-in zoom-in duration-500" style="width: 20vmin; height: 20vmin;" alt="Leaf Icon" /> }
                            <div class="w-12 h-12 border-4 border-green-500 border-t-transparent rounded-full animate-spin"></div>
                            if *is_authenticated { <p class="mt-4 text-white font-bold text-lg animate-pulse">{ i18n::t(*loading_message_key, lang) }</p> }
                        </div>
                    </div>
                }
                
                if *is_logout_confirm_visible {
                    <div class="pointer-events-auto">
                        <ConfirmDialog 
                            title={i18n::t("logout", lang)} 
                            message={i18n::t("confirm_logout", lang)} 
                            on_confirm={
                                let ic = is_logout_confirm_visible.clone();
                                let il = is_loading.clone();
                                let lmk = loading_message_key.clone();
                                let ifo = is_fading_out.clone();
                                move |_| { 
                                    ic.set(false); 
                                    lmk.set("logging_out");
                                    il.set(true);
                                    ifo.set(false);
                                    
                                    spawn_local(async move {
                                        crate::auth_interop::sign_out();
                                        // ユーザーにログアウト中であることを示すために少し待つ
                                        Timeout::new(800, move || {
                                            web_sys::window().unwrap().location().reload().unwrap();
                                        }).forget();
                                    });
                                }
                            } 
                            on_cancel={let ic = is_logout_confirm_visible.clone(); move |_| ic.set(false)} 
                        />
                    </div>
                }
            </div>
        </div>
    }
}
