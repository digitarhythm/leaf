use yew::prelude::*;
use crate::components::button_bar::ButtonBar;
use crate::components::status_bar::StatusBar;
use crate::components::dialog::{CustomDialog, DialogOption};
use crate::components::file_open_dialog::FileOpenDialog;
use crate::js_interop::{init_editor, set_vim_mode, resize_editor, get_editor_content, set_editor_content, focus_editor, set_window_title, generate_uuid, set_gutter_status};
use crate::auth_interop::{init_google_auth, request_access_token};
use crate::db_interop::{init_db, save_sheet, load_sheets, save_categories, load_categories, JSCategory, JSSheet};
use crate::drive_interop::{upload_file, ensure_directory_structure, list_folders};
use crate::i18n::{self, Language};
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::spawn_local;
use gloo::events::EventListener;
use gloo::timers::callback::Timeout;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

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
    app_name: String,
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
    let app_name = config.app_name;
    
    let client_id = option_env!("LEAF_CLIENTID")
        .map(|s| s.to_string())
        .unwrap_or_else(|| config.google_client_id);

    let vim_mode = use_state(|| true);
    let sheets = use_state(|| Vec::<Sheet>::new());
    let active_sheet_id = use_state(|| None::<String>);
    let network_connected = use_state(|| true);
    let is_authenticated = use_state(|| false);
    let no_category_folder_id = use_state(|| None::<String>);
    let leaf_data_folder_id = use_state(|| None::<String>);
    let auto_save_timer = use_state(|| None::<Timeout>);
    let is_loading = use_state(|| true);
    let loading_message_key = use_state(|| "synchronizing");
    let is_fading_out = use_state(|| false);
    let min_time_passed = use_state(|| false);
    let is_file_open_dialog_visible = use_state(|| false);
    let is_suppressing_changes = use_state(|| false); 
    let categories = use_state(|| Vec::<JSCategory>::new());
    let db_loaded = use_state(|| false);
    
    let conflict_queue = use_state(|| Vec::<ConflictData>::new());

    let sheets_ref = use_mut_ref(|| Vec::<Sheet>::new());
    let active_id_ref = use_mut_ref(|| None::<String>);
    let no_category_id_ref = use_mut_ref(|| None::<String>);
    let is_loading_ref = use_mut_ref(|| true);
    let is_saving_ref = use_mut_ref(|| false);
    let is_suppressing_ref = use_mut_ref(|| false);

    // Ref sync
    {
        let s = sheets.clone();
        let aid = active_sheet_id.clone();
        let ncid = no_category_folder_id.clone();
        let ld = is_loading.clone();
        let sp = is_suppressing_changes.clone();
        let r_s = sheets_ref.clone();
        let r_aid = active_id_ref.clone();
        let r_ncid = no_category_id_ref.clone();
        let r_ld = is_loading_ref.clone();
        let r_sp = is_suppressing_ref.clone();
        use_effect_with(((*s).clone(), (*aid).clone(), (*ncid).clone(), *ld, *sp), move |deps| {
            let (s_val, aid_val, ncid_val, ld_val, sp_val) = deps;
            *r_s.borrow_mut() = s_val.clone();
            *r_aid.borrow_mut() = aid_val.clone();
            *r_ncid.borrow_mut() = ncid_val.clone();
            *r_ld.borrow_mut() = *ld_val;
            *r_sp.borrow_mut() = *sp_val;
            || ()
        });
    }

    // Timers & Title
    {
        let min_time_passed = min_time_passed.clone();
        let is_auth = is_authenticated.clone();
        let is_ld = is_loading.clone();
        let is_fo = is_fading_out.clone();
        use_effect_with((), move |_| {
            let timeout = Timeout::new(1500, move || { 
                min_time_passed.set(true); 
                // 1.5秒経過しても未認証の場合は、ログイン画面を表示するためにローディングを解除
                if !*is_auth {
                    is_fo.set(true);
                    let ild = is_ld.clone();
                    Timeout::new(300, move || { ild.set(false); }).forget();
                }
            });
            move || { drop(timeout); }
        });
    }
    {
        let app_name = app_name.clone();
        use_effect_with((), move |_| { set_window_title(&app_name); || () });
    }

    // DB Initialization
    {
        let sheets = sheets.clone();
        let aid = active_sheet_id.clone();
        let cats = categories.clone();
        let r_s = sheets_ref.clone();
        let r_aid = active_id_ref.clone();
        let db_name = format!("{}DB", app_name);
        let db_loaded_init = db_loaded.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                if let Err(_) = init_db(&db_name).await { gloo::console::error!("DB init failed"); }
                if let Ok(c_val) = load_categories().await {
                    if let Ok(loaded_cats) = serde_wasm_bindgen::from_value::<Vec<JSCategory>>(c_val) { cats.set(loaded_cats); }
                }
                let mut initial_needed = true;
                if let Ok(val) = load_sheets().await {
                    if let Ok(loaded_sheets) = serde_wasm_bindgen::from_value::<Vec<JSSheet>>(val) {
                        if !loaded_sheets.is_empty() {
                            let mut mapped = Vec::new();
                            for s in loaded_sheets {
                                let content = s.temp_content.clone().unwrap_or(s.content.clone());
                                mapped.push(Sheet {
                                    id: s.id, guid: s.guid, category: s.category, title: s.title, content,
                                    is_modified: s.temp_timestamp.is_some(), drive_id: s.drive_id,
                                    temp_content: s.temp_content, temp_timestamp: s.temp_timestamp,
                                    last_sync_timestamp: s.last_sync_timestamp, tab_color: if s.tab_color.is_empty() { generate_random_color() } else { s.tab_color },
                                });
                            }
                            let last_id = mapped.last().map(|s| s.id.clone());
                            *r_s.borrow_mut() = mapped.clone();
                            *r_aid.borrow_mut() = last_id.clone();
                            aid.set(last_id);
                            sheets.set(mapped);
                            initial_needed = false;
                        }
                    }
                }
                if initial_needed {
                    let new_id = js_sys::Date::now().to_string();
                    let new_sheet = Sheet {
                        id: new_id.clone(), guid: None, category: "NO_CATEGORY".to_string(), title: "Untitled 1".to_string(),
                        content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None,
                        last_sync_timestamp: None, tab_color: generate_random_color(),
                    };
                    *r_s.borrow_mut() = vec![new_sheet.clone()];
                    *r_aid.borrow_mut() = Some(new_id.clone());
                    sheets.set(vec![new_sheet]);
                    aid.set(Some(new_id));
                }
                db_loaded_init.set(true);
            });
            || ()
        });
    }

    // Auth Initialization
    {
        let is_auth = is_authenticated.clone();
        let ncid = no_category_folder_id.clone();
        let ldid = leaf_data_folder_id.clone();
        let cats_init = categories.clone();
        let client_id = client_id.clone();
        use_effect_with((), move |_| {
            let is_auth_cb = is_auth.clone();
            let ncid_cb = ncid.clone();
            let ldid_cb = ldid.clone();
            let cats_cb = cats_init.clone();
            let callback = Closure::wrap(Box::new(move |_token: String| {
                if !*is_auth_cb {
                    is_auth_cb.set(true);
                    let ncid_i = ncid_cb.clone(); let ldid_i = ldid_cb.clone(); let cats_i = cats_cb.clone();
                    let is_auth_err = is_auth_cb.clone();
                    spawn_local(async move {
                        match ensure_directory_structure().await {
                            Ok(res) => {
                                if let Ok(id_val) = js_sys::Reflect::get(&res, &JsValue::from_str("noCategoryId")) { if let Some(id) = id_val.as_string() { ncid_i.set(Some(id)); } }
                                if let Ok(id_val) = js_sys::Reflect::get(&res, &JsValue::from_str("leafDataId")) {
                                    if let Some(id) = id_val.as_string() {
                                        ldid_i.set(Some(id.clone()));
                                        let c_state = cats_i.clone();
                                        spawn_local(async move {
                                            if let Ok(c_res) = list_folders(&id).await {
                                                if let Ok(f_val) = js_sys::Reflect::get(&c_res, &JsValue::from_str("files")) {
                                                    let f_arr = js_sys::Array::from(&f_val);
                                                    let mut n_cats = Vec::new();
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
                                        });
                                    }
                                }
                            },
                            Err(_) => { is_auth_err.set(false); },
                        }
                    });
                }
            }) as Box<dyn FnMut(String)>);
            init_google_auth(&client_id, &callback); callback.forget(); || ()
        });
    }

    // Network Monitoring
    {
        let network_connected = network_connected.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let on_online = { let nc = network_connected.clone(); EventListener::new(&window, "online", move |_| { nc.set(true); }) };
            let on_offline = { let nc = network_connected.clone(); EventListener::new(&window, "offline", move |_| { nc.set(false); }) };
            network_connected.set(window.navigator().on_line());
            move || { drop(on_online); drop(on_offline); }
        });
    }

    // Conflict Resolution
    let conflict_checked = use_state(|| false);
    {
        let sheets = sheets.clone();
        let is_auth = *is_authenticated;
        let ncid = no_category_folder_id.clone();
        let checked = conflict_checked.clone();
        let is_ld = is_loading.clone();
        let is_fo = is_fading_out.clone();
        let min_p = min_time_passed.clone();
        let aid = active_sheet_id.clone();
        let cq = conflict_queue.clone();
        let db_ready = *db_loaded;
        use_effect_with((is_auth, ncid.clone(), sheets.len(), (*aid).clone(), db_ready), move |deps| {
            let (auth, folder, count, _, ready) = deps;
            if *auth && folder.is_some() && !*checked && *ready {
                let s_state = sheets.clone(); let c_checked = checked.clone(); let ild = is_ld.clone();
                let ifo_c = is_fo.clone(); let min = min_p.clone(); let q = cq.clone();
                
                let sheet_count = *count;
                spawn_local(async move {
                    gloo::console::log!("[Conflict] Starting check for", sheet_count, "sheets...");
                    let s_val = (*s_state).clone(); 
                    let mut conflicts = Vec::new();
                    let mut updated_sheets = s_val.clone();
                    let mut needs_db_update = false;

                    for (idx, s) in s_val.iter().enumerate() {
                        if let Some(did) = &s.drive_id {
                            match crate::drive_interop::get_file_metadata(did).await {
                                Ok(meta) => {
                                    let is_trashed = js_sys::Reflect::get(&meta, &wasm_bindgen::JsValue::from_str("trashed")).unwrap_or(wasm_bindgen::JsValue::FALSE).as_bool().unwrap_or(false);
                                    
                                    if is_trashed {
                                        gloo::console::warn!("[Conflict] Detected (Trashed on Drive):", &s.title);
                                        conflicts.push(ConflictData { sheet_id: s.id.clone(), title: s.title.clone(), drive_id: did.clone(), local_content: s.content.clone(), drive_time: 0, time_str: "".to_string(), is_missing_on_drive: true });
                                    } else {
                                        let lt = s.temp_timestamp.or(s.last_sync_timestamp).unwrap_or(0);
                                        if let Ok(tv) = js_sys::Reflect::get(&meta, &wasm_bindgen::JsValue::from_str("modifiedTime")) {
                                            if let Some(ts) = tv.as_string() {
                                                let dt = crate::drive_interop::parse_date(&ts) as u64;
                                                if dt > lt && (dt - lt) > 2000 {
                                                    // 時刻に差異がある場合、内容をダウンロードして比較
                                                    gloo::console::log!("[Conflict] Time difference detected for", &s.title, ". Comparing content...");
                                                    if let Ok(drive_val) = crate::drive_interop::download_file(did, None).await {
                                                        let drive_content = drive_val.as_string().unwrap_or_default();
                                                        if drive_content == s.content {
                                                            // 内容が同じならサイレント更新
                                                            gloo::console::log!("[Conflict] Content is identical. Silently updating timestamp.");
                                                            updated_sheets[idx].last_sync_timestamp = Some(dt);
                                                            needs_db_update = true;
                                                            
                                                            // DB保存
                                                            let ds = &updated_sheets[idx];
                                                            let js = JSSheet { id: ds.id.clone(), guid: ds.guid.clone(), category: ds.category.clone(), title: ds.title.clone(), content: ds.content.clone(), is_modified: ds.is_modified, drive_id: ds.drive_id.clone(), temp_content: ds.temp_content.clone(), temp_timestamp: ds.temp_timestamp, last_sync_timestamp: ds.last_sync_timestamp, tab_color: ds.tab_color.clone() };
                                                            let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                                            if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                                        } else {
                                                            // 内容が異なる場合のみ競合リストへ
                                                            gloo::console::warn!("[Conflict] Content differs. Showing dialog for:", &s.title);
                                                            conflicts.push(ConflictData { sheet_id: s.id.clone(), title: s.title.clone(), drive_id: did.clone(), local_content: s.content.clone(), drive_time: dt, time_str: ts.clone(), is_missing_on_drive: false });
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                Err(e) => {
                                    gloo::console::error!("[Conflict] Detected (Missing/404 on Drive):", &s.title, e);
                                    conflicts.push(ConflictData { sheet_id: s.id.clone(), title: s.title.clone(), drive_id: did.clone(), local_content: s.content.clone(), drive_time: 0, time_str: "".to_string(), is_missing_on_drive: true });
                                }
                            }
                        } else {
                            let has_real_content = !s.content.is_empty() || s.temp_content.is_some();
                            let was_ever_synced = s.last_sync_timestamp.is_some();
                            if was_ever_synced && has_real_content {
                                gloo::console::warn!("[Conflict] Detected (Was synced but Drive ID lost):", &s.title);
                                conflicts.push(ConflictData { sheet_id: s.id.clone(), title: s.title.clone(), drive_id: "".to_string(), local_content: s.content.clone(), drive_time: 0, time_str: "".to_string(), is_missing_on_drive: true });
                            }
                        }
                    }

                    if needs_db_update {
                        s_state.set(updated_sheets);
                    }

                    if !conflicts.is_empty() { 
                        gloo::console::log!("[Conflict] Found total", conflicts.len(), "real conflicts.");
                        q.set(conflicts); 
                    }
                    else { 
                        gloo::console::log!("[Conflict] No real conflicts found.");
                        let finish = move || { ifo_c.set(true); Timeout::new(300, move || { ild.set(false); }).forget(); }; 
                        if *min { finish(); } else { Timeout::new(1000, move || { finish(); }).forget(); } 
                    }
                    c_checked.set(true); 
                });
            }
            || ()
        });
    }

    // Network Recovery
    {
        let network_connected = network_connected.clone();
        let sheets = sheets.clone();
        let no_category_folder_id = no_category_folder_id.clone();
        let conflict_checked = conflict_checked.clone();
        let db_ready = *db_loaded;
        use_effect_with((*network_connected, no_category_folder_id.clone(), *conflict_checked, db_ready), move |deps| {
            let (connected, folder_option, checked, ready) = deps;
            if *connected && *checked && *ready {
                if let Some(fid) = folder_option.as_ref().map(|s| s.clone()) {
                    let s_state = sheets.clone();
                    spawn_local(async move {
                        let mut u_sheets = (*s_state).clone(); let mut changed = false;
                        for s in u_sheets.iter_mut() {
                            if let Some(tc) = &s.temp_content {
                                let save_c = tc.clone();
                                if s.guid.is_none() { s.guid = Some(generate_uuid()); }
                                let fname = format!("{}.txt", s.guid.as_ref().unwrap());
                                if let Ok(rv) = upload_file(&fname, &save_c, &fid, s.drive_id.as_deref()).await {
                                    let mut n_did = s.drive_id.clone();
                                    let mut stime = s.last_sync_timestamp;

                                    if let Ok(iv) = js_sys::Reflect::get(&rv, &JsValue::from_str("id")) {
                                        if let Some(is) = iv.as_string() { n_did = Some(is); }
                                    }
                                    if let Ok(tv) = js_sys::Reflect::get(&rv, &JsValue::from_str("modifiedTime")) {
                                        if let Some(ts) = tv.as_string() { stime = Some(crate::drive_interop::parse_date(&ts) as u64); }
                                    }

                                    if let Some(is) = n_did.clone() {
                                        s.drive_id = Some(is.clone()); s.temp_content = None; s.temp_timestamp = None;
                                        s.content = save_c.clone(); s.is_modified = false; s.last_sync_timestamp = stime;
                                        changed = true;
                                        let js = JSSheet { id: s.id.clone(), guid: s.guid.clone(), category: s.category.clone(), title: s.title.clone(), content: save_c, is_modified: false, drive_id: Some(is), temp_content: None, temp_timestamp: None, last_sync_timestamp: stime, tab_color: s.tab_color.clone() };
                                        let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                        if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                    }
                                }
                            }
                        }
                        if changed { s_state.set(u_sheets); }
                    });
                }
            }
            || ()
        });
    }

    let on_login = Callback::from(|_: MouseEvent| { request_access_token(); });

    let on_new_sheet_cb = {
        let s_state = sheets.clone();
        let aid_state = active_sheet_id.clone();
        let sp_state = is_suppressing_changes.clone();
        let r_s = sheets_ref.clone();
        let r_aid = active_id_ref.clone();
        Callback::from(move |_| {
            let s = s_state.clone(); let aid = aid_state.clone(); let sp = sp_state.clone();
            let r_s = r_s.clone(); let r_aid = r_aid.clone();
            Timeout::new(0, move || {
                sp.set(true);
                let nid = js_sys::Date::now().to_string();
                let ns = Sheet { id: nid.clone(), guid: None, category: "NO_CATEGORY".to_string(), title: "Untitled".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color() };
                *r_s.borrow_mut() = vec![ns.clone()];
                *r_aid.borrow_mut() = Some(nid.clone());
                s.set(vec![ns.clone()]);
                aid.set(Some(nid.clone()));
                set_editor_content(""); set_gutter_status(true); focus_editor();
                let spr = sp.clone(); Timeout::new(100, move || { spr.set(false); }).forget();
                spawn_local(async move {
                    let js = JSSheet { id: nid, guid: None, category: "NO_CATEGORY".to_string(), title: "Untitled".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: ns.tab_color };
                    let ser = serde_wasm_bindgen::Serializer::json_compatible();
                    if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                });
            }).forget();
        })
    };
    
    let on_save_cb = {
        let r_aid = active_id_ref.clone();
        let r_s = sheets_ref.clone();
        let s_state = sheets.clone();
        let r_ncid = no_category_id_ref.clone();
        let network_connected = network_connected.clone();
        let is_ld = is_loading.clone();
        let is_fo = is_fading_out.clone();
        let r_is_saving = is_saving_ref.clone();
        let sp = is_suppressing_changes.clone();
        let lmk = loading_message_key.clone();
        
        Callback::from(move |is_manual: bool| {
            gloo::console::log!("[Save] Callback triggered. Manual:", is_manual);
            if *r_is_saving.borrow() { gloo::console::log!("[Save] Ignored: busy"); return; }
            let aid_opt = (*r_aid.borrow()).clone();
            if let Some(id) = aid_opt {
                let cur_c_val = get_editor_content();
                let cur_c = if let Some(s) = cur_c_val.as_string() { s } else { gloo::console::warn!("[Save] Aborted: Editor not ready"); return; };
                
                let mut cur_s = (*r_s.borrow()).clone();
                let is_online = *network_connected && web_sys::window().unwrap().navigator().on_line();
                let sheet_opt = if let Some(idx) = cur_s.iter().position(|s| s.id == id) { cur_s.get_mut(idx) } else { cur_s.get_mut(0) };
                
                if let Some(sheet) = sheet_opt {
                    let is_new = sheet.drive_id.is_none();
                    // 手動保存でない場合にのみ、変更がないかチェックしてスキップする
                    if !is_manual && !is_new && !sheet.is_modified && sheet.content == cur_c {
                        return;
                    }

                    gloo::console::log!("[Save] process starting for:", &sheet.title, "Manual:", is_manual);
                    sheet.content = cur_c.clone(); sheet.is_modified = false;
                    
                    if !is_online {
                        gloo::console::log!("[Save] Offline: DB only");
                        sheet.temp_content = Some(cur_c.clone()); sheet.temp_timestamp = Some(js_sys::Date::now() as u64);
                        let js = JSSheet { id: sheet.id.clone(), guid: sheet.guid.clone(), category: sheet.category.clone(), title: sheet.title.clone(), content: sheet.content.clone(), is_modified: false, drive_id: sheet.drive_id.clone(), temp_content: sheet.temp_content.clone(), temp_timestamp: sheet.temp_timestamp, last_sync_timestamp: sheet.last_sync_timestamp, tab_color: sheet.tab_color.clone() };
                        s_state.set(cur_s);
                        spawn_local(async move { let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } });
                        return;
                    }
                    
                    let ncid_val = (*r_ncid.borrow()).clone();
                    if ncid_val.is_none() { gloo::console::error!("[Save] Aborted: No folder ID"); return; }
                    let fid = ncid_val.unwrap();
                    if sheet.guid.is_none() { sheet.guid = Some(generate_uuid()); }
                    let s_clone = sheet.clone(); let s_inner = s_state.clone(); let nc_inner = network_connected.clone();
                    let ild_inner = is_ld.clone(); let ifo_inner = is_fo.clone(); let ris_inner = r_is_saving.clone(); let sp_inner = sp.clone();
                    let lmk_inner = lmk.clone();
                    
                    *r_is_saving.borrow_mut() = true;
                    
                    // 手動保存、または新規ファイル保存の場合はUIをブロックしてインジケータを表示
                    if is_manual || is_new {
                        lmk_inner.set("saving");
                        ild_inner.set(true);
                        ifo_inner.set(false);
                        sp_inner.set(true);
                    }

                    spawn_local(async move {
                         let mut n_did = s_clone.drive_id.clone(); let mut stime = s_clone.last_sync_timestamp;
                         let fname = format!("{}.txt", s_clone.guid.as_ref().unwrap());
                         
                         gloo::console::log!("[Save] Uploading:", &fname, "Content length:", s_clone.content.len());
                         let res = upload_file(&fname, &s_clone.content, &fid, s_clone.drive_id.as_deref()).await;
                         
                         match res {
                             Ok(rv) => {
                                 if let Ok(iv) = js_sys::Reflect::get(&rv, &JsValue::from_str("id")) { if let Some(is) = iv.as_string() { n_did = Some(is); } }
                                 if let Ok(tv) = js_sys::Reflect::get(&rv, &JsValue::from_str("modifiedTime")) { if let Some(ts) = tv.as_string() { stime = Some(crate::drive_interop::parse_date(&ts) as u64); } }
                                 gloo::console::log!("[Save] Upload successful.");
                             },
                             Err(e) => {
                                 gloo::console::error!("[Save] Upload failed:", e); nc_inner.set(false);
                                 let js = JSSheet { id: s_clone.id.clone(), guid: s_clone.guid.clone(), category: s_clone.category.clone(), title: s_clone.title.clone(), content: s_clone.content.clone(), is_modified: false, drive_id: s_clone.drive_id.clone(), temp_content: Some(s_clone.content.clone()), temp_timestamp: Some(js_sys::Date::now() as u64), last_sync_timestamp: s_clone.last_sync_timestamp, tab_color: s_clone.tab_color.clone() };
                                 let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                 
                                 if is_manual || is_new { 
                                     ild_inner.set(false); 
                                     sp_inner.set(false); 
                                 }
                                 *ris_inner.borrow_mut() = false; 
                                 return;
                             },
                         }
                         
                         let js = JSSheet { id: s_clone.id.clone(), guid: s_clone.guid.clone(), category: s_clone.category.clone(), title: s_clone.title.clone(), content: s_clone.content.clone(), is_modified: false, drive_id: n_did.clone(), temp_content: None, temp_timestamp: None, last_sync_timestamp: stime, tab_color: s_clone.tab_color.clone() };
                         let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                         
                         let mut u_s = (*s_inner).clone();
                         if let Some(si) = u_s.iter_mut().find(|x| x.id == s_clone.id) {
                             si.drive_id = n_did; si.content = s_clone.content.clone(); si.is_modified = false;
                             si.temp_content = None; si.temp_timestamp = None; si.last_sync_timestamp = stime;
                         }
                         s_inner.set(u_s);

                         if is_manual || is_new {
                             ifo_inner.set(true);
                             let ildf = ild_inner.clone(); let spf = sp_inner.clone();
                             Timeout::new(300, move || { ildf.set(false); spf.set(false); }).forget();
                         }
                         
                         *ris_inner.borrow_mut() = false;
                         gloo::console::log!("[Save] Done.");
                    });
                    s_state.set(cur_s);
                }
            } else { gloo::console::error!("[Save] Error: No active ID"); }
        })
    };

    let on_toggle_vim = { let vim = vim_mode.clone(); Callback::from(move |_| { vim.set(!*vim); }) };
    let on_open_dialog = { let iv = is_file_open_dialog_visible.clone(); let sp = is_suppressing_changes.clone(); Callback::from(move |_| { sp.set(true); iv.set(true); }) };
    let on_change_font_size = Callback::from(|delta: i32| { crate::js_interop::change_font_size(delta); });

    // Editor Initialization
    {
        let os = on_save_cb.clone(); let on = on_new_sheet_cb.clone(); let oo = on_open_dialog.clone();
        let is_auth = *is_authenticated; let ast = auto_save_timer.clone(); let s_init = sheets.clone(); 
        let v_init = vim_mode.clone(); let ncid = no_category_folder_id.clone(); let nc_init = network_connected.clone();
        let sp_init = is_suppressing_changes.clone(); let r_s = sheets_ref.clone(); let r_aid = active_id_ref.clone();
        let is_ld = *is_loading;
        use_effect_with((is_auth, ncid.clone(), is_ld), move |deps| {
            let (auth, _, loading) = deps;
            if *auth && !*loading {
                let os_i = os.clone(); let on_i = on.clone(); let oo_i = oo.clone(); let s_state = s_init.clone();
                let timer = ast.clone(); let vim_val = *v_init; let ncid_cb = ncid.clone(); let nc_cb = nc_init.clone();
                let sp_cb = sp_init.clone(); let r_s_i = r_s.clone(); let r_aid_i = r_aid.clone();
                let callback = Closure::wrap(Box::new(move |cmd: String| {
                    if cmd == "save" { os_i.emit(true); } // 手動保存
                    else if cmd == "new_sheet" { on_i.emit(()); }
                    else if cmd == "open" { oo_i.emit(()); }
                    else if cmd == "change" {
                        if *sp_cb { return; }
                        if ncid_cb.is_none() && *nc_cb { return; }
                        let cur_c_val = get_editor_content();
                        let cur_c = if let Some(s) = cur_c_val.as_string() { s } else { return; };
                        
                        let aid = (*r_aid_i.borrow()).clone();
                        if let Some(id) = aid {
                            let mut cur_s = (*r_s_i.borrow()).clone();
                            let mut drv_exists = false; let mut needs_upd = false;
                            if let Some(sheet) = cur_s.iter_mut().find(|s| s.id == id) {
                                if sheet.content != cur_c {
                                    sheet.content = cur_c.clone();
                                    if !sheet.is_modified { sheet.is_modified = true; needs_upd = true; }
                                }
                                drv_exists = sheet.drive_id.is_some();
                            }
                            if needs_upd { s_state.set(cur_s); }
                            if drv_exists { let osa = os_i.clone(); timer.set(Some(Timeout::new(3000, move || { osa.emit(false); }))); } // 自動保存
                        }
                    }
                }) as Box<dyn FnMut(String)>);
                init_editor("editor", &callback); set_vim_mode(vim_val); callback.forget();
            }
            || ()
        });
    }

    // Sync content
    {
        let aid = (*active_sheet_id).clone(); 
        let is_auth = *is_authenticated; let is_ld = *is_loading;
        let s_handle = sheets.clone();
        use_effect_with((aid, is_auth, is_ld), move |deps| {
            let (id_opt, auth, loading) = deps;
            if *auth && !*loading { 
                if let Some(id) = id_opt { 
                    if let Some(s) = (*s_handle).iter().find(|x| x.id == *id) { 
                        gloo::console::log!("[Sync] Updating editor content for:", &s.title);
                        set_editor_content(&s.content); 
                        set_gutter_status(s.drive_id.is_none()); 
                    } 
                } 
            }
            || ()
        });
    }

    // Vim mode & Resize
    { let v = *vim_mode; let a = *is_authenticated; use_effect_with((v, a), move |deps| { if deps.1 { set_vim_mode(deps.0); } || () }); }
    { let a = *is_authenticated; use_effect_with(a, move |auth| { let mut l = None; if *auth { let w = web_sys::window().unwrap(); l = Some(EventListener::new(&w, "resize", move |_| { resize_editor(); })); } move || { drop(l); } }); }

    let on_refresh_cats = {
        let ldid_s = leaf_data_folder_id.clone(); let cats_s = categories.clone();
        Callback::from(move |_: ()| {
            if let Some(id) = (*ldid_s).clone() {
                let c_s = cats_s.clone();
                spawn_local(async move {
                    if let Ok(cr) = list_folders(&id).await {
                        if let Ok(fv) = js_sys::Reflect::get(&cr, &JsValue::from_str("files")) {
                            let fa = js_sys::Array::from(&fv); let mut n_cats = Vec::new();
                            for i in 0..fa.length() { let v = fa.get(i); let ci = js_sys::Reflect::get(&v, &JsValue::from_str("id")).unwrap().as_string().unwrap(); let cn = js_sys::Reflect::get(&v, &JsValue::from_str("name")).unwrap().as_string().unwrap(); n_cats.push(JSCategory { id: ci, name: cn }); }
                            if let Ok(v) = serde_wasm_bindgen::to_value(&n_cats) { let _ = save_categories(v).await; }
                            c_s.set(n_cats);
                        }
                    }
                });
            }
        })
    };

    let on_file_sel = {
        let aid = active_sheet_id.clone(); let iv = is_file_open_dialog_visible.clone(); let sp = is_suppressing_changes.clone();
        let s_s = sheets.clone(); let il = is_loading.clone(); let ifo = is_fading_out.clone();
        let r_s = sheets_ref.clone(); let r_aid = active_id_ref.clone();
        Callback::from(move |(did, title, cat): (String, String, String)| {
            iv.set(false); il.set(true); ifo.set(false);
            let s_state = s_s.clone(); let aid_state = aid.clone(); let sp_inner = sp.clone(); let il_inner = il.clone(); let ifo_inner = ifo.clone();
            let rs = r_s.clone(); let raid = r_aid.clone();
            spawn_local(async move {
                if let Ok(cv) = crate::drive_interop::download_file(&did, None).await {
                    if let Some(c) = cv.as_string() {
                        let mut cs = (*s_state).clone(); let tidx = if cs.len() == 1 && cs[0].drive_id.is_none() { Some(0) } else { None };
                        let guid = if title.ends_with(".txt") { Some(title.replace(".txt", "")) } else { Some(title.clone()) };
                        let nid = if let Some(idx) = tidx { cs[idx].id.clone() } else { js_sys::Date::now().to_string() };
                        let ns = Sheet { id: nid.clone(), guid: guid.clone(), category: cat.clone(), title: title.clone(), content: c.clone(), is_modified: false, drive_id: Some(did.clone()), temp_content: None, temp_timestamp: None, last_sync_timestamp: Some(js_sys::Date::now() as u64), tab_color: if let Some(idx) = tidx { cs[idx].tab_color.clone() } else { generate_random_color() } };
                        if let Some(idx) = tidx { cs[idx] = ns.clone(); } else { cs = vec![ns.clone()]; }
                        *rs.borrow_mut() = cs.clone(); *raid.borrow_mut() = Some(nid.clone());
                        s_state.set(cs); aid_state.set(Some(nid.clone())); set_editor_content(&c); set_gutter_status(false); focus_editor();
                        let spr = sp_inner.clone(); Timeout::new(100, move || { spr.set(false); }).forget();
                        let js = JSSheet { id: nid, guid, category: cat, title, content: c, is_modified: false, drive_id: Some(did), temp_content: None, temp_timestamp: None, last_sync_timestamp: Some(js_sys::Date::now() as u64), tab_color: ns.tab_color };
                        let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                        ifo_inner.set(true); Timeout::new(300, move || { il_inner.set(false); }).forget();
                    }
                } else { il_inner.set(false); }
            });
        })
    };

    let on_conf_cfm = {
        let cq = conflict_queue.clone(); let s_s = sheets.clone(); let il = is_loading.clone(); let ifo = is_fading_out.clone();
        let ncid = no_category_folder_id.clone(); let aid = active_sheet_id.clone();
        Callback::from(move |sel: usize| {
            let mut q = (*cq).clone(); if q.is_empty() { return; } let conf = q.remove(0);
            let ss = s_s.clone(); let qs = cq.clone(); let ifod = ifo.clone();
            let fid_opt = ncid.as_ref().map(|s| s.clone()); let aid_v = (*aid).clone();
            let aid_inner = aid.clone(); 
            let aid_final_c = aid.clone(); 
            let ild_final_c = il.clone();
            spawn_local(async move {
                let mut us = (*ss).clone();
                let mut is_deleted = false;
                if let Some(pos) = us.iter().position(|x| x.id == conf.sheet_id) {
                    let s = &mut us[pos];
                    match sel {
                        0 => { if let Ok(dv) = crate::drive_interop::download_file(&conf.drive_id, None).await { if let Some(t) = dv.as_string() { s.content = t.clone(); s.temp_content = None; s.temp_timestamp = None; s.last_sync_timestamp = Some(conf.drive_time); s.is_modified = false; if Some(s.id.clone()) == aid_v { set_editor_content(&t); } } } },
                        1 => { 
                            if let Some(fid) = fid_opt { 
                                // ファイルが存在しない場合は既存IDを渡さず新規作成扱いにする
                                let did_arg = if conf.is_missing_on_drive { None } else { Some(conf.drive_id.as_str()) };
                                if let Ok(rv) = upload_file(&format!("{}.txt", s.guid.as_ref().unwrap_or(&generate_uuid())), &s.content, &fid, did_arg).await {
                                    if let Ok(iv) = js_sys::Reflect::get(&rv, &JsValue::from_str("id")) { if let Some(is) = iv.as_string() { s.drive_id = Some(is); } }
                                    if let Ok(tv) = js_sys::Reflect::get(&rv, &JsValue::from_str("modifiedTime")) { if let Some(ts) = tv.as_string() { s.last_sync_timestamp = Some(crate::drive_interop::parse_date(&ts) as u64); } }
                                    s.temp_content = None; s.temp_timestamp = None; s.is_modified = false; 
                                }
                            } 
                        },
                        2 => { if let Some(fid) = fid_opt { let ng = generate_uuid(); let _ = upload_file(&format!("{}.txt", ng), &s.content, &fid, None).await; s.guid = Some(ng); s.temp_content = None; s.temp_timestamp = None; s.last_sync_timestamp = Some(js_sys::Date::now() as u64); s.is_modified = false; s.tab_color = generate_random_color(); } },
                        3 => {
                            // 削除処理
                            let _ = crate::db_interop::delete_sheet(&s.id).await;
                            us.remove(pos);
                            is_deleted = true;
                        },
                        _ => {}
                    }
                    if !is_deleted {
                        let s = &us[pos];
                        let js = JSSheet { id: s.id.clone(), guid: s.guid.clone(), category: s.category.clone(), title: s.title.clone(), content: s.content.clone(), is_modified: s.is_modified, drive_id: s.drive_id.clone(), temp_content: s.temp_content.clone(), temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, tab_color: s.tab_color.clone() };
                        let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                    }
                }
                
                // シートが空になったら新規作成
                if us.is_empty() {
                    let nid = js_sys::Date::now().to_string();
                    let ns = Sheet { id: nid.clone(), guid: None, category: "NO_CATEGORY".to_string(), title: "Untitled 1".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color() };
                    us.push(ns.clone());
                    aid_inner.set(Some(nid.clone()));
                    set_editor_content(""); focus_editor();
                    let js = JSSheet { id: nid, guid: None, category: "NO_CATEGORY".to_string(), title: "Untitled 1".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: ns.tab_color };
                    let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                } else if is_deleted {
                    // 削除した場合、有効なシートを選択
                    let nid = us.last().unwrap().id.clone();
                    aid_inner.set(Some(nid));
                }

                ss.set(us.clone()); qs.set(q.clone());
                if q.is_empty() { 
                    gloo::console::log!("[Conflict] All conflicts resolved. Restoring editor display.");
                    ifod.set(true); 
                    let ild_final = ild_final_c.clone();
                    let aid_final = aid_final_c.clone();
                    let us_final = us.clone(); // 最新のベクタを直接Timeoutへ渡す
                    
                    Timeout::new(350, move || { 
                        ild_final.set(false); 
                        
                        // エディタの内容を最新の状態で上書き
                        if let Some(id) = (*aid_final).clone() {
                            if let Some(s) = us_final.iter().find(|x| x.id == id) {
                                gloo::console::log!("[Conflict] Restoring content for editor:", &s.title);
                                set_editor_content(&s.content);
                                set_gutter_status(s.drive_id.is_none());
                            }
                        }
                        
                        // レイアウトを強制的に計算し直し、表示を復旧
                        resize_editor();
                        focus_editor();
                        
                        // 微調整のため少し遅れて再度リサイズ
                        Timeout::new(50, move || { resize_editor(); }).forget();
                    }).forget(); 
                }
            });
        })
    };

    let lmk_conflict = loading_message_key.clone();
    let conflict_dialog = if let Some(conflict) = conflict_queue.first() {
        let title = if conflict.is_missing_on_drive {
            i18n::t("file_not_found", lang)
        } else {
            i18n::t("conflict_detected", lang)
        };
        
        let message = if conflict.is_missing_on_drive {
            i18n::t("missing_file_message", lang).replace("{}", &conflict.title)
        } else {
            i18n::t("conflict_message", lang).replace("{}", &conflict.title)
        };

        let options = if conflict.is_missing_on_drive {
            vec![
                DialogOption { id: 1, label: i18n::t("opt_reupload", lang) },
                DialogOption { id: 3, label: i18n::t("opt_delete_local", lang) },
            ]
        } else {
            vec![
                DialogOption { id: 0, label: i18n::t("opt_load_drive", lang) },
                DialogOption { id: 1, label: i18n::t("opt_overwrite_drive", lang) },
                DialogOption { id: 2, label: i18n::t("opt_save_new", lang) },
            ]
        };

        Some(html! {
            <CustomDialog 
                title={title} 
                message={message} 
                options={options} 
                on_confirm={on_conf_cfm} 
                on_start_processing={let il = is_loading.clone(); let ifo = is_fading_out.clone(); let lmk = lmk_conflict.clone(); move |_| { lmk.set("saving"); il.set(true); ifo.set(false); }}
            />
        })
    } else {
        None
    };

    html! {
        <div class="relative h-screen w-screen overflow-hidden">
            if !*is_authenticated {
                <main class={classes!("flex", "h-screen", "w-screen", "items-center", "justify-center", "bg-gray-900", "text-white")}>
                    <div class={classes!("text-center")}>
                        <h1 class={classes!("text-5xl", "font-bold", "mb-8", "text-green-500")} style="font-family: 'Petit Formal Script', cursive;">{ app_name }</h1>
                        <button onclick={on_login} class={classes!("bg-blue-600", "hover:bg-blue-700", "text-white", "font-bold", "py-2", "px-6", "rounded-md", "transition-colors", "shadow-lg")}>{ i18n::t("signin_with_google", lang) }</button>
                        <div class={classes!("mt-6", "text-gray-400", "text-sm")}>{ i18n::t("login_required", lang) }</div>
                    </div>
                </main>
            } else {
                <main class={classes!("flex", "flex-col", "bg-gray-900", "text-white", "h-full", "w-full")}>
                    <ButtonBar vim_mode={*vim_mode} on_toggle_vim={on_toggle_vim} on_new_sheet={on_new_sheet_cb.clone()} on_open={on_open_dialog} on_change_font_size={on_change_font_size} />
                    <div id="editor" key="ace-editor-main" class={classes!("flex-1", "bg-gray-950", "z-10")} style="width: 100%; min-height: 0;"></div>
                    <StatusBar network_status={*network_connected} version={env!("CARGO_PKG_VERSION").to_string()} />
                </main>
            }
            if *is_file_open_dialog_visible {
                if let Some(ldid) = (*leaf_data_folder_id).clone() {
                    <FileOpenDialog 
                        on_close={let iv = is_file_open_dialog_visible.clone(); let sp = is_suppressing_changes.clone(); move |_| { iv.set(false); sp.set(false); focus_editor(); }} 
                        on_select={on_file_sel} 
                        leaf_data_id={ldid} 
                        categories={(*categories).clone()} 
                        on_refresh={on_refresh_cats} 
                        on_start_processing={let il = is_loading.clone(); let ifo = is_fading_out.clone(); let lmk = loading_message_key.clone(); move |_| { lmk.set("synchronizing"); il.set(true); ifo.set(false); }}
                    />
                }
            }
            { for conflict_dialog }
            if *is_loading {
                <div class={classes!("fixed", "inset-0", "z-[200]", "flex", "items-center", "justify-center", "bg-gray-900", "transition-opacity", "duration-300", if *is_fading_out { "opacity-0" } else { "opacity-100" } )}>
                    <div class="flex flex-col items-center">
                        <img src="icon.svg" class="mb-8 shadow-2xl animate-in fade-in zoom-in duration-500" style="width: 20vmin; height: 20vmin;" alt="Leaf Icon" />
                        <div class="w-12 h-12 border-4 border-green-500 border-t-transparent rounded-full animate-spin"></div>
                        if *is_authenticated { <p class="mt-4 text-white font-bold text-lg animate-pulse">{ i18n::t(*loading_message_key, lang) }</p> }
                    </div>
                </div>
            }
        </div>
    }
}
