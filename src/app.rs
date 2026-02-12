use yew::prelude::*;
use crate::components::button_bar::ButtonBar;
use crate::components::status_bar::StatusBar;
use crate::components::tab_area::{TabArea, Sheet};
use crate::components::dialog::{CustomDialog, DialogOption};
use crate::js_interop::{init_editor, set_vim_mode, resize_editor, get_editor_content, set_editor_content, set_window_title, generate_uuid};
use crate::auth_interop::{init_google_auth, request_access_token};
use crate::db_interop::{init_db, save_sheet, load_sheets, delete_sheet, JSSheet};
use crate::drive_interop::{upload_file, ensure_directory_structure};
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::spawn_local;
use gloo::events::EventListener;
use gloo::timers::callback::Timeout;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

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
}

#[function_component(App)]
pub fn app() -> Html {
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
    let auto_save_timer = use_state(|| None::<Timeout>);
    let is_loading = use_state(|| true);
    let is_fading_out = use_state(|| false);
    let min_time_passed = use_state(|| false);
    
    // 競合解決用のキュー
    let conflict_queue = use_state(|| Vec::<ConflictData>::new());

    // Minimum display time timer (1s)
    {
        let min_time_passed = min_time_passed.clone();
        use_effect_with((), move |_| {
            let timeout = Timeout::new(1000, move || {
                min_time_passed.set(true);
            });
            move || { drop(timeout); }
        });
    }
    
    {
        let app_name = app_name.clone();
        use_effect_with((), move |_| {
            set_window_title(&app_name);
            || ()
        });
    }

    // Auth Initialization & Directory Setup
    {
        let is_authenticated = is_authenticated.clone();
        let no_category_folder_id = no_category_folder_id.clone();
        let client_id = client_id.clone();
        
        use_effect_with((), move |_| {
            let is_authenticated_cb = is_authenticated.clone();
            let no_category_folder_id = no_category_folder_id.clone();

            let callback = Closure::wrap(Box::new(move |_token: String| {
                if !*is_authenticated_cb {
                    is_authenticated_cb.set(true);
                    let no_category_folder_id = no_category_folder_id.clone();
                    let is_authenticated_err = is_authenticated_cb.clone();
                    spawn_local(async move {
                        match ensure_directory_structure().await {
                            Ok(res) => {
                                if let Ok(id_val) = js_sys::Reflect::get(&res, &JsValue::from_str("noCategoryId")) {
                                    if let Some(id) = id_val.as_string() {
                                        no_category_folder_id.set(Some(id));
                                    }
                                }
                            },
                            Err(e) => {
                                gloo::console::error!("Failed to setup directories", e);
                                is_authenticated_err.set(false); // 認証エラー時等にログイン画面に戻す
                            },
                        }
                    });
                }
            }) as Box<dyn FnMut(String)>);
            
            init_google_auth(&client_id, &callback);
            callback.forget(); 
            || ()
        });
    }

    // Network Monitoring
    {
        let network_connected = network_connected.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let on_online = {
                let network_connected = network_connected.clone();
                EventListener::new(&window, "online", move |_| { network_connected.set(true); })
            };
            let on_offline = {
                let network_connected = network_connected.clone();
                EventListener::new(&window, "offline", move |_| { network_connected.set(false); })
            };
            network_connected.set(window.navigator().on_line());
            move || { drop(on_online); drop(on_offline); }
        });
    }

    // Conflict Resolution Effect
    let conflict_checked = use_state(|| false);
    {
        let sheets = sheets.clone();
        let is_authenticated_val = *is_authenticated;
        let no_category_id_val = (*no_category_folder_id).clone();
        let conflict_checked = conflict_checked.clone();
        let is_loading = is_loading.clone();
        let is_fading_out = is_fading_out.clone();
        let min_time_passed = min_time_passed.clone();
        let active_sheet_id = active_sheet_id.clone();
        let conflict_queue_for_effect = conflict_queue.clone();
        
        // sheets, auth, folder_id, active_sheet_id のすべてが揃った時に実行
        use_effect_with((is_authenticated_val, no_category_id_val, sheets.len(), (*active_sheet_id).clone()), move |(auth, folder_id, sheet_count, _)| {
            if *auth && folder_id.is_some() && !*conflict_checked {
                let sheets_state = sheets.clone();
                let conflict_checked = conflict_checked.clone();
                let is_loading = is_loading.clone();
                let is_fading_out = is_fading_out.clone();
                let min_time_passed = min_time_passed.clone();
                let conflict_queue = conflict_queue_for_effect.clone();
                let count = *sheet_count;
                
                if count == 0 {
                    conflict_checked.set(true);
                    let finish = move || {
                        is_fading_out.set(true);
                        Timeout::new(300, move || { is_loading.set(false); }).forget();
                    };
                    if *min_time_passed { finish(); } else { Timeout::new(1000, move || { finish(); }).forget(); }
                } else {
                    spawn_local(async move {
                        gloo::console::log!("Scanning for conflicts in", count, "sheets...");
                        let sheets_val = (*sheets_state).clone();
                        let mut conflicts = Vec::new();

                        for sheet in sheets_val {
                            let local_timestamp = sheet.temp_timestamp.or(sheet.last_sync_timestamp);

                            if let (Some(l_time), Some(drive_id)) = (local_timestamp, &sheet.drive_id) {
                                if let Ok(meta) = crate::drive_interop::get_file_metadata(drive_id).await {
                                    if let Ok(time_val) = js_sys::Reflect::get(&meta, &JsValue::from_str("modifiedTime")) {
                                        if let Some(time_str) = time_val.as_string() {
                                            let drive_time = crate::drive_interop::parse_date(&time_str) as u64;
                                            let diff = if drive_time > l_time { drive_time - l_time } else { l_time - drive_time };
                                            
                                            if drive_time > l_time && diff > 2000 {
                                                conflicts.push(ConflictData {
                                                    sheet_id: sheet.id.clone(),
                                                    title: sheet.title.clone(),
                                                    drive_id: drive_id.clone(),
                                                    local_content: sheet.content.clone(),
                                                    drive_time,
                                                    time_str: time_str.clone(),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        if !conflicts.is_empty() {
                            conflict_queue.set(conflicts);
                        } else {
                            // 競合がなければ即終了処理
                            let finish = move || {
                                is_fading_out.set(true);
                                Timeout::new(300, move || { is_loading.set(false); }).forget();
                            };
                            if *min_time_passed { finish(); } else { Timeout::new(1000, move || { finish(); }).forget(); }
                        }
                        conflict_checked.set(true); 
                    });
                }
            }
            || ()
        });
    }
    // Network Recovery Sync
    {
        let network_connected = network_connected.clone();
        let sheets = sheets.clone();
        let no_category_folder_id = no_category_folder_id.clone();
        
        use_effect_with((*network_connected, no_category_folder_id.clone()), move |(connected, folder_id)| {
            if *connected {
                if let Some(fid) = (**folder_id).clone() {
                    let sheets_state = sheets.clone();
                    spawn_local(async move {
                        let mut updated_sheets = (*sheets_state).clone();
                        let mut changed = false;

                        for sheet in updated_sheets.iter_mut() {
                            if let Some(temp_content) = &sheet.temp_content {
                                let content_to_save = temp_content.clone();
                                gloo::console::log!("Network restored: Syncing temporary data for", &sheet.title);
                                
                                if sheet.guid.is_none() {
                                    sheet.guid = Some(generate_uuid());
                                }
                                
                                let filename = format!("{}.txt", sheet.guid.as_ref().unwrap());
                                let res = upload_file(&filename, &content_to_save, &fid, sheet.drive_id.as_deref()).await;
                                
                                if let Ok(res_val) = res {
                                    if let Ok(id_val) = js_sys::Reflect::get(&res_val, &JsValue::from_str("id")) {
                                        if let Some(id_str) = id_val.as_string() {
                                            let current_now = js_sys::Date::now() as u64;
                                            sheet.drive_id = Some(id_str);
                                            sheet.temp_content = None;
                                            sheet.temp_timestamp = None;
                                            sheet.content = content_to_save;
                                            sheet.is_modified = false;
                                            sheet.last_sync_timestamp = Some(current_now);
                                            changed = true;

                                            // Save to IndexedDB
                                            let js_sheet = JSSheet {
                                                id: sheet.id.clone(),
                                                guid: sheet.guid.clone(),
                                                category: sheet.category.clone(),
                                                title: sheet.title.clone(),
                                                content: sheet.content.clone(),
                                                is_modified: false,
                                                drive_id: sheet.drive_id.clone(),
                                                temp_content: None,
                                                temp_timestamp: None,
                                                last_sync_timestamp: Some(current_now),
                                            };
                                            let serializer = serde_wasm_bindgen::Serializer::json_compatible();
                                            if let Ok(val) = js_sheet.serialize(&serializer) {
                                                let _ = save_sheet(val).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if changed {
                            sheets_state.set(updated_sheets);
                        }
                    });
                }
            }
            || ()
        });
    }
    // DB Initialization
    {
        let sheets = sheets.clone();
        let active_sheet_id = active_sheet_id.clone();
        let db_name = format!("{}DB", app_name);
        use_effect_with((), move |_| {
            spawn_local(async move {
                if let Err(e) = init_db(&db_name).await {
                    gloo::console::error!("Failed to init DB", e);
                    return;
                }
                match load_sheets().await {
                    Ok(val) => {
                        if let Ok(loaded_sheets) = serde_wasm_bindgen::from_value::<Vec<JSSheet>>(val) {
                            if !loaded_sheets.is_empty() {
                                let mut mapped_sheets = Vec::new();
                                for s in loaded_sheets {
                                    // 初期復元はIndexedDBのcontentまたはtemp_contentを使用
                                    let content = s.temp_content.clone().unwrap_or(s.content.clone());
                                    mapped_sheets.push(Sheet {
                                        id: s.id,
                                        guid: s.guid,
                                        category: s.category,
                                        title: s.title,
                                        content,
                                        is_modified: s.temp_timestamp.is_some(),
                                        drive_id: s.drive_id,
                                        temp_content: s.temp_content,
                                        temp_timestamp: s.temp_timestamp,
                                        last_sync_timestamp: s.last_sync_timestamp,
                                    });
                                }
                                
                                if let Some(last) = mapped_sheets.last() {
                                    active_sheet_id.set(Some(last.id.clone()));
                                }
                                sheets.set(mapped_sheets);
                            } else {
                                // Default sheet if none exists
                                let new_id = js_sys::Date::now().to_string();
                                sheets.set(vec![Sheet {
                                    id: new_id.clone(),
                                    guid: None,
                                    category: "NO_CATEGORY".to_string(),
                                    title: "Untitled 1".to_string(),
                                    content: "".to_string(),
                                    is_modified: false,
                                    drive_id: None,
                                    temp_content: None,
                                    temp_timestamp: None,
                                    last_sync_timestamp: None,
                                }]);
                                active_sheet_id.set(Some(new_id));
                            }
                        }
                    },
                    Err(e) => gloo::console::error!("Failed to load sheets", e),
                }
            });
            || ()
        });
    }

    let on_new_sheet_cb = {
        let sheets = sheets.clone();
        let active_sheet_id = active_sheet_id.clone();
        Callback::from(move |_| {
            let mut current_sheets = (*sheets).clone();
            let new_id = js_sys::Date::now().to_string(); 
            current_sheets.push(Sheet {
                id: new_id.clone(),
                guid: None,
                category: "NO_CATEGORY".to_string(),
                title: format!("Untitled {}", current_sheets.len() + 1),
                content: "".to_string(),
                is_modified: false,
                drive_id: None,
                temp_content: None,
                temp_timestamp: None,
                last_sync_timestamp: None,
            });
            sheets.set(current_sheets);
            active_sheet_id.set(Some(new_id));
        })
    };
    
    let on_save_cb = {
        let active_sheet_id = active_sheet_id.clone();
        let sheets = sheets.clone();
        let no_category_folder_id = no_category_folder_id.clone();
        let network_connected = network_connected.clone();
        Callback::from(move |_| {
            let active_id = (*active_sheet_id).clone();
            if let Some(id) = active_id {
                let content = get_editor_content();
                let mut current_sheets = (*sheets).clone();
                let no_category_id = (*no_category_folder_id).clone();
                
                // より厳格なオンライン判定
                let window = web_sys::window().unwrap();
                let is_online = *network_connected && window.navigator().on_line();

                if let Some(sheet) = current_sheets.iter_mut().find(|s| s.id == id) {
                    sheet.content = content.clone();
                    sheet.is_modified = false;

                    if !is_online {
                        // Offline: Save to temporary
                        gloo::console::log!("Offline detected: Saving to temporary storage");
                        sheet.temp_content = Some(content.clone());
                        sheet.temp_timestamp = Some(js_sys::Date::now() as u64);
                        
                        let sheet_clone = sheet.clone();
                        let sheets_state = sheets.clone();
                        spawn_local(async move {
                            let js_sheet = JSSheet {
                                id: sheet_clone.id.clone(),
                                guid: sheet_clone.guid.clone(),
                                category: sheet_clone.category.clone(),
                                title: sheet_clone.title.clone(),
                                content: sheet_clone.content.clone(),
                                is_modified: false,
                                drive_id: sheet_clone.drive_id.clone(),
                                temp_content: sheet_clone.temp_content.clone(),
                                temp_timestamp: sheet_clone.temp_timestamp,
                                last_sync_timestamp: sheet_clone.last_sync_timestamp,
                            };
                            let serializer = serde_wasm_bindgen::Serializer::json_compatible();
                            if let Ok(val) = js_sheet.serialize(&serializer) {
                                gloo::console::log!("Offline save to DB:", &val);
                                let _ = save_sheet(val).await;
                            }
                            sheets_state.set((*sheets_state).clone());
                        });
                        return;
                    }

                    // Online: Save to Google Drive
                    if no_category_id.is_none() {
                        gloo::console::warn!("Cannot save: NO_CATEGORY folder ID is missing");
                        return;
                    }

                    if sheet.guid.is_none() {
                        sheet.guid = Some(generate_uuid());
                    }

                    gloo::console::log!("Saving sheet to Drive:", &sheet.title);
                    
                    let sheet_clone = sheet.clone();
                    let sheets_state = sheets.clone();
                    let network_connected_err = network_connected.clone();
                    
                    spawn_local(async move {
                         let mut new_drive_id = sheet_clone.drive_id.clone();
                         let mut sync_time = sheet_clone.last_sync_timestamp;

                         if let Some(fid) = no_category_id {
                             let filename = format!("{}.txt", sheet_clone.guid.as_ref().unwrap());
                             let res = upload_file(&filename, &sheet_clone.content, &fid, sheet_clone.drive_id.as_deref()).await;
                             match res {
                                 Ok(res_val) => {
                                     if let Ok(id_val) = js_sys::Reflect::get(&res_val, &JsValue::from_str("id")) {
                                         if let Some(id_str) = id_val.as_string() {
                                             new_drive_id = Some(id_str);
                                         }
                                     }
                                     if let Ok(time_val) = js_sys::Reflect::get(&res_val, &JsValue::from_str("modifiedTime")) {
                                         if let Some(time_str) = time_val.as_string() {
                                             sync_time = Some(crate::drive_interop::parse_date(&time_str) as u64);
                                         }
                                     }
                                 },
                                 Err(e) => {
                                     gloo::console::error!("Upload failed (network error), saving to temporary:", &e);
                                     network_connected_err.set(false); // 通信エラーを検知したらオフラインへ
                                     
                                     let js_sheet = JSSheet {
                                         id: sheet_clone.id.clone(),
                                         guid: sheet_clone.guid.clone(),
                                         category: sheet_clone.category.clone(),
                                         title: sheet_clone.title.clone(),
                                         content: sheet_clone.content.clone(),
                                         is_modified: false,
                                         drive_id: sheet_clone.drive_id.clone(),
                                         temp_content: Some(sheet_clone.content.clone()),
                                         temp_timestamp: Some(js_sys::Date::now() as u64),
                                         last_sync_timestamp: sheet_clone.last_sync_timestamp,
                                     };
                                     let serializer = serde_wasm_bindgen::Serializer::json_compatible();
                                     if let Ok(val) = js_sheet.serialize(&serializer) {
                                         let _ = save_sheet(val).await;
                                     }
                                     return;
                                 },
                             }
                         }

                         let js_sheet = JSSheet {
                             id: sheet_clone.id.clone(),
                             guid: sheet_clone.guid.clone(),
                             category: sheet_clone.category.clone(),
                             title: sheet_clone.title.clone(),
                             content: sheet_clone.content.clone(), // 最新内容を確実に入れる
                             is_modified: false,
                             drive_id: new_drive_id.clone(),
                             temp_content: None, // 明示的にnullにする
                             temp_timestamp: None,
                             last_sync_timestamp: sync_time,
                         };
                         
                         let serializer = serde_wasm_bindgen::Serializer::json_compatible();
                         if let Ok(val) = js_sheet.serialize(&serializer) {
                             gloo::console::log!("Syncing to IndexedDB:", &val);
                             let _ = save_sheet(val).await;
                         }

                         let mut updated_sheets = (*sheets_state).clone();
                         if let Some(s) = updated_sheets.iter_mut().find(|s| s.id == sheet_clone.id) {
                             s.drive_id = new_drive_id;
                             s.content = sheet_clone.content.clone(); // ステートも更新
                             s.is_modified = false;
                             s.temp_content = None;
                             s.temp_timestamp = None;
                             s.last_sync_timestamp = sync_time;
                         }
                         sheets_state.set(updated_sheets);
                    });
                }
                sheets.set(current_sheets);
            }
        })
    };

    let on_close_sheet_cb = {
        let sheets = sheets.clone();
        let active_sheet_id = active_sheet_id.clone();
        Callback::from(move |id: String| {
            let mut current_sheets = (*sheets).clone();
            if let Some(pos) = current_sheets.iter().position(|s| s.id == id) {
                // Warning if not synced? (Optionally implementation)
                current_sheets.remove(pos);
                if (*active_sheet_id).as_ref() == Some(&id) {
                     active_sheet_id.set(if current_sheets.is_empty() { None } else { Some(current_sheets.last().unwrap().id.clone()) }); 
                }
                sheets.set(current_sheets);
                let id_clone = id.clone();
                spawn_local(async move { let _ = delete_sheet(&id_clone).await; });
            }
        })
    };

    // Editor Initialization
    {
        let on_save = on_save_cb.clone();
        let on_new = on_new_sheet_cb.clone();
        let active_sheet_id = active_sheet_id.clone();
        let on_close_sheet_cb = on_close_sheet_cb.clone();
        let is_authenticated_val = *is_authenticated;
        let auto_save_timer = auto_save_timer.clone();
        let sheets = sheets.clone();
        let vim_mode_for_init = vim_mode.clone();
        let no_category_id_val = (*no_category_folder_id).clone();
        let network_connected_for_effect = network_connected.clone();
        
        use_effect_with((is_authenticated_val, no_category_id_val.clone()), move |(auth, folder_id)| {
            if *auth {
                let on_save_inner = on_save.clone();
                let on_new_inner = on_new.clone();
                let active_id_inner = active_sheet_id.clone();
                let on_close_inner = on_close_sheet_cb.clone();
                let sheets_inner = sheets.clone();
                let timer_inner = auto_save_timer.clone();
                let vim_val = *vim_mode_for_init;
                let folder_id_for_cb = folder_id.clone();
                let network_connected_cb = network_connected_for_effect.clone();

                let callback = Closure::wrap(Box::new(move |cmd: String| {
                    if cmd == "save" { on_save_inner.emit(()); }
                    else if cmd == "new_sheet" { on_new_inner.emit(()); }
                    else if cmd == "close" { if let Some(id) = (*active_id_inner).clone() { on_close_inner.emit(id); } }
                    else if cmd == "change" {
                        if folder_id_for_cb.is_none() && *network_connected_cb {
                            // フォルダIDが未確定かつオンラインなら、確定するまで待機
                            return;
                        }
                        
                        gloo::console::log!("Content changed, setting auto-save timer...");
                        if let Some(active_id) = (*active_id_inner).clone() {
                            let mut current_sheets = (*sheets_inner).clone();
                            if let Some(sheet) = current_sheets.iter_mut().find(|s| s.id == active_id) {
                                if !sheet.is_modified {
                                    sheet.is_modified = true;
                                    sheets_inner.set(current_sheets);
                                }
                            }
                        }
                        // Reset and set 1s timer
                        let on_save_auto = on_save_inner.clone();
                        let timeout = Timeout::new(1000, move || {
                            gloo::console::log!("Auto-save timer fired!");
                            on_save_auto.emit(());
                        });
                        timer_inner.set(Some(timeout));
                    }
                }) as Box<dyn FnMut(String)>);
                init_editor("editor", &callback);
                set_vim_mode(vim_val);
                callback.forget();
            }
            || ()
        });
    }

    // Sync content
    {
        let active_sheet_id = (*active_sheet_id).clone();
        let sheets_val = (*sheets).clone();
        let is_authenticated_val = *is_authenticated;
        use_effect_with((active_sheet_id, is_authenticated_val), move |(active_id, auth)| {
            if *auth {
                if let Some(id) = active_id {
                    if let Some(sheet) = sheets_val.iter().find(|s| s.id == *id) {
                        set_editor_content(&sheet.content);
                    }
                }
            }
            || ()
        });
    }

    // Vim mode
    {
        let vim_mode_val = *vim_mode;
        let is_authenticated_val = *is_authenticated;
        use_effect_with((vim_mode_val, is_authenticated_val), move |&(vim, auth)| {
            if auth { set_vim_mode(vim); }
            || ()
        });
    }

    // Resize Handling
    {
        let is_authenticated_val = *is_authenticated;
        use_effect_with(is_authenticated_val, move |&auth| {
            let mut listener = None;
            if auth {
                let window = web_sys::window().unwrap();
                listener = Some(EventListener::new(&window, "resize", move |_| {
                    resize_editor();
                }));
            }
            move || { drop(listener); }
        });
    }

    let on_toggle_vim = {
        let vim_mode = vim_mode.clone();
        Callback::from(move |_| { vim_mode.set(!*vim_mode); })
    };

    let on_login = Callback::from(|_| { request_access_token(); });

    let on_select_sheet = {
        let sheets = sheets.clone();
        let active_sheet_id = active_sheet_id.clone();
        Callback::from(move |id: String| {
            if let Some(current_id) = (*active_sheet_id).as_ref() {
                let content = get_editor_content();
                let mut current_sheets = (*sheets).clone();
                if let Some(sheet) = current_sheets.iter_mut().find(|s| s.id == *current_id) {
                    if sheet.content != content {
                         sheet.content = content;
                         sheet.is_modified = true;
                    }
                }
                sheets.set(current_sheets);
            }
            active_sheet_id.set(Some(id));
        })
    };
    
    if !*is_authenticated {
        return html! {
             <main class={classes!("flex", "h-screen", "w-screen", "items-center", "justify-center", "bg-gray-900", "text-white")}>
                <div class={classes!("text-center")}>
                    <h1 class={classes!("text-4xl", "font-bold", "mb-8")}>{ app_name }</h1>
                    <button onclick={on_login} class={classes!("bg-blue-600", "hover:bg-blue-700", "text-white", "font-bold", "py-2", "px-4", "rounded", "transition-colors")}>
                        { "Sign in with Google" }
                    </button>
                    <div class={classes!("mt-4", "text-gray-400", "text-sm")}>{ "Please sign in to access your files." }</div>
                </div>
            </main>
        };
    }

    // --- ダイアログ処理 ---
    let on_dialog_confirm = {
        let conflict_queue = conflict_queue.clone();
        let sheets = sheets.clone();
        let is_loading = is_loading.clone();
        let is_fading_out = is_fading_out.clone();
        let no_category_folder_id = no_category_folder_id.clone();
        let active_sheet_id = active_sheet_id.clone();

        Callback::from(move |selection: usize| {
            let mut queue = (*conflict_queue).clone();
            if queue.is_empty() { return; }
            let conflict = queue.remove(0); // 現在の競合データ
            
            let sheets_state = sheets.clone();
            let queue_state = conflict_queue.clone();
            let is_loading = is_loading.clone();
            let is_fading_out = is_fading_out.clone();
            let folder_id = (*no_category_folder_id).clone();
            let active_id_val = (*active_sheet_id).clone();

            spawn_local(async move {
                let mut updated_sheets = (*sheets_state).clone();
                if let Some(sheet) = updated_sheets.iter_mut().find(|s| s.id == conflict.sheet_id) {
                    match selection {
                        0 => { // Googleドライブのデータを読み込む
                            if let Ok(d_content) = crate::drive_interop::download_file(&conflict.drive_id).await {
                                if let Some(text) = d_content.as_string() {
                                    sheet.content = text.clone();
                                    sheet.temp_content = None;
                                    sheet.temp_timestamp = None;
                                    sheet.last_sync_timestamp = Some(conflict.drive_time);
                                    sheet.is_modified = false;
                                    if Some(sheet.id.clone()) == active_id_val { set_editor_content(&text); }
                                }
                            }
                        },
                        1 => { // ローカルのデータをGoogleドライブへ上書き保存
                            let _ = upload_file(&format!("{}.txt", sheet.guid.as_ref().unwrap()), &sheet.content, folder_id.as_deref().unwrap(), Some(&conflict.drive_id)).await;
                            sheet.temp_content = None;
                            sheet.temp_timestamp = None;
                            sheet.last_sync_timestamp = Some(js_sys::Date::now() as u64);
                            sheet.is_modified = false;
                        },
                        2 => { // ローカルのデータをGoogleドライブへ新規ファイルとして保存
                            let new_guid = generate_uuid();
                            let _ = upload_file(&format!("{}.txt", new_guid), &sheet.content, folder_id.as_deref().unwrap(), None).await;
                            sheet.guid = Some(new_guid);
                            sheet.temp_content = None;
                            sheet.temp_timestamp = None;
                            sheet.last_sync_timestamp = Some(js_sys::Date::now() as u64);
                            sheet.is_modified = false;
                        },
                        _ => {}
                    }

                    // IndexedDB保存
                    let js_sheet = JSSheet {
                        id: sheet.id.clone(),
                        guid: sheet.guid.clone(),
                        category: sheet.category.clone(),
                        title: sheet.title.clone(),
                        content: sheet.content.clone(),
                        is_modified: sheet.is_modified,
                        drive_id: sheet.drive_id.clone(),
                        temp_content: sheet.temp_content.clone(),
                        temp_timestamp: sheet.temp_timestamp,
                        last_sync_timestamp: sheet.last_sync_timestamp,
                    };
                    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
                    if let Ok(val) = js_sheet.serialize(&serializer) { let _ = save_sheet(val).await; }
                }
                
                sheets_state.set(updated_sheets);
                queue_state.set(queue.clone());

                if queue.is_empty() {
                    // 全ての競合が解決したらフェードアウト開始
                    is_fading_out.set(true);
                    Timeout::new(300, move || { is_loading.set(false); }).forget();
                }
            });
        })
    };

    html! {
        <div class="relative h-screen w-screen overflow-hidden">
            <main class={classes!("flex", "flex-col", "bg-gray-900", "text-white", "h-full", "w-full")}>
                <ButtonBar vim_mode={*vim_mode} on_toggle_vim={on_toggle_vim} on_new_sheet={on_new_sheet_cb.clone()} on_save={on_save_cb} />
                <TabArea sheets={(*sheets).clone()} active_sheet_id={(*active_sheet_id).clone()} on_select_sheet={on_select_sheet} on_close_sheet={on_close_sheet_cb} on_new_sheet={on_new_sheet_cb} />
                
                <div id="editor" class={classes!("flex-1", "bg-gray-950", "z-10")} style="width: 100%; min-height: 0;"></div>
                
                <StatusBar network_status={*network_connected} />
            </main>

            if let Some(conflict) = conflict_queue.first() {
                <CustomDialog 
                    title="データの競合を検知"
                    message={format!("シート \"{}\" の新しいバージョンがGoogleドライブで見つかりました。実行するアクションを選択してください。", conflict.title)}
                    options={vec![
                        DialogOption { id: 0, label: "Googleドライブのデータを読み込む".to_string() },
                        DialogOption { id: 1, label: "ローカルのデータをGoogleドライブへ上書き保存".to_string() },
                        DialogOption { id: 2, label: "ローカルのデータをGoogleドライブへ新規ファイルとして保存".to_string() },
                    ]}
                    on_confirm={on_dialog_confirm}
                />
            }

            if *is_loading {
                <div class={classes!(
                    "fixed", "inset-0", "z-50", "flex", "items-center", "justify-center", "bg-black/80", "backdrop-blur-sm", "transition-opacity", "duration-300",
                    if *is_fading_out { "opacity-0" } else { "opacity-100" }
                )}>
                    <div class="flex flex-col items-center">
                        <img src="icon.svg" class="mb-8 shadow-2xl" style="width: 20vmin; height: 20vmin;" alt="Leaf Icon" />
                        <div class="w-12 h-12 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"></div>
                        <p class="mt-4 text-white font-bold text-lg">{ "Synchronizing..." }</p>
                    </div>
                </div>
            }
        </div>
    }
}
