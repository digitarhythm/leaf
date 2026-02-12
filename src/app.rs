use yew::prelude::*;
use crate::components::button_bar::ButtonBar;
use crate::components::status_bar::StatusBar;
use crate::components::tab_area::{TabArea, Sheet};
use crate::js_interop::{init_editor, set_vim_mode, resize_editor, get_editor_content, set_editor_content, set_window_title, generate_uuid};
use crate::auth_interop::{init_google_auth, request_access_token};
use crate::db_interop::{init_db, save_sheet, load_sheets, delete_sheet, JSSheet};
use crate::drive_interop::{upload_file, ensure_directory_structure};
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::spawn_local;
use gloo::events::EventListener;
use gloo::timers::callback::Timeout;
use serde::Deserialize;
use wasm_bindgen::JsValue;

#[derive(Deserialize)]
struct Config {
    app_name: String,
    google_client_id: String,
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
    let sheets = use_state(|| vec![]);
    let active_sheet_id = use_state(|| None::<String>);
    let network_connected = use_state(|| true);
    let is_authenticated = use_state(|| false);
    let no_category_folder_id = use_state(|| None::<String>);
    let auto_save_timer = use_state(|| None::<Timeout>);
    
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
                                let mapped_sheets: Vec<Sheet> = loaded_sheets.into_iter().map(|s| Sheet {
                                    id: s.id,
                                    guid: s.guid,
                                    category: s.category,
                                    title: s.title,
                                    content: s.content,
                                    is_modified: s.is_modified,
                                    drive_id: s.drive_id,
                                }).collect();
                                
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
            });
            sheets.set(current_sheets);
            active_sheet_id.set(Some(new_id));
        })
    };
    
    let on_save_cb = {
        let active_sheet_id = active_sheet_id.clone();
        let sheets = sheets.clone();
        let no_category_folder_id = no_category_folder_id.clone();
        Callback::from(move |_| {
            let active_id = (*active_sheet_id).clone();
            if let Some(id) = active_id {
                let content = get_editor_content();
                let mut current_sheets = (*sheets).clone();
                let no_category_id = (*no_category_folder_id).clone();

                if let Some(sheet) = current_sheets.iter_mut().find(|s| s.id == id) {
                    // Generate GUID if not exists
                    if sheet.guid.is_none() {
                        sheet.guid = Some(generate_uuid());
                    }

                    sheet.content = content.clone();
                    sheet.is_modified = false; 
                    
                    let sheet_clone = sheet.clone();
                    let sheets_state = sheets.clone();
                    
                    spawn_local(async move {
                         let mut new_drive_id = sheet_clone.drive_id.clone();
                         if let Some(fid) = no_category_id {
                             let filename = format!("{}.txt", sheet_clone.guid.as_ref().unwrap());
                             let res = upload_file(&filename, &sheet_clone.content, &fid, sheet_clone.drive_id.as_deref()).await;
                             if let Ok(res_val) = res {
                                 if let Ok(id_val) = js_sys::Reflect::get(&res_val, &JsValue::from_str("id")) {
                                     if let Some(id_str) = id_val.as_string() {
                                         new_drive_id = Some(id_str);
                                     }
                                 }
                             }
                         }

                         let js_sheet = JSSheet {
                             id: sheet_clone.id.clone(),
                             guid: sheet_clone.guid.clone(),
                             category: sheet_clone.category.clone(),
                             title: sheet_clone.title.clone(),
                             content: sheet_clone.content.clone(),
                             is_modified: false,
                             drive_id: new_drive_id.clone(),
                         };
                         
                         if let Ok(val) = serde_wasm_bindgen::to_value(&js_sheet) {
                             let _ = save_sheet(val).await;
                         }

                         let mut updated_sheets = (*sheets_state).clone();
                         if let Some(s) = updated_sheets.iter_mut().find(|s| s.id == sheet_clone.id) {
                             s.drive_id = new_drive_id;
                             s.is_modified = false;
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
        let vim_mode_val = *vim_mode;
        
        use_effect_with(is_authenticated_val, move |&auth| {
            if auth {
                let callback = Closure::wrap(Box::new(move |cmd: String| {
                    if cmd == "save" { on_save.emit(()); }
                    else if cmd == "new_sheet" { on_new.emit(()); }
                    else if cmd == "close" { if let Some(id) = (*active_sheet_id).clone() { on_close_sheet_cb.emit(id); } }
                    else if cmd == "change" {
                        if let Some(active_id) = (*active_sheet_id).clone() {
                            let mut current_sheets = (*sheets).clone();
                            if let Some(sheet) = current_sheets.iter_mut().find(|s| s.id == active_id) {
                                if !sheet.is_modified {
                                    sheet.is_modified = true;
                                    sheets.set(current_sheets);
                                }
                            }
                        }
                        // Auto-save timer
                        let on_save_auto = on_save.clone();
                        let timeout = Timeout::new(5000, move || { on_save_auto.emit(()); });
                        auto_save_timer.set(Some(timeout));
                    }
                }) as Box<dyn FnMut(String)>);
                init_editor("editor", &callback);
                set_vim_mode(vim_mode_val);
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

    html! {
        <main class={classes!("flex", "flex-col", "bg-gray-900", "text-white", "overflow-hidden")} style="height: 100vh; width: 100vw; display: flex; flex-direction: column;">
            <ButtonBar vim_mode={*vim_mode} on_toggle_vim={on_toggle_vim} on_new_sheet={on_new_sheet_cb.clone()} on_save={on_save_cb} />
            <TabArea sheets={(*sheets).clone()} active_sheet_id={(*active_sheet_id).clone()} on_select_sheet={on_select_sheet} on_close_sheet={on_close_sheet_cb} on_new_sheet={on_new_sheet_cb} />
            
            <div id="editor" class={classes!("flex-1", "bg-gray-950", "z-10")} style="width: 100%; min-height: 0;"></div>
            
            <StatusBar network_status={*network_connected} />
        </main>
    }
}
