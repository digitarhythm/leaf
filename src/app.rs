use yew::prelude::*;
use crate::components::button_bar::ButtonBar;
use crate::components::status_bar::StatusBar;
use crate::components::tab_bar::{TabBar, TabInfo, SheetListPanel};
use crate::components::dialog::{CustomDialog, DialogOption, ConfirmDialog, NameConflictDialog, LoadingOverlay};
use crate::components::file_open_dialog::FileOpenDialog;
use crate::components::preview::Preview;
use crate::components::settings_dialog::SettingsDialog;
use crate::js_interop::{init_editor, set_vim_mode, get_editor_content, set_editor_content, load_editor_content, focus_editor, set_gutter_status, set_preview_active, generate_uuid, open_local_file, save_local_file, clear_local_handle};
use crate::auth_interop::request_access_token;
use crate::db_interop::{save_sheet, save_categories, JSCategory, JSSheet};
use crate::drive_interop::{upload_file, ensure_directory_structure, list_folders, download_file, list_files, get_file_metadata, delete_file, move_file, find_file_by_name};
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
    pub total_size: u64,
    pub loaded_bytes: u64,
    pub needs_bom: bool,
    pub is_preview: bool,
}

fn has_utf8_bom(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF
}

pub const SUPPORTED_EXTENSIONS: &[(&str, &str)] = &[
    ("txt", "ext_txt"),
    ("md", "ext_md"),
    ("js", "ext_js"),
    ("ts", "ext_ts"),
    ("rs", "ext_rs"),
    ("c", "ext_c"),
    ("cpp", "ext_cpp"),
    ("h", "ext_h"),
    ("m", "ext_m"),
    ("cs", "ext_cs"),
    ("java", "ext_java"),
    ("php", "ext_php"),
    ("rb", "ext_rb"),
    ("pl", "ext_pl"),
    ("py", "ext_py"),
    ("sh", "ext_sh"),
    ("coffee", "ext_coffee"),
    ("toml", "ext_toml"),
    ("json", "ext_json"),
    ("xml", "ext_xml"),
    ("html", "ext_html"),
    ("css", "ext_css"),
    ("sql", "ext_sql"),
    ("yaml", "ext_yaml"),
];

impl Sheet {
    fn to_js(&self) -> JSSheet {
        JSSheet {
            id: self.id.clone(),
            guid: self.guid.clone(),
            category: self.category.clone(),
            title: self.title.clone(),
            content: self.content.clone(),
            is_modified: self.is_modified,
            drive_id: self.drive_id.clone(),
            temp_content: self.temp_content.clone(),
            temp_timestamp: self.temp_timestamp,
            last_sync_timestamp: self.last_sync_timestamp,
            tab_color: self.tab_color.clone(),
            total_size: self.total_size,
            loaded_bytes: self.loaded_bytes,
            needs_bom: self.needs_bom,
            is_preview: self.is_preview,
        }
    }
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
const PREVIEW_FONT_SIZE_KEY: &str = "leaf_preview_font_size";
const FIRST_LAUNCH_KEY: &str = "leaf_first_launch_v1";
const ACTIVE_TAB_KEY: &str = "leaf_active_tab";

/// アカウント別のlocalStorageキーを返す
fn account_key(base_key: &str) -> String {
    let email = crate::auth_interop::get_user_email();
    if let Some(email) = email.as_string() {
        if !email.is_empty() {
            return format!("{}_{}", base_key, email);
        }
    }
    base_key.to_string()
}

/// アカウント別のlocalStorageから値を取得
fn get_account_storage(base_key: &str) -> Option<String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(&account_key(base_key)).ok().flatten())
}

/// アカウント別のlocalStorageに値を保存
fn set_account_storage(base_key: &str, value: &str) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(&account_key(base_key), value);
    }
}

/// アカウント別のIndexedDB名を返す
fn account_db_name() -> String {
    let email = crate::auth_interop::get_user_email();
    if let Some(email) = email.as_string() {
        if !email.is_empty() {
            return format!("LeafDB_{}", email);
        }
    }
    "LeafDB".to_string()
}

fn generate_random_color() -> String {
    let h = (js_sys::Math::random() * 360.0) as u32;
    let s = 40 + (js_sys::Math::random() * 30.0) as u32;
    let l = 40 + (js_sys::Math::random() * 20.0) as u32;
    format!("hsl({}, {}%, {}%)", h, s, l)
}

fn render_preview_inline(
    active_sheet_id: &UseStateHandle<Option<String>>,
    sheets: &UseStateHandle<Vec<Sheet>>,
    file_ext: &str,
    font_size: i32,
) -> Html {
    use yew::AttrValue;
    let aid = (**active_sheet_id).clone();
    let content = if let Some(id) = aid {
        crate::js_interop::get_editor_content().as_string().unwrap_or_else(|| {
            sheets.iter().find(|s| s.id == id).map(|s| s.content.clone()).unwrap_or_default()
        })
    } else { "".to_string() };
    let is_markdown = file_ext == "md" || file_ext == "markdown";
    let rendered_html = if is_markdown {
        crate::js_interop::render_markdown(&content)
    } else {
        let code_html = crate::js_interop::highlight_code(&content, file_ext);
        format!(r#"<pre class="hljs whitespace-pre-wrap break-all"><code class="hljs language-{}">{}</code></pre>"#, file_ext, code_html)
    };
    html! {
        <div class="absolute inset-0 z-20 overflow-y-auto bg-[#1a1b26] p-6 sm:p-12">
            <div
                class={classes!(if is_markdown { "markdown-body" } else { "hljs" }, "max-w-none")}
                style={format!("font-size: {}pt;", font_size)}
            >
                { Html::from_html_unchecked(AttrValue::from(rendered_html)) }
            </div>
        </div>
    }
}

fn close_tab_direct(
    close_id: String,
    rs: Rc<RefCell<Vec<Sheet>>>,
    s_state: UseStateHandle<Vec<Sheet>>,
    aid: UseStateHandle<Option<String>>,
    sp: UseStateHandle<bool>,
    ncid: UseStateHandle<Option<String>>,
    aid_ref: Option<Rc<RefCell<Option<String>>>>,
) {
    sp.set(true);
    let mut us = (*rs.borrow()).clone();
    let pos = us.iter().position(|s| s.id == close_id);
    if let Some(pos) = pos {
        let sheet_id = us[pos].id.clone();
        us.remove(pos);

        if us.is_empty() {
            // 最後のタブ → 新規シート自動作成
            let cat_id = (*ncid).clone().unwrap_or_default();
            let nid = js_sys::Date::now().to_string();
            let ns = Sheet {
                id: nid.clone(), guid: None, category: cat_id, title: "Untitled.txt".to_string(),
                content: "".to_string(), is_modified: false, drive_id: None, temp_content: None,
                temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(),
                total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false,
            };
            us.push(ns.clone());
            *rs.borrow_mut() = us.clone();
            s_state.set(us);
            aid.set(Some(nid.clone()));
            if let Some(ref r) = aid_ref { *r.borrow_mut() = Some(nid.clone()); }
            load_editor_content("");
            set_gutter_status("unsaved");
            let sp_inner = sp.clone();
            Timeout::new(100, move || { sp_inner.set(false); focus_editor(); }).forget();
            spawn_local(async move {
                let js = ns.to_js();
                let ser = serde_wasm_bindgen::Serializer::json_compatible();
                if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
            });
        } else {
            // 閉じたタブがアクティブだった場合、隣接タブに切り替え（左優先）
            // RefCellから最新のactive_idを取得（UseStateHandleはstaleの可能性がある）
            let was_active = if let Some(ref r) = aid_ref {
                r.borrow().as_ref() == Some(&close_id)
            } else {
                aid.as_ref() == Some(&close_id)
            };
            *rs.borrow_mut() = us.clone();
            s_state.set(us.clone());

            if was_active {
                let new_idx = if pos > 0 { pos - 1 } else { 0 };
                let new_sheet = &us[new_idx];
                let new_id = new_sheet.id.clone();
                load_editor_content(&new_sheet.content);
                crate::js_interop::set_editor_mode(&new_sheet.title);
                if new_sheet.drive_id.is_none() && new_sheet.guid.is_none() {
                    if new_sheet.category == "__LOCAL__" { set_gutter_status("local"); } else { set_gutter_status("unsaved"); }
                } else if new_sheet.is_modified {
                    set_gutter_status("unsaved");
                } else {
                    set_gutter_status("none");
                }
                aid.set(Some(new_id.clone()));
                if let Some(ref r) = aid_ref { *r.borrow_mut() = Some(new_id); }
            }
            let sp_inner = sp.clone();
            Timeout::new(100, move || { sp_inner.set(false); focus_editor(); }).forget();
        }

        // IndexedDBから削除
        spawn_local(async move {
            let _ = crate::db_interop::delete_sheet(&sheet_id).await;
        });
    }
}

fn trigger_conflict_check(
    aid_ref: Rc<RefCell<Option<String>>>,
    s_ref: Rc<RefCell<Vec<Sheet>>>,
    s_state: UseStateHandle<Vec<Sheet>>,
    ild: UseStateHandle<bool>,
    ifo: UseStateHandle<bool>,
    lmk: UseStateHandle<&'static str>,
    is_init: Option<UseStateHandle<bool>>,
    on_save: Callback<bool>
) {
    let aid = (*aid_ref.borrow()).clone();
    let sheets = (*s_ref.borrow()).clone();

    if let Some(id) = aid {
        if let Some(sheet) = sheets.iter().find(|s| s.id == id) {
            if let Some(drive_id) = &sheet.drive_id {
                let drive_id = drive_id.clone();
                let sheet_id = sheet.id.clone();
                let is_modified = sheet.is_modified;
                // ローカルの最終更新時刻（tempがあればそれを、なければ同期時刻を使用）
                let local_time = sheet.temp_timestamp.unwrap_or_else(|| sheet.last_sync_timestamp.unwrap_or(0));
                let last_sync = sheet.last_sync_timestamp.unwrap_or(0);
                let ild_inner = ild.clone();
                let ifo_inner = ifo.clone();
                let lmk_inner = lmk.clone();
                let is_init_inner = is_init.clone();
                let on_save_inner = on_save.clone();

                let s_ref_inner = s_ref.clone();
                let s_state_inner = s_state.clone();
                // 競合チェック開始（オーバーレイは出さず、バックグラウンドでメタデータを確認）
                spawn_local(async move {
                    if let Ok(metadata) = get_file_metadata(&drive_id).await {
                        if let Ok(time_val) = js_sys::Reflect::get(&metadata, &JsValue::from_str("modifiedTime")) {
                            if let Some(time_str) = time_val.as_string() {
                                let drive_time = crate::drive_interop::parse_date(&time_str) as u64;
                                gloo::console::log!(format!("[Leaf-SYSTEM] Sync Check: drive={}, last_sync={}, local_temp={}, is_modified={}", drive_time, last_sync, local_time, is_modified));

                                if drive_time > last_sync + 1000 {
                                    // Googleドライブの方が新しい → ダイアログを出さずにDriveの内容で自動更新
                                    lmk_inner.set("synchronizing");
                                    ild_inner.set(true);
                                    ifo_inner.set(false);

                                    gloo::console::log!(format!("[Leaf-SYSTEM] Drive is newer (drive_time={}, last_sync={}). Auto-loading from Drive.", drive_time, last_sync));
                                    if let Ok(drive_bytes) = download_file(&drive_id, None, None).await {
                                        let decoder = js_sys::Reflect::get(&web_sys::window().unwrap(), &JsValue::from_str("TextDecoder")).unwrap();
                                        let decoder_instance = js_sys::Reflect::construct(&decoder.into(), &js_sys::Array::of1(&JsValue::from_str("utf-8"))).unwrap();
                                        let decode_fn = js_sys::Reflect::get(&decoder_instance, &JsValue::from_str("decode")).unwrap();
                                        let drive_content = js_sys::Reflect::apply(&decode_fn.into(), &decoder_instance, &js_sys::Array::of1(&drive_bytes)).unwrap().as_string().unwrap_or_default();
                                        let content_len = drive_content.len() as u64;

                                        let mut us = (*s_ref_inner.borrow()).clone();
                                        if let Some(s) = us.iter_mut().find(|x| x.id == sheet_id) {
                                            s.content = drive_content.clone();
                                            s.temp_content = None;
                                            s.temp_timestamp = None;
                                            s.last_sync_timestamp = Some(drive_time);
                                            s.is_modified = false;
                                            s.loaded_bytes = content_len;
                                            s.total_size = content_len;
                                            let js = s.to_js();
                                            let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                            if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                        }
                                        *s_ref_inner.borrow_mut() = us.clone();
                                        s_state_inner.set(us);
                                        load_editor_content(&drive_content);
                                    }
                                    let ild = ild_inner.clone(); let ifo = ifo_inner.clone(); let isi = is_init_inner.clone();
                                    ifo.set(true);
                                    Timeout::new(300, move || { ild.set(false); ifo.set(false); if let Some(h) = isi { h.set(false); } }).forget();
                                } else {
                                    // ローカルの方が新しい、または一致
                                    if is_modified || local_time > drive_time + 1000 {
                                        gloo::console::log!(format!("[Leaf-SYSTEM] Local is newer. Triggering silent auto-upload..."));
                                        on_save_inner.emit(false);
                                    } else {
                                        gloo::console::log!("[Leaf-SYSTEM] No sync needed.");
                                    }

                                    // 初期ロード画面があれば解除
                                    let ild = ild_inner.clone(); let ifo = ifo_inner.clone(); let isi = is_init_inner.clone();
                                    if *ild || isi.as_ref().map(|v| **v).unwrap_or(false) {
                                        ifo.set(true);
                                        Timeout::new(300, move || { ild.set(false); ifo.set(false); if let Some(h) = isi { h.set(false); } }).forget();
                                    }
                                }
                            }
                        }
                    } else {
                        gloo::console::error!("[Leaf-SYSTEM] Failed to get file metadata.");
                        // 失敗しても初期ロード画面は解除する
                        if let Some(ref h) = is_init_inner {
                            let ild = ild_inner.clone(); let ifo = ifo_inner.clone(); let isi = h.clone();
                            ifo.set(true);
                            Timeout::new(300, move || { ild.set(false); isi.set(false); ifo.set(false); }).forget();
                        }
                    }
                });
                return;
            }
        }
    }
    
    // 衝突チェックが不要な場合（ローカルファイル等）は即座にローディング解除
    ifo.set(true);
    let ifo_final = ifo.clone();
    let ild_final = ild.clone();
    let init_final = is_init.clone();
    Timeout::new(300, move || {
        ild_final.set(false);
        ifo_final.set(false);
        if let Some(h) = init_final { h.set(false); }
    }).forget();
}

#[function_component(App)]
pub fn app() -> Html {
    let lang = Language::detect();
    let config_str = include_str!("../application.toml");
    let config: Config = toml::from_str(config_str).expect("Failed to parse application.toml");
    let client_id = config.google_client_id.clone();

    let vim_mode = use_state(|| {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item(VIM_MODE_KEY).ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(true)
    });
    let sheets = use_state(|| Vec::<Sheet>::new());
    let active_sheet_id = use_state(|| None::<String>);
    let network_connected = use_state(|| {
        web_sys::window().and_then(|w| Some(w.navigator().on_line())).unwrap_or(true)
    });
    let is_authenticated = use_state(|| false);
    let is_auth_flag = use_mut_ref(|| false); // タイムアウトクロージャ用の共有フラグ
    let no_category_folder_id = use_state(|| None::<String>);
    let leaf_data_folder_id = use_state(|| None::<String>);
    let auto_save_timer = use_state(|| None::<Timeout>);
    let is_loading = use_state(|| true);
    let saving_sheet_id = use_state(|| None::<String>);
    let is_creating_new = use_state(|| false); // 新規作成連打防止用
    let is_import_lock = use_state(|| false);
    let is_import_fading_out = use_state(|| false);
    let is_initial_load = use_state(|| true);
    let loading_message_key = use_state(|| "synchronizing");
    let is_fading_out = use_state(|| false);
    let is_category_dropdown_open = use_state(|| false);
    let categories = use_state(|| Vec::<JSCategory>::new());
    let db_ready_state = use_state(|| false);
    let conflict_queue = use_state(|| Vec::<ConflictData>::new());
    let name_conflict_queue = use_state(|| Vec::<NameConflictData>::new());
    let fallback_queue = use_state(|| Vec::<String>::new());
    let is_logout_confirm_visible = use_state(|| false);
    let is_file_open_dialog_visible = use_state(|| false);
    let file_close_trigger = use_state(|| 0u32);
    let is_creating_category = use_state(|| false);
    let is_file_dialog_sub_active = use_state(|| false);
    let file_refresh_trigger = use_state(|| 0usize);
    let is_file_list_loading = use_state(|| false);
    let font_size = use_state(|| crate::js_interop::get_font_size());
    let preview_font_size = use_state(|| {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item("leaf_preview_font_size").ok().flatten())
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or_else(|| crate::js_interop::get_font_size())
    });
    let is_preview_visible = use_state(|| false);
    let is_preview_fading_out = use_state(|| false);
    let is_help_visible = use_state(|| false);
    let is_suppressing_changes = use_state(|| false); 
    let pending_delete_category = use_state(|| None::<String>);
    let is_processing_dialog = use_state(|| false);
    let is_install_confirm_visible = use_state(|| false);
    let is_settings_visible = use_state(|| false);
    let is_install_manual_visible = use_state(|| false);

    let is_ad_free = use_state(|| false);
    let pending_close_tab = use_state(|| None::<String>);
    let pending_close_unsynced_tab = use_state(|| None::<String>);
    let pending_save_close_tab = use_state(|| None::<String>);
    let is_sheet_list_visible = use_state(|| false);

    let sheets_ref = use_mut_ref(|| Vec::<Sheet>::new());
    let active_id_ref = use_mut_ref(|| None::<String>);
    let no_category_id_ref = use_mut_ref(|| None::<String>);
    let is_loading_ref = use_mut_ref(|| true);
    let saving_id_ref = use_mut_ref(|| None::<String>);
    let is_suppressing_ref = use_mut_ref(|| false);
    let is_first_edit_done_ref = use_mut_ref(|| false);
    let is_preview_ref = use_mut_ref(|| false);
    let is_file_open_ref = use_mut_ref(|| false);
    let is_help_ref = use_mut_ref(|| false);

    // AdSenseスクリプト読み込み（Tauri以外）
    {
        use_effect_with((), |_| {
            if !crate::js_interop::is_tauri() {
                crate::adsense_interop::load_adsense_script();
            }
            || ()
        });
    }

    {
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let check_size = {
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

                    if let Some(doc) = window_c.document() {
                        if let Some(body) = doc.body() {
                            if is_portrait {
                                let _ = body.class_list().add_1("leaf-mobile-mode");
                            } else {
                                let _ = body.class_list().remove_1("leaf-mobile-mode");
                            }
                        }
                    }
                }
            };
            check_size();
            let listener = EventListener::new(&window, "resize", move |_| { check_size(); });
            move || { drop(listener); }
        })
    }

    {
        let fs = font_size.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let fs_c = fs.clone();
            let listener = EventListener::new(&window, "leaf-font-size-changed", move |e| {
                let ce = e.unchecked_ref::<web_sys::CustomEvent>();
                if let Ok(val) = js_sys::Reflect::get(&ce.detail(), &JsValue::from_f64(0.0)) {
                    if let Some(n) = val.as_f64() { fs_c.set(n as i32); return; }
                }
                if let Some(n) = ce.detail().as_f64() { fs_c.set(n as i32); }
            });
            move || { drop(listener); }
        });
    }

    {
        let is_auth = is_authenticated.clone();
        let is_help = is_help_visible.clone();
        use_effect_with(is_auth, move |auth| {
            if **auth {
                let first_launch = get_account_storage(FIRST_LAUNCH_KEY).is_none();
                if first_launch {
                    is_help.set(true);
                    set_account_storage(FIRST_LAUNCH_KEY, "done");
                }
            }
            || ()
        });
    }

    {
        let s = sheets.clone(); let aid = active_sheet_id.clone(); let ncid = no_category_folder_id.clone();
        let ld = is_loading.clone(); let sp = is_suppressing_changes.clone();
        let prev = is_preview_visible.clone(); let open = is_file_open_dialog_visible.clone(); let help = is_help_visible.clone();
        
        let r_s = sheets_ref.clone(); let r_aid = active_id_ref.clone();
        let r_ncid = no_category_id_ref.clone(); let r_ld = is_loading_ref.clone(); let r_sp = is_suppressing_ref.clone();
        let r_prev = is_preview_ref.clone(); let r_open = is_file_open_ref.clone(); let r_help = is_help_ref.clone();
        let r_saving = saving_id_ref.clone();

        use_effect_with((((*s).clone(), (*aid).clone(), (*ncid).clone()), (*ld, *sp, *prev, *open, *help, (*saving_sheet_id).clone())), move |deps| {
            let ((s_val, aid_val, ncid_val), (ld_val, sp_val, prev_val, open_val, help_val, saving_val)) = deps;
            *r_s.borrow_mut() = s_val.clone(); *r_aid.borrow_mut() = aid_val.clone();
            *r_ncid.borrow_mut() = ncid_val.clone(); *r_ld.borrow_mut() = *ld_val; *r_sp.borrow_mut() = *sp_val;
            *r_prev.borrow_mut() = *prev_val; *r_open.borrow_mut() = *open_val; *r_help.borrow_mut() = *help_val;
            *r_saving.borrow_mut() = saving_val.clone();
            || ()
        });
    }

    let on_login = Callback::from(|_: MouseEvent| { request_access_token(); });
    let on_toggle_vim = {
        let vim = vim_mode.clone();
        Callback::from(move |_| {
            let next = !*vim;
            vim.set(next);
            set_account_storage(VIM_MODE_KEY, if next { "true" } else { "false" });
            set_vim_mode(next);
        })
    };
    let on_change_preview_font_size = {
        let pfs = preview_font_size.clone();
        Callback::from(move |delta: i32| {
            let current = *pfs;
            let new_size = std::cmp::max(8, std::cmp::min(72, current + delta));
            pfs.set(new_size);
            set_account_storage(PREVIEW_FONT_SIZE_KEY, &new_size.to_string());
        })
    };

    let on_change_font_size = {
        let fs = font_size.clone();
        Callback::from(move |delta: i32| { 
            let new_size = crate::js_interop::change_font_size(delta);
            fs.set(new_size);
        })
    };
    let on_logout = { let ic = is_logout_confirm_visible.clone(); Callback::from(move |_| { ic.set(true); }) };

    let on_refresh_cats_cb = {
        let ldid_s = leaf_data_folder_id.clone(); let cats_s = categories.clone();
        let s_state = sheets.clone(); let aid_handle = active_sheet_id.clone();
        let ifo = is_file_open_dialog_visible.clone(); let il = is_loading.clone();
        let nc = network_connected.clone();
        Callback::from(move |_: ()| {
            if let Some(id) = (*ldid_s).clone() {
                let cs = cats_s.clone(); let ss_inner = s_state.clone(); let aid_inner = aid_handle.clone();
                let ifod = ifo.clone(); let ild_final = il.clone();
                let nc_inner = nc.clone();
                spawn_local(async move {
                    if let Ok(cr) = list_folders(&id).await {
                        nc_inner.set(true); // 成功したのでオンラインに
                        if let Ok(fv) = js_sys::Reflect::get(&cr, &JsValue::from_str("files")) {
                            let fa = js_sys::Array::from(&fv); let mut n_cats = Vec::new();
                            for i in 0..fa.length() { let v = fa.get(i); let ci = js_sys::Reflect::get(&v, &JsValue::from_str("id")).unwrap().as_string().unwrap(); let cn = js_sys::Reflect::get(&v, &JsValue::from_str("name")).unwrap().as_string().unwrap(); n_cats.push(JSCategory { id: ci, name: cn }); }
                            if let Ok(v) = serde_wasm_bindgen::to_value(&n_cats) { let _ = save_categories(v).await; }
                            cs.set(n_cats);
                        }
                    }
                    let mut us = (*ss_inner).clone(); let mut q = Vec::new();
                    let mut deleted = false;
                    for s in us.iter_mut() {
                        if s.drive_id.is_some() { if let Ok(_) = get_file_metadata(&s.drive_id.clone().unwrap()).await { } else { q.push(s.id.clone()); } }
                    }
                    for qid in q.clone() { if let Some(pos) = us.iter().position(|x| x.id == qid) { let _ = crate::db_interop::delete_sheet(&qid).await; us.remove(pos); deleted = true; } }
                    if deleted {
                        let ser = serde_wasm_bindgen::Serializer::json_compatible();
                        for s in us.iter() { let js = s.to_js(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } }
                    }
                    if us.is_empty() {
                        let nid = js_sys::Date::now().to_string();
                        let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false };
                        us.push(ns.clone()); aid_inner.set(Some(nid.clone())); load_editor_content(""); focus_editor();
                        let js = ns.to_js();
                        let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                    } else if deleted { let nid = us.last().unwrap().id.clone(); aid_inner.set(Some(nid)); }
                    ss_inner.set(us.clone());
                    if q.is_empty() {
                        ifod.set(true);
                        let ild = ild_final.clone(); let aid = aid_inner.clone(); let u_final = us.clone();
                        Timeout::new(350, move || {
                            ild.set(false);
                            if let Some(id) = (*aid).clone() {
                                if let Some(s) = u_final.iter().find(|x| x.id == id) {
                                    load_editor_content(&s.content); 
                                    let mode = if s.category == "__LOCAL__" { "local" } else if s.category.is_empty() { if s.title.starts_with("Untitled.txt") { "unsaved" } else { "local" } } else if s.drive_id.is_none() && s.guid.is_none() { "unsaved" } else { "none" }; 
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
        let lmk_h = loading_message_key.clone();
        let ris_h = saving_id_ref.clone(); let is_saving_h = saving_sheet_id.clone();
        let ncq_h = name_conflict_queue.clone();
        let osh_cb = os_handle.clone();
        let cq_save = conflict_queue.clone();
        let ifo_save = is_fading_out.clone();
        Callback::from(move |is_manual: bool| {
            let aid_opt = (*r_aid.borrow()).clone();
            let id = if let Some(ref id) = aid_opt {
                if let Some(ref saving_id) = *ris_h.borrow() { if saving_id == id { return; } }
                id.clone()
            } else { return; };

            // エディタの内容を「今この瞬間」にキャプチャする（超重要）
            let captured_content = if let Some(s) = get_editor_content().as_string() { s } else { return; };

            let r_aid = r_aid.clone(); let r_s = r_s.clone(); let s_state = s_state.clone();
            let r_ncid = r_ncid.clone(); let nc_h = nc_h.clone();
            let ild_h = ild_h.clone();
            let lmk_h = lmk_h.clone();
            let lock_h = lock_h.clone();
            let lock_fade_h = lock_fade_h.clone();
            let ris_h = ris_h.clone(); let is_saving_h = is_saving_h.clone();
            let ncq_h = ncq_h.clone();
            let osh_async = osh_cb.clone();
            let cq_async = cq_save.clone();
            let ifo_async = ifo_save.clone();

            Timeout::new(0, move || {
                let ncid_val = (*r_ncid.borrow()).clone();
                let cur_c = captured_content;
                let mut cur_s = (*r_s.borrow()).clone();
                let sheet_idx = cur_s.iter().position(|s| s.id == id);
                
                if let Some(idx) = sheet_idx {
                    let mut local_save_triggered = false;
                    let mut drive_save_prepared = false;
                    let mut target_folder_id_val = String::new();
                    let mut s_clone_opt: Option<Sheet> = None;

                    {
                        let sheet = &mut cur_s[idx];

                        // 自動保存時の空データ保護:
                        // エディタが空で、かつ元のデータが空でない場合、異常事態とみなして復元する。
                        if !is_manual && cur_c.is_empty() && !sheet.content.is_empty() {
                            gloo::console::warn!("[Leaf-SYSTEM] Auto-save blocked: captured content is empty. Restoring from state.");
                            set_editor_content(&sheet.content);
                            return;
                        }

                        if !is_manual && !sheet.is_modified && sheet.content == cur_c { return; }
                        
                        sheet.content = cur_c.clone(); 
                        sheet.is_modified = false;
                        sheet.temp_content = Some(cur_c.clone());
                        sheet.temp_timestamp = Some(js_sys::Date::now() as u64);
                        
                        if sheet.drive_id.is_none() && sheet.guid.is_none() && !sheet.category.is_empty() && sheet.category != "__LOCAL__" {
                            let new_guid = generate_uuid();
                            let original_ext = sheet.title.split('.').last().unwrap_or("txt").to_lowercase();
                            // フッターの拡張子リストに基づいて判定
                            let is_supported = SUPPORTED_EXTENSIONS.iter().any(|(ext, _)| *ext == original_ext);
                            let final_ext = if is_supported { original_ext } else { "txt".to_string() };
                            
                            sheet.guid = Some(new_guid.clone());
                            sheet.title = format!("{}.{}", new_guid, final_ext);
                        }
                        
                        if sheet.category == "__LOCAL__" {
                            // ... (local save logic)
                            let content_to_save = cur_c.clone();
                            let is_saving_inner = is_saving_h.clone();
                            let ild_inner = ild_h.clone();
                            let lock_inner = lock_h.clone();
                            let lock_fade_inner = lock_fade_h.clone();
                            let sheet_id = id.clone();
                            let rs_cb_inner = r_s.clone();
                            let s_state_inner = s_state.clone();
                            let n_bom = sheet.needs_bom;

                            s_state.set(cur_s.clone());
                            if is_manual { is_saving_h.set(Some(id.clone())); } // マニュアル時のみIDセット
                            spawn_local(async move {
                                let result = save_local_file(&content_to_save, n_bom).await;
                                if let Some(fname) = result.as_string() {
                                    let mut us = (*rs_cb_inner.borrow()).clone();
                                    if let Some(s) = us.iter_mut().find(|x| x.id == sheet_id) {
                                        s.category = "__LOCAL__".to_string();
                                        s.title = fname.clone();
                                        s.is_modified = false;
                                        s.content = content_to_save;
                                        s.total_size = s.content.len() as u64;
                                        s.loaded_bytes = s.content.len() as u64;
                                        let js = s.to_js();
                                        let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                        if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                        crate::js_interop::set_editor_mode(&fname);
                                    }
                                    *rs_cb_inner.borrow_mut() = us.clone();
                                    s_state_inner.set(us);
                                    is_saving_inner.set(None);
                                    ild_inner.set(false);
                                    if *lock_inner {
                                        lock_fade_inner.set(true);
                                        let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                        let _il = ild_inner.clone();
                                        Timeout::new(300, move || { lf.set(false); l.set(false); _il.set(false); }).forget();
                                    }
                                } else {
                                    is_saving_inner.set(None);
                                    ild_inner.set(false);
                                    if *lock_inner {
                                        lock_fade_inner.set(true);
                                        let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                        let _il = ild_inner.clone();
                                        Timeout::new(300, move || { lf.set(false); l.set(false); _il.set(false); }).forget();
                                    }
                                }
                            });
                            local_save_triggered = true;
                        } else if sheet.category != "__LOCAL__" {
                            if let Some(others_id) = ncid_val {
                                target_folder_id_val = if sheet.category.is_empty() || sheet.category == "OTHERS" { others_id } else { sheet.category.clone() };
                                s_clone_opt = Some(sheet.clone());
                                drive_save_prepared = true;
                            } else {
                                ild_h.set(false); lock_h.set(false); return;
                            }
                        }
                    }

                    if local_save_triggered { return; }
                    if !drive_save_prepared { return; }

                    let s_clone = s_clone_opt.unwrap();
                    *r_s.borrow_mut() = cur_s.clone();
                    s_state.set(cur_s);

                    let s_inner = s_state.clone(); let nc_inner = nc_h.clone();
                    let rs_async = r_s.clone();
                    let ris_inner = ris_h.clone(); let is_saving_inner = is_saving_h.clone(); let ncq_inner = ncq_h.clone();
                    let ild_inner = ild_h.clone();
                    let lock_inner = lock_h.clone();
                    let lock_fade_inner = lock_fade_h.clone();
                    
                    // 手動・自動に関わらず「保存中」状態にする（右下インジケータ用）
                    *ris_inner.borrow_mut() = Some(id.clone()); 
                    is_saving_h.set(Some(id.clone()));

                    if is_manual {
                        // 手動保存時のみ画面中央のインジケーターを表示
                        let lmk = lmk_h.clone();
                        let ild = ild_h.clone();
                        let ifo = lock_fade_h.clone(); // フェード制御用
                        lmk.set("saving");
                        ild.set(true);
                        ifo.set(false);
                    }
                    
                    spawn_local(async move {
                         let target_folder_id = target_folder_id_val;
                         let sheet = s_clone;
                         let _structure = match ensure_directory_structure().await { Ok(res) => res, Err(_) => { 
                             *ris_inner.borrow_mut() = None; is_saving_inner.set(None); 
                             if *lock_inner {
                                 lock_fade_inner.set(true);
                                 let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                 let _il = ild_inner.clone();
                                 Timeout::new(300, move || { lf.set(false); l.set(false); _il.set(false); }).forget();
                             } else { ild_inner.set(false); }
                             return; 
                         } };
                         
                         if !sheet.category.is_empty() && sheet.category != "OTHERS" {
                             if let Err(_) = get_file_metadata(&sheet.category).await {
                                 *ris_inner.borrow_mut() = None; is_saving_inner.set(None); 
                                 let mut u_s = (*rs_async.borrow()).clone();
                                 if let Some(si) = u_s.iter_mut().find(|x| x.id == sheet.id) { 
                                     si.is_modified = true; 
                                     let js = si.to_js();
                                     let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                 }
                                 *rs_async.borrow_mut() = u_s.clone(); s_inner.set(u_s);
                                 if *lock_inner {
                                     lock_fade_inner.set(true);
                                     let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                                                             let _il = ild_inner.clone();
                                                                             Timeout::new(300, move || { lf.set(false); l.set(false); _il.set(false); }).forget();                                 } else { ild_inner.set(false); }
                                 return;
                             }
                         }
                    
                         let fname = sheet.title.clone();
                    
                         if sheet.drive_id.is_none() && sheet.guid.is_none() {
                             if let Ok(existing) = find_file_by_name(&fname, &target_folder_id).await {
                                 if !existing.is_null() && !existing.is_undefined() {
                                     if let Ok(eid) = js_sys::Reflect::get(&existing, &JsValue::from_str("id")) {
                                         if let Some(eid_str) = eid.as_string() {
                                             let mut q = (*ncq_inner).clone();
                                             q.push(NameConflictData { sheet_id: sheet.id.clone(), filename: fname.clone(), folder_id: target_folder_id.clone(), existing_file_id: eid_str });
                                             ncq_inner.set(q); *ris_inner.borrow_mut() = None; is_saving_inner.set(None); ild_inner.set(false); return;
                                         }
                                     }
                                 }
                             }
                         }


                         // 保存前コンフリクトチェック: Driveのファイルが更新されていないか確認
                         if let Some(ref did) = sheet.drive_id {
                             if let Some(sync_ts) = sheet.last_sync_timestamp {
                                 if let Ok(metadata) = get_file_metadata(did).await {
                                     if let Ok(tv) = js_sys::Reflect::get(&metadata, &JsValue::from_str("modifiedTime")) {
                                         if let Some(ts) = tv.as_string() {
                                             let drive_time = crate::drive_interop::parse_date(&ts) as u64;
                                             if drive_time > sync_ts + 1000 {
                                                 // Driveの方が新しい → コンフリクトダイアログを表示して保存中断
                                                 gloo::console::warn!(format!("[Leaf-SYSTEM] Pre-save conflict! drive_time({}) > sync_ts({}). Aborting save.", drive_time, sync_ts));
                                                 let mut current_q = (*cq_async).clone();
                                                 if !current_q.iter().any(|c| c.sheet_id == sheet.id) {
                                                     current_q.push(ConflictData {
                                                         sheet_id: sheet.id.clone(), title: fname.clone(), drive_id: did.clone(),
                                                         local_content: sheet.content.clone(), drive_time, time_str: ts, is_missing_on_drive: false,
                                                     });
                                                     cq_async.set(current_q);
                                                 }
                                                 *ris_inner.borrow_mut() = None; is_saving_inner.set(None);
                                                 ifo_async.set(true);
                                                 let ild = ild_inner.clone(); let ifo = ifo_async.clone();
                                                 Timeout::new(300, move || { ild.set(false); ifo.set(false); }).forget();
                                                 return;
                                             }
                                         }
                                     }
                                 }
                             }
                         }

                         let final_content = &sheet.content;
                         let res = upload_file(&fname, &JsValue::from_str(final_content), &target_folder_id, sheet.drive_id.as_deref()).await;
                         let mut n_did = sheet.drive_id.clone(); let mut stime = sheet.last_sync_timestamp;
                         match res {
                             Ok(rv) => {
                                 if let Ok(iv) = js_sys::Reflect::get(&rv, &JsValue::from_str("id")) { if let Some(is) = iv.as_string() { n_did = Some(is); } }
                                 if let Ok(tv) = js_sys::Reflect::get(&rv, &JsValue::from_str("modifiedTime")) { if let Some(ts) = tv.as_string() { stime = Some(crate::drive_interop::parse_date(&ts) as u64); } }
                                 nc_inner.set(true); // 成功したのでオンラインに
                             },
                             Err(_) => {
                                 nc_inner.set(false); // 失敗したのでオフライン状態へ
                                 let js = sheet.to_js();
                                 let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                 let mut u_s = (*rs_async.borrow()).clone();
                                 if let Some(si) = u_s.iter_mut().find(|x| x.id == sheet.id) { si.is_modified = true; }
                                 *rs_async.borrow_mut() = u_s.clone(); s_inner.set(u_s);
                                 *ris_inner.borrow_mut() = None; is_saving_inner.set(None); 
                                 if *lock_inner {
                                     lock_fade_inner.set(true);
                                     let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                                                             let _il = ild_inner.clone();
                                                                             Timeout::new(300, move || { lf.set(false); l.set(false); _il.set(false); }).forget();                                 } else { ild_inner.set(false); }
                                 return;
                             },
                         }
                         
                         let mut u_s = (*rs_async.borrow()).clone();
                         let final_size = final_content.len() as u64;
                         if let Some(si) = u_s.iter_mut().find(|x| x.id == sheet.id) { 
                             si.drive_id = n_did; si.total_size = final_size; si.loaded_bytes = final_size;
                             if si.content == sheet.content { si.is_modified = false; }
                             si.temp_content = None; si.temp_timestamp = None; si.last_sync_timestamp = stime; 
                             let js = si.to_js();
                             let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                             if si.title == fname { crate::js_interop::set_editor_mode(&fname); }
                         }
                         let is_active = (*r_aid.borrow()).as_ref() == Some(&sheet.id);
                         *rs_async.borrow_mut() = u_s.clone(); s_inner.set(u_s);
                         if is_active { set_gutter_status("none"); }
                         *ris_inner.borrow_mut() = None; is_saving_inner.set(None); 

                         let latest_content = get_editor_content();
                         if let Some(lc) = latest_content.as_string() {
                             if lc != *final_content {
                                 if let Some(cb) = &*osh_async.borrow() {
                                     let cb = cb.clone();
                                     Timeout::new(1000, move || { cb.emit(false); }).forget();
                                 }
                             }
                         }

                         if *lock_inner {
                             lock_fade_inner.set(true);
                             let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                                                     let _il = ild_inner.clone();
                                                                     Timeout::new(300, move || { lf.set(false); l.set(false); _il.set(false); }).forget();                         } else { ild_inner.set(false); }
                    });
                } else { ild_h.set(false); lock_h.set(false); }
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
                    2 => { if !input_name.trim().is_empty() { s.title = input_name; s.guid = None; } else { s.guid = Some(generate_uuid()); } }
                    _ => {}
                }
                let js = s.to_js();
                let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { spawn_local(async move { let _ = save_sheet(v).await; }); }
            }
            *rs.borrow_mut() = us.clone(); s_state.set(us); ncq.set(q);
            let os_retry = os.clone(); Timeout::new(100, move || { os_retry.emit(true); }).forget();
        })
    };

    let ncid_for_new_cb = no_category_folder_id.clone();
    let on_new_sheet_cb = {
        let s_state = sheets.clone(); let aid_state = active_sheet_id.clone();
        let sp_state = is_suppressing_changes.clone(); let r_s = sheets_ref.clone();
        let r_aid = active_id_ref.clone();
        let os = on_save_cb.clone();
        let ncid_h = ncid_for_new_cb;
        let ris_h = saving_id_ref.clone();
        let is_creating = is_creating_new.clone();
        let is_ld = is_loading.clone();
        let lmk = loading_message_key.clone();
        let ifo = is_fading_out.clone();
        Callback::from(move |_| {
            if *is_creating { return; } // 作成中の連打を防止

            let s = s_state.clone(); let aid = aid_state.clone(); let sp = sp_state.clone();
            let rs = r_s.clone();
            let os_cb = os.clone();
            let is_creating_handle = is_creating.clone();
            let is_loading_handle = is_ld.clone();
            let lmk_handle = lmk.clone();
            let ifo_handle = ifo.clone();
            
            let aid_val = (*r_aid.borrow()).clone();
            let mut needs_save = false;
            if let Some(ref id) = aid_val {
                if let Some(ref saving_id) = *ris_h.borrow() {
                    if saving_id == id {
                        gloo::console::log!("[Leaf-SYSTEM] Waiting for active sync before new sheet...");
                        let os_retry = os_cb.clone();
                        Timeout::new(200, move || { os_retry.emit(false); }).forget();
                        return;
                    }
                }

                let cur_s = (*rs.borrow()).clone();
                if let Some(sheet) = cur_s.iter().find(|x| x.id == *id) {
                    let cur_c_val = get_editor_content();
                    if let Some(cur_c) = cur_c_val.as_string() {
                        if !cur_c.trim().is_empty() && (sheet.is_modified || sheet.content != cur_c) { needs_save = true; }
                    }
                }
            }

            is_creating_handle.set(true);
            if needs_save {
                gloo::console::log!("[Leaf-SYSTEM] Triggering final save before new sheet...");
                lmk_handle.set("saving");
                is_loading_handle.set(true);
                ifo_handle.set(false);
                os_cb.emit(false);
            }

            sp.set(true); 
            // 保存が必要な場合は確実にキャプチャされるまで少し待機
            let delay = if needs_save { 150 } else { 0 };
            let ncid_for_new = ncid_h.clone();
            let is_ld_inner = is_loading_handle.clone();
            let ifo_inner = ifo_handle.clone();
            let is_creating_inner = is_creating_handle.clone();
            Timeout::new(delay, move || {
                clear_local_handle();
                let nid = js_sys::Date::now().to_string();
                let cat_id = (*ncid_for_new).clone().unwrap_or_else(|| "".to_string());
                let ns = Sheet { id: nid.clone(), guid: None, category: cat_id, title: "Untitled.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false };
                load_editor_content(""); set_gutter_status("unsaved");

                let mut current_sheets = (*rs.borrow()).clone();
                current_sheets.push(ns.clone());
                *rs.borrow_mut() = current_sheets.clone();
                s.set(current_sheets);
                aid.set(Some(nid.clone()));
                
                focus_editor(); 
                let spr = sp.clone(); 
                Timeout::new(500, move || { 
                    spr.set(false); 
                    if *is_ld_inner {
                        ifo_inner.set(true);
                        let is_ld_final = is_ld_inner.clone();
                        let ifo_final = ifo_inner.clone();
                        Timeout::new(300, move || {
                            is_ld_final.set(false);
                            ifo_final.set(false);
                        }).forget();
                    }
                    is_creating_inner.set(false);
                }).forget();

                spawn_local(async move {
                    let js = ns.to_js();
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
                let js = s.to_js();
                let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { spawn_local(async move { let _ = save_sheet(v).await; }); }
            }
            *rs.borrow_mut() = us.clone(); s_state.set(us); fq.set(q); os.emit(true); 
        })
    };

    let on_delete_category_cb = { let pending = pending_delete_category.clone(); Callback::from(move |id: String| { pending.set(Some(id)); }) };

    let on_delete_category_cfm = {
        let pending = pending_delete_category.clone();
        let on_refresh = on_refresh_cats_cb.clone();
        let s_state = sheets.clone(); let rs = sheets_ref.clone(); let ncid_state = no_category_folder_id.clone();
        let cats_state = categories.clone();
        let file_refresh_trigger_h = file_refresh_trigger.clone();
        let is_processing = is_processing_dialog.clone();
        let is_file_list_loading_h = is_file_list_loading.clone();
        Callback::from(move |_: usize| {
            if let Some(tcid) = (*pending).clone() {
                let pending_inner = pending.clone(); let on_refresh_inner = on_refresh.clone();
                let ss = s_state.clone(); let rs_inner = rs.clone(); let ncid_s = ncid_state.clone();
                let cs_inner = cats_state.clone(); let target_cid = tcid.clone();
                let file_refresh_trigger = file_refresh_trigger_h.clone();
                let is_processing_h = is_processing.clone();
                let is_file_list_loading = is_file_list_loading_h.clone();
                
                pending_inner.set(None); is_processing_h.set(true); is_file_list_loading.set(true);

                spawn_local(async move {
                    let structure = match ensure_directory_structure().await { Ok(res) => res, Err(_) => { is_processing_h.set(false); is_file_list_loading.set(false); return; } };
                    let ncid = js_sys::Reflect::get(&structure, &JsValue::from_str("othersId")).unwrap().as_string().unwrap();
                    ncid_s.set(Some(ncid.clone()));

                    let mut us = (*rs_inner.borrow()).clone();
                    let mut changed = false;
                    for s in us.iter_mut() { if s.category == target_cid { s.category = ncid.clone(); changed = true; } }
                    if changed {
                        *rs_inner.borrow_mut() = us.clone(); ss.set(us.clone());
                        let ser = serde_wasm_bindgen::Serializer::json_compatible();
                        for s in us.iter() { let js = s.to_js(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } }
                    }

                    if let Ok(fr) = list_files(&target_cid, None).await {
                        if let Ok(fv) = js_sys::Reflect::get(&fr, &JsValue::from_str("files")) {
                            let fa = js_sys::Array::from(&fv);
                            for i in 0..fa.length() {
                                let fm = fa.get(i); let fid = js_sys::Reflect::get(&fm, &JsValue::from_str("id")).unwrap().as_string().unwrap();
                                let _ = move_file(&fid, &target_cid, &ncid).await;
                            }
                        }
                    }
                    let _ = delete_file(&target_cid).await;

                    let trigger_handle = file_refresh_trigger.clone();
                    let cs_inner_final = cs_inner.clone();
                    let target_cid_final = target_cid.clone();
                    let is_proc_final = is_processing_h.clone();
                    Timeout::new(10, move || {
                        let mut current_cats = (*cs_inner_final).clone();
                        current_cats.retain(|c| c.id != target_cid_final);
                        cs_inner_final.set(current_cats);
                        on_refresh_inner.emit(());
                        trigger_handle.set(*trigger_handle + 1);
                        is_proc_final.set(false);
                        is_file_list_loading.set(false);
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
                if let Ok(_) = crate::drive_interop::rename_folder(&id, &new_name).await { on_refresh_inner.emit(()); }
                ifo_inner.set(true); 
                let ifo_final = ifo_inner.clone();
                Timeout::new(300, move || { il_inner.set(false); ifo_final.set(false); }).forget();
            });
        })
    };

    let on_conf_cfm = {
        let cq = conflict_queue.clone(); let ss = sheets.clone(); let il = is_loading.clone();
        let ifo = is_fading_out.clone(); let ncid = no_category_folder_id.clone();
        let aid = active_sheet_id.clone();
        let lmk = loading_message_key.clone();
        let osa = os_handle.clone();
        let timer_h = auto_save_timer.clone();
        Callback::from(move |sel: usize| {
            let mut q = (*cq).clone(); if q.is_empty() { return; } let conf = q.remove(0);
            let ss_inner = ss.clone(); let qs = cq.clone(); let ifod = ifo.clone();
            let fid_opt = ncid.as_ref().map(|s| s.clone()); let aid_v = (*aid).clone();
            let aid_inner = aid.clone(); let ild_final = il.clone();
            let lmk_inner = lmk.clone();
            let osa_inner = osa.clone();
            let timer_inner = timer_h.clone();

            lmk_inner.set("synchronizing");
            ild_final.set(true); 
            ifod.set(false);

            spawn_local(async move {
                let mut us = (*ss_inner).clone(); let mut deleted = false;
                if let Some(pos) = us.iter().position(|x| x.id == conf.sheet_id) {
                    let s = &mut us[pos];
                    match sel {
                        0 => { // Googleドライブのデータを読み込む
                            if let Ok(dv) = download_file(&conf.drive_id, None, None).await { 
                                let decoder = js_sys::Reflect::get(&web_sys::window().unwrap(), &JsValue::from_str("TextDecoder")).unwrap();
                                let decoder_instance = js_sys::Reflect::construct(&decoder.into(), &js_sys::Array::of1(&JsValue::from_str("utf-8"))).unwrap();
                                let decode_fn = js_sys::Reflect::get(&decoder_instance, &JsValue::from_str("decode")).unwrap();
                                let t = js_sys::Reflect::apply(&decode_fn.into(), &decoder_instance, &js_sys::Array::of1(&dv)).unwrap().as_string().unwrap_or_default();
                                let t_len = t.len() as u64;
                                s.content = t.clone(); s.temp_content = None; s.temp_timestamp = None; s.last_sync_timestamp = Some(conf.drive_time); s.is_modified = false; s.loaded_bytes = t_len; s.total_size = t_len; if Some(s.id.clone()) == aid_v { load_editor_content(&t); }
                            }
                        },
                        1 => { // 編集中のデータを上書き
                            if let Some(fid) = fid_opt { 
                                let did = if conf.is_missing_on_drive { None } else { Some(conf.drive_id.as_str()) };
                                if let Ok(rv) = upload_file(&format!("{}.txt", s.guid.as_ref().unwrap_or(&generate_uuid())), &JsValue::from_str(&s.content), &fid, did).await {
                                    if let Ok(iv) = js_sys::Reflect::get(&rv, &JsValue::from_str("id")) { if let Some(is) = iv.as_string() { s.drive_id = Some(is); } }
                                    if let Ok(tv) = js_sys::Reflect::get(&rv, &JsValue::from_str("modifiedTime")) { if let Some(ts) = tv.as_string() { s.last_sync_timestamp = Some(crate::drive_interop::parse_date(&ts) as u64); } }
                                    s.temp_content = None; s.temp_timestamp = None; s.is_modified = false; 
                                }
                            } 
                        },
                        2 => { // 別ファイルとして保存
                            if let Some(fid) = fid_opt { 
                                let ng = generate_uuid(); 
                                if let Ok(rv) = upload_file(&format!("{}.txt", ng), &JsValue::from_str(&s.content), &fid, None).await {
                                    if let Ok(iv) = js_sys::Reflect::get(&rv, &JsValue::from_str("id")) { if let Some(is) = iv.as_string() { s.drive_id = Some(is); } }
                                    if let Ok(tv) = js_sys::Reflect::get(&rv, &JsValue::from_str("modifiedTime")) { if let Some(ts) = tv.as_string() { s.last_sync_timestamp = Some(crate::drive_interop::parse_date(&ts) as u64); } }
                                    s.guid = Some(ng); s.temp_content = None; s.temp_timestamp = None; s.is_modified = false; s.tab_color = generate_random_color(); 
                                }
                            } 
                        },
                        3 => { let _ = crate::db_interop::delete_sheet(&s.id).await; us.remove(pos); deleted = true; },
                        _ => {}
                    }
                    if !deleted {
                        let ds = &us[pos];
                        let js = ds.to_js();
                        let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                    }
                }
                if us.is_empty() {
                    let nid = js_sys::Date::now().to_string();
                    let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false };
                    us.push(ns.clone()); aid_inner.set(Some(nid.clone())); load_editor_content(""); focus_editor();
                    let js = ns.to_js();
                    let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                } else if deleted { let nid = us.last().unwrap().id.clone(); aid_inner.set(Some(nid)); }
                ss_inner.set(us.clone()); qs.set(q.clone());
                
                // 処理完了後の後始末
                if q.is_empty() { 
                    ifod.set(true); 
                    let ild = ild_final.clone(); let aid = aid_inner.clone(); let u_final = us.clone(); 
                    let ifo_inner = ifod.clone();
                    let osa = osa_inner.clone();
                    let timer = timer_inner.clone();
                    Timeout::new(350, move || { 
                        ild.set(false); ifo_inner.set(false);
                        if let Some(id) = (*aid).clone() {
                            if let Some(s) = u_final.iter().find(|x| x.id == id) {
                                load_editor_content(&s.content);
                                let mode = if s.category == "__LOCAL__" { "local" } else if s.category.is_empty() { if s.title.starts_with("Untitled.txt") { "unsaved" } else { "local" } } else if s.drive_id.is_none() && s.guid.is_none() { "unsaved" } else { "none" };
                                set_gutter_status(mode);
                            }
                        }
                        focus_editor(); 
                        
                        // 自動保存の監視を再開（1秒後にチェック）
                        if let Some(osa_cb) = (*osa.borrow()).as_ref() {
                            let osa_cb = osa_cb.clone();
                            timer.set(Some(Timeout::new(1000, move || { osa_cb.emit(false); })));
                        }
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
                    if let Some(cur_c) = cur_c_val.as_string() { if !cur_c.trim().is_empty() && (sheet.is_modified || sheet.content != cur_c) { needs_save = true; } }
                }
            }
            if needs_save { os.emit(false); }

            iv.set(false); lmk.set("synchronizing"); il.set(true); ifo.set(false); sp.set(true); 
            let ss_inner = ss.clone(); let aid_inner = aid.clone(); let sp_inner = sp.clone();
            let il_inner = il.clone(); let ifo_inner = ifo.clone(); let rs_inner = rs.clone();
            spawn_local(async move {
                if let Ok(cv) = download_file(&did, None, None).await {
                    let bytes = js_sys::Uint8Array::new(&cv).to_vec();
                    let has_bom = has_utf8_bom(&bytes);
                    let decoder = js_sys::Reflect::get(&web_sys::window().unwrap(), &JsValue::from_str("TextDecoder")).unwrap();
                    let decoder_instance = js_sys::Reflect::construct(&decoder.into(), &js_sys::Array::of1(&JsValue::from_str("utf-8"))).unwrap();
                    let decode_fn = js_sys::Reflect::get(&decoder_instance, &JsValue::from_str("decode")).unwrap();
                    let c = js_sys::Reflect::apply(&decode_fn.into(), &decoder_instance, &js_sys::Array::of1(&cv)).unwrap().as_string().unwrap_or_default();
                    let c_len = c.len() as u64;
                    
                    // DriveのmodifiedTimeを取得して正確なタイムスタンプを使用
                    let sync_ts = if let Ok(meta) = get_file_metadata(&did).await {
                        if let Ok(tv) = js_sys::Reflect::get(&meta, &JsValue::from_str("modifiedTime")) {
                            tv.as_string().map(|ts| crate::drive_interop::parse_date(&ts) as u64)
                        } else { None }
                    } else { None };

                    let mut cs = (*rs_inner.borrow()).clone();
                    let tidx = if cs.len() == 1 && cs[0].drive_id.is_none() && cs[0].content.is_empty() { Some(0) } else { None };
                    // 既に同じdrive_idのシートが開かれている場合はそのタブを更新
                    let existing_idx = cs.iter().position(|s| s.drive_id.as_ref() == Some(&did));
                    let guid = if title.ends_with(".txt") { Some(title.replace(".txt", "")) } else { Some(title.clone()) };
                    let nid = if let Some(idx) = tidx { cs[idx].id.clone() } else if let Some(idx) = existing_idx { cs[idx].id.clone() } else { js_sys::Date::now().to_string() };
                    let ns = Sheet { id: nid.clone(), guid: guid.clone(), category: cat_id.clone(), title: title.clone(), content: c.clone(), is_modified: false, drive_id: Some(did.clone()), temp_content: None, temp_timestamp: None, last_sync_timestamp: sync_ts, tab_color: if let Some(idx) = tidx { cs[idx].tab_color.clone() } else if let Some(idx) = existing_idx { cs[idx].tab_color.clone() } else { generate_random_color() }, total_size: c_len, loaded_bytes: c_len, needs_bom: has_bom, is_preview: false };
                    load_editor_content(&c); set_gutter_status("none");
                    if let Some(idx) = tidx { cs[idx] = ns.clone(); } else if let Some(idx) = existing_idx { cs[idx] = ns.clone(); } else { cs.push(ns.clone()); }
                    *rs_inner.borrow_mut() = cs.clone(); ss_inner.set(cs); aid_inner.set(Some(nid.clone()));
                    focus_editor(); 
                    let js = ns.to_js();
                    let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                    Timeout::new(50, move || {
                        ifo_inner.set(true); let ifo_final = ifo_inner.clone();
                        Timeout::new(300, move || { il_inner.set(false); sp_inner.set(false); ifo_final.set(false); }).forget();
                    }).forget();
                } else { il_inner.set(false); sp_inner.set(false); }
            });
        })
    };

    let on_delete_file_cb = {
        let s_state = sheets.clone(); let rs = sheets_ref.clone();
        Callback::from(move |(drive_id, _filename): (String, String)| {
            let rs_inner = rs.clone(); let ss = s_state.clone(); let d_id = drive_id.clone();
            spawn_local(async move {
                let mut us = (*rs_inner.borrow()).clone();
                if let Some(sheet) = us.iter_mut().find(|s| s.drive_id.as_ref() == Some(&d_id)) {
                    sheet.drive_id = None; 
                    let new_guid = generate_uuid(); 
                    let original_ext = sheet.title.split('.').last().unwrap_or("txt").to_lowercase();
                    let is_supported = SUPPORTED_EXTENSIONS.iter().any(|(ext, _)| *ext == original_ext);
                    let final_ext = if is_supported { original_ext } else { "txt".to_string() };
                    
                    sheet.guid = Some(new_guid.clone()); 
                    sheet.title = format!("{}.{}", new_guid, final_ext); 
                    sheet.is_modified = true; 
                    set_gutter_status("unsaved");
                    let js = sheet.to_js(); let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                }
                *rs_inner.borrow_mut() = us.clone(); ss.set(us); let _ = delete_file(&d_id).await;
            });
        })
    };

    let on_move_file_cb = {
        let s_state = sheets.clone(); let rs = sheets_ref.clone();
        Callback::from(move |(drive_id, new_category_id): (String, String)| {
            let mut us = (*rs.borrow()).clone();
            if let Some(sheet) = us.iter_mut().find(|s| s.drive_id.as_ref() == Some(&drive_id)) {
                sheet.category = new_category_id;
                let js = sheet.to_js();
                let ser = serde_wasm_bindgen::Serializer::json_compatible(); 
                if let Ok(v) = js.serialize(&ser) { spawn_local(async move { let _ = save_sheet(v).await; }); }
            }
            *rs.borrow_mut() = us.clone(); s_state.set(us);
        })
    };

    let lock_for_import = is_import_lock.clone();
    let il_for_import = is_loading.clone();
    let ifo_for_import = is_fading_out.clone();
    let lock_fade_for_import = is_import_fading_out.clone();
    let lmk_for_import = loading_message_key.clone();
    let on_import_cb = {
        let s_state = sheets.clone(); let aid_state = active_sheet_id.clone();
        let sp_state = is_suppressing_changes.clone(); let r_s = sheets_ref.clone();
        let lock_h = lock_for_import; let il_h = il_for_import; let ifo_h = ifo_for_import;
        let lock_fade_h = lock_fade_for_import; let lmk_h = lmk_for_import;
        let os = on_save_cb.clone();
        Callback::from(move |_| {
            let aid_val = (*aid_state).clone();
            let mut needs_save = false;
            if let Some(id) = aid_val {
                let cur_s = (*r_s.borrow()).clone();
                if let Some(sheet) = cur_s.iter().find(|x| x.id == id) {
                    let cur_c_val = get_editor_content();
                    if let Some(cur_c) = cur_c_val.as_string() { if !cur_c.trim().is_empty() && (sheet.is_modified || sheet.content != cur_c) { needs_save = true; } }
                }
            }
            if needs_save { os.emit(false); }

            let s_state_c = s_state.clone(); let aid_state_c = aid_state.clone();
            let sp_state_c = sp_state.clone(); let r_s_c = r_s.clone();
            let lock_cb = lock_h.clone(); let il_cb = il_h.clone(); let ifo_cb = ifo_h.clone();
            let lock_fade_cb = lock_fade_h.clone(); let lmk_cb = lmk_h.clone();
            spawn_local(async move {
                let res = open_local_file().await; if res.is_null() || res.is_undefined() { return; }
                
                let content_val = js_sys::Reflect::get(&res, &JsValue::from_str("content")).ok().and_then(|v| v.as_string());
                let bytes_val = js_sys::Reflect::get(&res, &JsValue::from_str("bytes")).ok();
                let name_val = js_sys::Reflect::get(&res, &JsValue::from_str("name")).ok().and_then(|v| v.as_string());

                if let (Some(name), Some(content), Some(bytes_js)) = (name_val, content_val, bytes_val) {
                    let bytes = js_sys::Uint8Array::new(&bytes_js).to_vec();
                    let has_bom = has_utf8_bom(&bytes);
                    
                    lmk_cb.set("synchronizing"); ifo_cb.set(false); lock_fade_cb.set(false); il_cb.set(true); lock_cb.set(true);
                    let nid = js_sys::Date::now().to_string();
                    let ns = Sheet { id: nid.clone(), guid: None, category: "__LOCAL__".to_string(), title: name.clone(), content: content.clone(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: content.len() as u64, loaded_bytes: content.len() as u64, needs_bom: has_bom, is_preview: false };
                    sp_state_c.set(true);
                    let mut current = (*r_s_c.borrow()).clone();
                    // 未保存の新規シート1枚のみなら置換、それ以外はpush
                    if current.len() == 1 && current[0].drive_id.is_none() && current[0].content.is_empty() {
                        current[0] = ns.clone();
                    } else {
                        current.push(ns.clone());
                    }
                    *r_s_c.borrow_mut() = current.clone(); s_state_c.set(current); aid_state_c.set(Some(nid.clone()));
                    load_editor_content(&content); set_gutter_status("local"); crate::js_interop::set_editor_mode(&name);
                    let js = ns.to_js(); let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                    Timeout::new(100, move || {
                        lock_fade_cb.set(true); let l = lock_cb.clone(); let lf = lock_fade_cb.clone(); let il = il_cb.clone(); let sp = sp_state_c.clone();
                        Timeout::new(300, move || { lf.set(false); l.set(false); il.set(false); sp.set(false); }).forget();
                    }).forget();
                } else { il_cb.set(false); lock_cb.set(false); }
            });
        })
    };

    let on_change_category_cb = {
        let s_state = sheets.clone(); let aid_state = active_sheet_id.clone();
        let ncid = no_category_folder_id.clone(); let il = is_loading.clone();
        let ifo = is_fading_out.clone(); let lmk = loading_message_key.clone();
        let r_s = sheets_ref.clone(); let os = on_save_cb.clone();
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
                    let lmk_inner = lmk.clone(); let r_s_inner = r_s.clone(); let os_inner = os.clone();

                    if (old_cat_id == "__LOCAL__" || old_cat_id.is_empty()) && !new_cat_id.is_empty() && new_cat_id != "__LOCAL__" {
                        set_gutter_status("none"); 
                        let guid = generate_uuid(); 
                        let original_ext = sheet.title.split('.').last().unwrap_or("txt").to_lowercase();
                        // フッターの拡張子リストに基づいて判定
                        let is_supported = SUPPORTED_EXTENSIONS.iter().any(|(ext, _)| *ext == original_ext);
                        let final_ext = if is_supported { original_ext } else { "txt".to_string() };
                        
                        sheet.guid = Some(guid.clone()); 
                        sheet.title = format!("{}.{}", guid, final_ext); 
                        sheet.needs_bom = true;
                        clear_local_handle(); 
                        sheet.category = new_cat_id;
                        let mut us = current_sheets; us[pos] = sheet; *r_s_inner.borrow_mut() = us.clone(); s_state_inner.set(us);
                        Timeout::new(0, move || { os_inner.emit(true); }).forget(); return;
                    }
                    if new_cat_id == "__LOCAL__" { sheet.category = "__LOCAL__".to_string(); let mut us = current_sheets; us[pos] = sheet; *r_s_inner.borrow_mut() = us.clone(); s_state_inner.set(us); Timeout::new(0, move || { os_inner.emit(true); }).forget(); return; }

                    if let Some(fid) = file_id_opt {
                        lmk_inner.set("synchronizing"); il_inner.set(true); ifo_inner.set(false);
                        spawn_local(async move {
                            if let Ok(_) = move_file(&fid, &old_cat_id, &new_cat_id).await {
                                let mut us = (*s_state_inner).clone();
                                if let Some(s) = us.iter_mut().find(|x| x.id == id) { s.category = new_cat_id.clone(); let js = s.to_js(); let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } }
                                *r_s_inner.borrow_mut() = us.clone(); s_state_inner.set(us);
                            }
                            ifo_inner.set(true); let ifo_final = ifo_inner.clone();
                            Timeout::new(300, move || { il_inner.set(false); ifo_final.set(false); }).forget();
                        });
                    } else {
                        let mut us = current_sheets; us[pos].category = new_cat_id; let s = &us[pos]; let js = s.to_js();
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
                    let sheet = current_sheets[pos].clone(); if sheet.drive_id.is_none() { return; } 
                    let old_name = sheet.title.clone(); let name_parts: Vec<&str> = old_name.split('.').collect();
                    let base_name = if name_parts.len() > 1 { name_parts[..name_parts.len()-1].join(".") } else { old_name.clone() };
                    let new_name = format!("{}.{}", base_name, new_ext);
                    if old_name == new_name { return; }
                    let s_state_inner = s_state.clone(); let il_inner = il.clone(); let ifo_inner = ifo.clone(); let lmk_inner = lmk.clone(); let r_s_inner = r_s.clone(); let drive_id = sheet.drive_id.clone().unwrap();
                    lmk_inner.set("synchronizing"); il_inner.set(true); ifo_inner.set(false);
                    spawn_local(async move {
                        if let Ok(_) = crate::drive_interop::rename_folder(&drive_id, &new_name).await {
                            let mut us = (*s_state_inner).clone();
                            if let Some(s) = us.iter_mut().find(|x| x.id == id) { s.title = new_name.clone(); let js = s.to_js(); let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } crate::js_interop::set_editor_mode(&new_name); }
                            *r_s_inner.borrow_mut() = us.clone(); s_state_inner.set(us);
                        }
                        ifo_inner.set(true); let ifo_final = ifo_inner.clone();
                        Timeout::new(300, move || { il_inner.set(false); ifo_final.set(false); }).forget();
                    });
                }
            }
        })
    };

    let on_open_dialog = { let iv = is_file_open_dialog_visible.clone(); let sp = is_suppressing_changes.clone(); Callback::from(move |_| { sp.set(true); iv.set(true); }) };
    let on_help_cb = { let ih = is_help_visible.clone(); Callback::from(move |_| { ih.set(true); }) };
    let close_preview = {
        let fo = is_preview_fading_out.clone();
        let iv = is_preview_visible.clone();
        Callback::from(move |_: ()| {
            fo.set(true);
            let iv = iv.clone();
            let fo = fo.clone();
            gloo::timers::callback::Timeout::new(100, move || {
                iv.set(false);
                fo.set(false);
                crate::js_interop::focus_editor();
            }).forget();
        })
    };
    let on_preview_cb = {
        let ip = is_preview_visible.clone();
        let rs = sheets_ref.clone();
        let s_state = sheets.clone();
        let aid_ref = active_id_ref.clone();
        Callback::from(move |_| {
            let new_val = !*ip;
            ip.set(new_val);
            if let Some(id) = (*aid_ref.borrow()).clone() {
                let mut us = (*rs.borrow()).clone();
                if let Some(sheet) = us.iter_mut().find(|s| s.id == id) {
                    sheet.is_preview = new_val;
                    let js = sheet.to_js();
                    let ser = serde_wasm_bindgen::Serializer::json_compatible();
                    if let Ok(v) = js.serialize(&ser) { spawn_local(async move { let _ = save_sheet(v).await; }); }
                }
                *rs.borrow_mut() = us.clone();
                s_state.set(us);
            }
            if !new_val { focus_editor(); }
        })
    };

    {
        let is_auth = is_authenticated.clone();
        let is_ld = is_loading.clone();
        let is_fl_ld = is_file_list_loading.clone();
        let is_fo = is_fading_out.clone();
        let nc = network_connected.clone();
        let aid_ref_effect = active_id_ref.clone();
        let s_ref_effect = sheets_ref.clone();
        let s_handle_effect = sheets.clone();
        let lmk_effect = loading_message_key.clone();
        let on_save_for_net = on_save_cb.clone();

        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let is_auth_c = is_auth.clone();
            let is_ld_c = is_ld.clone();
            let is_fl_ld_c = is_fl_ld.clone();
            let is_fo_c = is_fo.clone();
            
            let listener_expired = EventListener::new(&window, "leaf-auth-expired", move |_| { 
                gloo::console::warn!("[Leaf-SYSTEM] Auth expired event received. Logging out..."); 
                is_auth_c.set(false); 
                is_ld_c.set(false);
                is_fl_ld_c.set(false);
                is_fo_c.set(false);
            });
            let is_auth_r = is_auth.clone();
            let listener_refreshed = EventListener::new(&window, "leaf-token-refreshed", move |_| { gloo::console::log!("[Leaf-SYSTEM] Token refreshed event received."); is_auth_r.set(true); });
            
            let nc_online = nc.clone();
            let listener_online = {
                let nc = nc_online.clone();
                let aid_ref = aid_ref_effect.clone();
                let s_ref = s_ref_effect.clone();
                let s_st = s_handle_effect.clone();
                let ild = is_ld.clone();
                let ifo = is_fo.clone();
                let lmk = lmk_effect.clone();
                let os_cb = on_save_for_net.clone();
                EventListener::new(&window, "online", move |_| {
                    gloo::console::log!("[Leaf-SYSTEM] Network online. Waiting 500ms for stable connection...");
                    nc.set(true);
                    let ar = aid_ref.clone(); let sr = s_ref.clone(); let ss = s_st.clone();
                    let il = ild.clone(); let i = ifo.clone(); let l = lmk.clone(); let o = os_cb.clone();
                    Timeout::new(500, move || {
                        gloo::console::log!("[Leaf-SYSTEM] connection stable. Checking for conflicts...");
                        trigger_conflict_check(ar, sr, ss, il, i, l, None, o);
                    }).forget();
                })
            };

            let nc_offline = nc.clone();
            let listener_offline = EventListener::new(&window, "offline", move |_| {
                gloo::console::warn!("[Leaf-SYSTEM] Network offline.");
                nc_offline.set(false);
            });

            let listener_visibility = {
                let aid_ref = aid_ref_effect.clone();
                let s_ref = s_ref_effect.clone();
                let s_st = s_handle_effect.clone();
                let ild = is_ld.clone();
                let ifo = is_fo.clone();
                let lmk = lmk_effect.clone();
                let doc = web_sys::window().unwrap().document().unwrap();
                let os_cb = on_save_for_net.clone();
                EventListener::new(&doc, "visibilitychange", move |_| {
                    let doc = web_sys::window().unwrap().document().unwrap();
                    if !doc.hidden() {
                        gloo::console::log!("[Leaf-SYSTEM] App visible. Checking for conflicts...");
                        trigger_conflict_check(aid_ref.clone(), s_ref.clone(), s_st.clone(), ild.clone(), ifo.clone(), lmk.clone(), None, os_cb.clone());
                    }
                })
            };

            move || { drop(listener_expired); drop(listener_refreshed); drop(listener_online); drop(listener_offline); drop(listener_visibility); }
        });
    }

    {
        let s_handle = sheets.clone(); let aid_handle = active_sheet_id.clone(); let cats_handle = categories.clone();
        let rs = sheets_ref.clone(); let db_loaded_init = db_ready_state.clone();
        let is_auth_init = is_authenticated.clone(); let is_ld_init = is_loading.clone();
        let is_fo_init = is_fading_out.clone(); let is_in_init = is_initial_load.clone();
        let is_online_init = *network_connected;

        use_effect_with((), move |_| {
            spawn_local(async move {
                let db_name = account_db_name();
                let has_account = db_name != "LeafDB"; // メールが取得できた場合のみDBを開く

                if has_account {
                    if let Err(_) = crate::db_interop::init_db(&db_name).await { gloo::console::error!("DB init failed"); }
                    if let Ok(c_val) = crate::db_interop::load_categories().await { if let Ok(loaded_cats) = serde_wasm_bindgen::from_value::<Vec<JSCategory>>(c_val) { cats_handle.set(loaded_cats); } }
                    let mut initial = true;
                    if let Ok(val) = crate::db_interop::load_sheets().await {
                        if let Ok(loaded) = serde_wasm_bindgen::from_value::<Vec<JSSheet>>(val) {
                            if !loaded.is_empty() {
                                let mapped: Vec<Sheet> = loaded.into_iter().map(|s| Sheet { id: s.id, guid: s.guid, category: s.category, title: s.title, content: s.temp_content.clone().unwrap_or(s.content), is_modified: s.temp_timestamp.is_some(), drive_id: s.drive_id, temp_content: s.temp_content, temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, tab_color: if s.tab_color.is_empty() { generate_random_color() } else { s.tab_color }, total_size: s.total_size, loaded_bytes: s.loaded_bytes, needs_bom: s.needs_bom, is_preview: s.is_preview }).collect();
                                // 保存されたアクティブタブIDを復元、なければ最後のシート
                                let saved_active = get_account_storage(ACTIVE_TAB_KEY);
                                let active_id = saved_active.and_then(|id| mapped.iter().find(|s| s.id == id).map(|s| s.id.clone())).or_else(|| mapped.last().map(|s| s.id.clone()));
                                *rs.borrow_mut() = mapped.clone(); s_handle.set(mapped); aid_handle.set(active_id); initial = false;
                            }
                        }
                    }
                    if initial {
                        let nid = js_sys::Date::now().to_string();
                        let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false };
                        *rs.borrow_mut() = vec![ns.clone()]; s_handle.set(vec![ns]); aid_handle.set(Some(nid));
                    }
                }

                // オフラインの場合は、認証を待たずに起動
                if !is_online_init {
                    gloo::console::log!("[Leaf-SYSTEM] Offline startup. revealing editor UI.");
                    is_auth_init.set(true);
                    is_fo_init.set(true);
                    let ild = is_ld_init.clone(); let isi = is_in_init.clone(); let ifo = is_fo_init.clone();
                    Timeout::new(300, move || {
                        ild.set(false);
                        isi.set(false);
                        ifo.set(false);
                    }).forget();
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
        let nc_h = network_connected.clone();
        let lmk_h = loading_message_key.clone();
        let aid_ref_h = active_id_ref.clone();
        let aid_state_h = active_sheet_id.clone();
        let on_save_for_auth = on_save_cb.clone();
        let is_online = *network_connected;
        let is_auth_flag_h = is_auth_flag.clone();
        let is_ad_free_c = is_ad_free.clone();
        let vim_mode_auth = vim_mode.clone();
        let pfs_auth = preview_font_size.clone();

        use_effect_with((is_online, ), move |_| {
            let cleanup = || ();
            if !is_online { return cleanup; }

            let ncid_cb = ncid.clone(); let ldid_cb = ldid.clone();
            let cats_cb = cats_init.clone(); let s_state_cb = s_state.clone(); let rs_cb = rs.clone();
            let ild_cb = ild_h.clone(); let ifo_cb = ifo_h.clone(); let is_init_cb = is_init_h.clone();
            let nc_cb = nc_h.clone();
            let lmk_cb = lmk_h.clone();
            let aid_ref_cb = aid_ref_h.clone();
            let aid_state_cb = aid_state_h.clone();
            let os_cb_inner = on_save_for_auth.clone();
            let is_ad_free_cb = is_ad_free_c.clone();

            // タイムアウトによる救済ロジック
            // Auth初期化から数秒経っても応答がない場合はローディングを外す
            let ild_timeout = ild_cb.clone();
            let ifo_timeout = ifo_cb.clone();
            let isi_timeout = is_init_cb.clone();
            let auth_flag_timeout = is_auth_flag_h.clone();
            Timeout::new(5000, move || {
                if !*auth_flag_timeout.borrow() {
                    gloo::console::warn!("[Leaf-SYSTEM] Auth initialization timed out or user not signed in. revealing login screen.");
                    ifo_timeout.set(true);
                    let ild = ild_timeout.clone(); let ifo = ifo_timeout.clone(); let isi = isi_timeout.clone();
                    Timeout::new(300, move || { ild.set(false); ifo.set(false); isi.set(false); }).forget();
                }
            }).forget();

            let is_auth_cb_final = is_auth.clone();
            let auth_flag_cb = is_auth_flag_h.clone();
            let callback = Closure::wrap(Box::new(move |_token: String| {
                let is_auth_inner = is_auth_cb_final.clone();
                let os_cb_final = os_cb_inner.clone();
                *auth_flag_cb.borrow_mut() = true; // RefCell: タイムアウトからも即座に参照可能
                if !*is_auth_inner {
                    is_auth_inner.set(true);
                    let ncid_i = ncid_cb.clone(); let ldid_i = ldid_cb.clone(); let cats_i = cats_cb.clone();
                    let s_inner = s_state_cb.clone(); let rs_inner = rs_cb.clone();
                    let ild_inner = ild_cb.clone(); let ifo_inner = ifo_cb.clone(); let is_init_inner = is_init_cb.clone();
                    let is_auth_err = is_auth_inner.clone();
                    let nc_inner = nc_cb.clone();
                    let lmk_inner = lmk_cb.clone();
                    let aid_ref_inner = aid_ref_cb.clone();
                    let aid_state_inner = aid_state_cb.clone();
                    let is_ad_free_inner = is_ad_free_cb.clone();

                    // メールアドレス取得 → アカウント別DB初期化 → 設定再読み込み → Drive初期化
                    let vim_auth = vim_mode_auth.clone();
                    let pfs_a = pfs_auth.clone();
                    let s_reload = s_inner.clone();
                    let rs_reload = rs_inner.clone();
                    let cats_reload = cats_i.clone();
                    spawn_local({
                        let ad_free = is_ad_free_inner.clone();
                        let s_handle = s_reload.clone();
                        let rs_ref = rs_reload.clone();
                        let cats_h = cats_reload.clone();
                        let aid_h = aid_state_inner.clone();
                        async move {
                            // 1. メールアドレスを取得
                            let _ = crate::auth_interop::fetch_user_email().await;
                            let email_val = crate::auth_interop::get_user_email();
                            if let Some(email) = email_val.as_string() {
                                const AD_FREE_EMAILS: &[&str] = &["trek.kbd@gmail.com"];
                                if AD_FREE_EMAILS.iter().any(|e| *e == email) {
                                    ad_free.set(true);
                                }
                            }

                            // 2. アカウント別DBに切り替え
                            crate::db_interop::close_db();
                            let db_name = account_db_name();
                            if let Err(_) = crate::db_interop::init_db(&db_name).await {
                                gloo::console::error!("Account DB init failed");
                            }

                            // 3. アカウント別DBからデータ再読み込み
                            if let Ok(c_val) = crate::db_interop::load_categories().await {
                                if let Ok(loaded_cats) = serde_wasm_bindgen::from_value::<Vec<JSCategory>>(c_val) {
                                    cats_h.set(loaded_cats);
                                }
                            }
                            let mut has_sheets = false;
                            if let Ok(val) = crate::db_interop::load_sheets().await {
                                if let Ok(loaded) = serde_wasm_bindgen::from_value::<Vec<JSSheet>>(val) {
                                    if !loaded.is_empty() {
                                        let mapped: Vec<Sheet> = loaded.into_iter().map(|s| Sheet {
                                            id: s.id, guid: s.guid, category: s.category, title: s.title,
                                            content: s.temp_content.clone().unwrap_or(s.content),
                                            is_modified: s.temp_timestamp.is_some(), drive_id: s.drive_id,
                                            temp_content: s.temp_content, temp_timestamp: s.temp_timestamp,
                                            last_sync_timestamp: s.last_sync_timestamp,
                                            tab_color: if s.tab_color.is_empty() { generate_random_color() } else { s.tab_color },
                                            total_size: s.total_size, loaded_bytes: s.loaded_bytes, needs_bom: s.needs_bom, is_preview: s.is_preview
                                        }).collect();
                                        let saved_active = get_account_storage(ACTIVE_TAB_KEY);
                                        let active_id = saved_active.and_then(|id| mapped.iter().find(|s| s.id == id).map(|s| s.id.clone())).or_else(|| mapped.last().map(|s| s.id.clone()));
                                        *rs_ref.borrow_mut() = mapped.clone();
                                        s_handle.set(mapped);
                                        aid_h.set(active_id);
                                        has_sheets = true;
                                    }
                                }
                            }
                            if !has_sheets {
                                let nid = js_sys::Date::now().to_string();
                                let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false };
                                *rs_ref.borrow_mut() = vec![ns.clone()];
                                s_handle.set(vec![ns]);
                                aid_h.set(Some(nid));
                            }

                            // 4. アカウント別localStorage設定の再読み込み
                            let vim_val = get_account_storage(VIM_MODE_KEY).map(|v| v == "true").unwrap_or(true);
                            vim_auth.set(vim_val);
                            crate::js_interop::set_vim_mode(vim_val);
                            if let Some(fs_str) = get_account_storage(PREVIEW_FONT_SIZE_KEY) {
                                if let Ok(fs) = fs_str.parse::<i32>() {
                                    pfs_a.set(fs);
                                }
                            }
                        }
                    });

                    spawn_local(async move {
                        match ensure_directory_structure().await {
                            Ok(res) => {
                                nc_inner.set(true);
                                if let Ok(id_val) = js_sys::Reflect::get(&res, &JsValue::from_str("othersId")) {
                                    if let Some(id) = id_val.as_string() {
                                        ncid_i.set(Some(id.clone()));
                                        let mut us = rs_inner.borrow().clone(); let mut changed = false;
                                        for s in us.iter_mut() { if s.category.is_empty() || s.category == "OTHERS" { s.category = id.clone(); changed = true; } }
                                        if changed {
                                            *rs_inner.borrow_mut() = us.clone(); s_inner.set(us.clone()); set_gutter_status("none");
                                            for s in us.iter() { let js = s.to_js(); let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } }
                                        }
                                    }
                                }
                                if let Ok(id_val) = js_sys::Reflect::get(&res, &JsValue::from_str("leafDataId")) {
                                    if let Some(id) = id_val.as_string() {
                                        ldid_i.set(Some(id.clone()));
                                        let c_state = cats_i.clone();
                                        if let Ok(c_res) = list_folders(&id).await {
                                            if let Ok(f_val) = js_sys::Reflect::get(&c_res, &JsValue::from_str("files")) {
                                                let f_arr = js_sys::Array::from(&f_val); let mut n_cats = Vec::new();
                                                for i in 0..f_arr.length() { let v = f_arr.get(i); let ci = js_sys::Reflect::get(&v, &JsValue::from_str("id")).unwrap().as_string().unwrap(); let cn = js_sys::Reflect::get(&v, &JsValue::from_str("name")).unwrap().as_string().unwrap(); n_cats.push(JSCategory { id: ci, name: cn }); }
                                                if let Ok(v) = serde_wasm_bindgen::to_value(&n_cats) { let _ = save_categories(v).await; }
                                                c_state.set(n_cats);
                                            }
                                        }
                                        
                                        // 初期化の最後に衝突チェックを実行
                                        trigger_conflict_check(
                                            aid_ref_inner,
                                            rs_inner.clone(),
                                            s_inner.clone(),
                                            ild_inner.clone(),
                                            ifo_inner.clone(),
                                            lmk_inner,
                                            Some(is_init_inner.clone()),
                                            os_cb_final
                                        );
                                    }
                                }
                            },
                            Err(_) => {
                                is_auth_err.set(true);
                                nc_inner.set(false);
                                ifo_inner.set(true); 
                                let ifo_final = ifo_inner.clone(); 
                                Timeout::new(300, move || { ild_inner.set(false); is_init_inner.set(false); ifo_final.set(false); }).forget(); 
                            },
                        }
                    });
                }
            }) as Box<dyn FnMut(String)>);
            crate::auth_interop::init_google_auth(&client_id, &callback); callback.forget(); cleanup
        });
    }

    {
        let os = on_save_cb.clone(); let on = on_new_sheet_cb.clone();
        let oi = on_import_cb.clone(); let ip = is_preview_visible.clone();
        let iv = is_file_open_dialog_visible.clone(); let ih = is_help_visible.clone();
        let r_prev = is_preview_ref.clone(); let r_open = is_file_open_ref.clone(); let r_help = is_help_ref.clone();
        let is_auth = is_authenticated.clone(); let ast = auto_save_timer.clone(); let s_init = sheets.clone(); 
        let v_init = vim_mode.clone(); let ncid = no_category_folder_id.clone();
        let sp_init = is_suppressing_changes.clone(); let r_s = sheets_ref.clone(); let r_aid = active_id_ref.clone();
        let db_ready = db_ready_state.clone(); let aid_for_editor_init = active_sheet_id.clone();
        let is_first_edit_done_cb = is_first_edit_done_ref.clone();
        use_effect_with((is_auth, ncid.clone(), db_ready), move |deps| {
            let (auth, _, ready) = deps;
            if **auth && **ready {
                let os_i = os.clone(); let on_i = on.clone(); let oi_i = oi.clone();
                let ip_i = ip.clone(); let iv_i = iv.clone(); let ih_i = ih.clone();
                let s_state = s_init.clone(); let r_prev_i = r_prev.clone();
                let r_open_i = r_open.clone(); let r_help_i = r_help.clone();
                let timer = ast.clone(); let vim_val = *v_init; 
                let sp_ref_cb = is_suppressing_ref.clone(); let r_s_i = r_s.clone(); let r_aid_i = r_aid.clone();
                let aid_state_for_cb = aid_for_editor_init.clone();
                let is_first_done_i = is_first_edit_done_cb.clone();
                let callback = Closure::wrap(Box::new(move |cmd: String| {
                    if cmd == "save" { os_i.emit(true); }
                    else if cmd == "new_sheet" { on_i.emit(()); }
                    else if cmd == "new_local_sheet" {
                        let s = s_state.clone(); let aid_ref = r_aid_i.clone(); let sp = sp_init.clone();
                        let aid_state = aid_state_for_cb.clone(); let rs = r_s_i.clone(); let os_cb = os_i.clone();
                        let aid_val = (*aid_ref.borrow()).clone();
                        let mut needs_save = false;
                        if let Some(id) = aid_val {
                            let cur_s = (*rs.borrow()).clone();
                            if let Some(sheet) = cur_s.iter().find(|x| x.id == id) {
                                let cur_c_val = get_editor_content();
                                if let Some(cur_c) = cur_c_val.as_string() { if !cur_c.trim().is_empty() && (sheet.is_modified || sheet.content != cur_c) { needs_save = true; } }
                            }
                        }
                        if needs_save { os_cb.emit(false); }
                        sp.set(true); let delay = if needs_save { 100 } else { 0 };
                        Timeout::new(delay, move || {
                            clear_local_handle();
                            let nid = js_sys::Date::now().to_string();
                            let ns = Sheet { id: nid.clone(), guid: None, category: "__LOCAL__".to_string(), title: "Untitled.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false };
                            load_editor_content(""); set_gutter_status("local");
                            let mut current_sheets = (*rs.borrow()).clone(); current_sheets.push(ns.clone());
                            *rs.borrow_mut() = current_sheets.clone(); s.set(current_sheets); aid_ref.borrow_mut().replace(nid.clone()); aid_state.set(Some(nid.clone()));
                            focus_editor(); let spr = sp.clone(); Timeout::new(500, move || { spr.set(false); }).forget();
                            let os_final = os_cb.clone();
                            spawn_local(async move { let js = ns.to_js(); let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } os_final.emit(true); });
                        }).forget();
                    }
                    else if cmd == "open" { let val = !*r_open_i.borrow(); iv_i.set(val); sp_init.set(val); }
                    else if cmd == "import" { oi_i.emit(()); }
                    else if cmd == "preview" { let cur_c_val = get_editor_content(); let is_empty = cur_c_val.as_string().map(|s| s.trim().is_empty()).unwrap_or(true); if !*r_prev_i.borrow() && is_empty { return; } ip_i.set(!*r_prev_i.borrow()); }
                    else if cmd == "help" { ih_i.set(!*r_help_i.borrow()); }
                    else if cmd == "change" {
                        if *sp_ref_cb.borrow() { return; }
                        let cur_c_val = get_editor_content(); let cur_c = if let Some(s) = cur_c_val.as_string() { s } else { return; };
                        
                        let aid = (*r_aid_i.borrow()).clone();
                        if let Some(id) = aid {
                            let mut cur_s = (*r_s_i.borrow()).clone();
                            let mut trigger_drive_sync = false; let mut needs_upd = false;
                            if let Some(sheet) = cur_s.iter_mut().find(|s| s.id == id) {
                                let mut is_first_done = is_first_done_i.borrow_mut();
                                if !*is_first_done {
                                    // 初期化時の空データ保護：最初の同期時に空データなら無視、そうでない場合はフラグを立てて以降の空保存を許可
                                    if cur_c.is_empty() && !sheet.content.is_empty() { return; }
                                    *is_first_done = true;
                                }

                                if sheet.content != cur_c { 
                                    let now = js_sys::Date::now() as u64;
                                    sheet.content = cur_c.clone(); 
                                    sheet.is_modified = true; 
                                    sheet.temp_content = Some(cur_c.clone());
                                    sheet.temp_timestamp = Some(now);
                                    needs_upd = true; 
                                    let js = JSSheet { id: sheet.id.clone(), guid: sheet.guid.clone(), category: sheet.category.clone(), title: sheet.title.clone(), content: sheet.content.clone(), is_modified: true, drive_id: sheet.drive_id.clone(), temp_content: Some(cur_c.clone()), temp_timestamp: Some(now), last_sync_timestamp: sheet.last_sync_timestamp, tab_color: sheet.tab_color.clone(), total_size: sheet.total_size, loaded_bytes: sheet.loaded_bytes, needs_bom: sheet.needs_bom, is_preview: sheet.is_preview };
                                    spawn_local(async move { let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } });
                                }
                                trigger_drive_sync = (sheet.category != "__LOCAL__" && !sheet.category.is_empty()) || (sheet.category != "__LOCAL__" && sheet.category.is_empty() && !sheet.title.starts_with("Untitled.txt")) || sheet.category == "__LOCAL__";
                            }
                            if needs_upd { *r_s_i.borrow_mut() = cur_s.clone(); s_state.set(cur_s); }
                            if trigger_drive_sync && needs_upd { let osa = os_i.clone(); timer.set(Some(Timeout::new(1000, move || { osa.emit(false); }))); }
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
        let db_ready = db_ready_state.clone(); let sheets_rf = sheets_ref.clone();
        let ip_init = is_preview_visible.clone();
        let sp = is_suppressing_changes.clone();
        let is_first_edit_done_cb = is_first_edit_done_ref.clone();
        use_effect_with((aid, is_ld, db_ready), move |deps| {
            let (aid_val, ld_val, ready_val) = deps;
            if **ready_val && !**ld_val { 
                if let Some(id) = &**aid_val { 
                    let current_sheets = (*sheets_rf.borrow()).clone();
                    if let Some(s) = current_sheets.iter().find(|x| x.id == *id) { 
                        sp.set(true);
                        // 新しいシート（または別のシート）がロードされた時点で、初回編集フラグを落とす
                        // これで、意図的に空にして保存する際は 1 回何文字か打たないといけないのではなく、
                        // "ロード直後の自動セーブだけ" 防ぐ形になる
                        *is_first_edit_done_cb.borrow_mut() = false;
                        
                        // エディタ起動時のバグが起きなかった場合、フラグが永遠に false のままになるのを防ぐため、
                        // 1.5秒経過したら自動的に「保護」を解除する。
                        let timeout_cb = is_first_edit_done_cb.clone();
                        Timeout::new(1500, move || {
                            *timeout_cb.borrow_mut() = true;
                        }).forget();

                        load_editor_content(&s.content);
                        let mode = if s.category == "__LOCAL__" { "local" } else if s.category.is_empty() { if s.title.starts_with("Untitled.txt") { "unsaved" } else { "local" } } else if s.drive_id.is_none() && s.guid.is_none() { "unsaved" } else { "none" };
                        set_gutter_status(mode); crate::js_interop::set_editor_mode(&s.title); focus_editor();
                        ip_init.set(s.is_preview);
                        let sp_c = sp.clone();
                        Timeout::new(100, move || { sp_c.set(false); }).forget();
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

    // 保存完了待ちタブ閉じ
    {
        let psc = pending_save_close_tab.clone();
        let saving = saving_sheet_id.clone();
        let rs = sheets_ref.clone();
        let s_state = sheets.clone();
        let aid = active_sheet_id.clone();
        let sp = is_suppressing_changes.clone();
        let ncid = no_category_folder_id.clone();
        let aid_ref = active_id_ref.clone();
        use_effect_with(((*saving).clone(), (*psc).clone()), move |deps| {
            let (saving_val, psc_val) = deps;
            if saving_val.is_none() {
                if let Some(close_id) = psc_val.clone() {
                    psc.set(None);
                    close_tab_direct(close_id, rs.clone(), s_state.clone(), aid.clone(), sp.clone(), ncid.clone(), Some(aid_ref.clone()));
                }
            }
            || ()
        });
    }

    // アクティブタブIDをlocalStorageに保存
    {
        let aid = active_sheet_id.clone();
        use_effect_with((*aid).clone(), move |aid_val| {
            if let Some(id) = aid_val { set_account_storage(ACTIVE_TAB_KEY, id); }
            || ()
        });
    }

    {
        let is_auth = is_authenticated.clone(); let is_ld = is_loading.clone();
        let is_file_open = is_file_open_dialog_visible.clone(); let is_prev = is_preview_visible.clone();
        let is_help = is_help_visible.clone(); let is_logout_conf = is_logout_confirm_visible.clone();
        let has_del = pending_delete_category.clone(); let has_conf = conflict_queue.clone();
        let has_nc = name_conflict_queue.clone(); let has_fall = fallback_queue.clone();
        let is_imp_lock = is_import_lock.clone();
        let is_drop = is_category_dropdown_open.clone(); let last_obscured = use_state(|| true);
        use_effect_with( ((is_auth, is_ld, is_file_open, is_prev, is_help, is_logout_conf), (has_del, has_conf, has_nc, has_fall, is_imp_lock, is_drop)), move |deps| {
                let ((auth, ld, file_open, prev, help, logout_conf), (del, conf, nc, fall, imp_lock, drop_open)) = deps;
                let obscured = !**auth || **ld || **file_open || **prev || **help || **logout_conf || (*del).is_some() || !(*conf).is_empty() || !(*nc).is_empty() || !(*fall).is_empty() || **imp_lock || **drop_open;
                if *last_obscured && !obscured { focus_editor(); }
                last_obscured.set(obscured); || ()
            }
        );
    }

    {
                let is_auth = is_authenticated.clone(); let is_file_open = is_file_open_dialog_visible.clone();
                let is_preview = is_preview_visible.clone(); let is_help = is_help_visible.clone();
                let pending_del = pending_delete_category.clone();
                let conflicts = conflict_queue.clone(); let fallbacks = fallback_queue.clone(); let sp = is_suppressing_changes.clone();
                let is_logout_conf = is_logout_confirm_visible.clone();
                let ncq_esc = name_conflict_queue.clone(); let is_imp_lock = is_import_lock.clone();
                let oi_cb = on_import_cb.clone(); let is_drop_ev = is_category_dropdown_open.clone();
                let is_settings_ev = is_settings_visible.clone();
                let is_fd_sub = is_file_dialog_sub_active.clone(); let is_creating_cat_ev = is_creating_category.clone();
                let is_ld_ev = is_loading.clone(); let is_fo_ev = is_fading_out.clone();
                let os_cb_ev = on_save_cb.clone(); let sheets_ev = sheets.clone();
                let aid_ev = active_sheet_id.clone();
                let file_close_trigger_ev = file_close_trigger.clone();
                let close_preview_ev = close_preview.clone();
                let sheets_ref_ev = sheets_ref.clone();
                let active_id_ref_ev = active_id_ref.clone();
                let saving_id_ref_ev = saving_id_ref.clone();
                let ncid_ev = no_category_folder_id.clone();
                let pending_close_tab_ev = pending_close_tab.clone();
                let pfs_ev = preview_font_size.clone();
                let pending_close_unsynced_tab_ev = pending_close_unsynced_tab.clone();
                let nc_ev = network_connected.clone();
                let pending_save_close_tab_ev = pending_save_close_tab.clone();
                use_effect_with((*is_auth, (*is_file_open, *is_preview, *is_help, *is_logout_conf, *is_imp_lock, *is_drop_ev, *is_fd_sub, *is_creating_cat_ev, *is_ld_ev, *is_fo_ev), ((*pending_del).is_some(), !(*conflicts).is_empty(), !(*fallbacks).is_empty(), !(*ncq_esc).is_empty(), *is_settings_ev)), move |deps| {
                    let (auth, (file_open, _preview, help, logout_conf, imp_lock, drop_open, fd_sub, is_creating_cat, is_loading, is_fading_out), (has_del, has_conf, has_fall, has_nc, settings_open)) = *deps;
                    if !auth { return Box::new(|| ()) as Box<dyn FnOnce()>; }
                    let window = web_sys::window().unwrap();
                    let is_file_open_c = is_file_open.clone(); let is_preview_c = is_preview.clone();
                    let is_help_c = is_help.clone(); let pending_del_c = pending_del.clone();
                    let is_settings_c = is_settings_ev.clone(); 
                    let conflicts_c = conflicts.clone(); let fallbacks_c = fallbacks.clone(); 
                    let sp_c = sp.clone();
                    let is_logout_conf_c = is_logout_conf.clone(); let ncq_esc_c = ncq_esc.clone();
                    let oi_c = oi_cb.clone(); let is_drop_c = is_drop_ev.clone();
                    let is_creating_cat_c = is_creating_cat_ev.clone();
                    let os_c = os_cb_ev.clone(); let sheets_c = sheets_ev.clone();
                    let aid_c = aid_ev.clone();
                    let file_close_trigger_c = file_close_trigger_ev.clone();
                    let _close_preview_c = close_preview_ev.clone();
                    let rs_c = sheets_ref_ev.clone();
                    let aid_ref_c = active_id_ref_ev.clone();
                    let saving_ref_c = saving_id_ref_ev.clone();
                    let ncid_c = ncid_ev.clone();
                    let pending_close_tab_c = pending_close_tab_ev.clone();
                    let pfs_c = pfs_ev.clone();
                    let pending_close_unsynced_c = pending_close_unsynced_tab_ev.clone();
                    let nc_c = nc_ev.clone();
                    let pending_save_close_c = pending_save_close_tab_ev.clone();
                    let mut opts = EventListenerOptions::run_in_capture_phase(); opts.passive = false;
                    let listener = EventListener::new_with_options(&window, "keydown", opts, move |e| {
                        let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                        let key = ke.key(); let code = ke.code();
                        let modifier_active = ke.alt_key();
                        let is_dialog_open = file_open || help || has_del || has_conf || has_fall || logout_conf || has_nc || drop_open || is_loading || is_fading_out || is_creating_cat || settings_open;
                        let is_overlay_active = is_dialog_open || imp_lock;
                        let key_lower = key.to_lowercase();
                        let is_l_key = code == "KeyL" || key_lower == "l" || key_lower == "¬";
                        let is_h_key = code == "KeyH" || key_lower == "h" || key_lower == "˙";
                        let is_m_key = code == "KeyM" || key_lower == "m" || key_lower == "µ";
                        let is_plus_key = code == "Equal" || key == "=" || key == "+" || key == "≠";
                        let is_minus_key = code == "Minus" || key == "-" || key == "–";
                        let is_toggle_shortcut = modifier_active && (is_l_key || is_h_key || is_m_key);
                        let is_font_size_shortcut = modifier_active && (is_plus_key || is_minus_key);
                        // Modifier+アプリショートカットキーはブラウザデフォルト動作を先にブロック
                        // 注意: Cmd+N, Cmd+W はブラウザが最優先で処理するため preventDefault では防げない
                        if modifier_active {
                            let is_app_key = is_l_key || is_h_key || is_m_key || is_plus_key || is_minus_key
                                || code == "KeyN" || code == "KeyS" || code == "KeyO" || code == "KeyF" || code == "KeyW"
                                || code == "BracketLeft" || code == "BracketRight";
                            if is_app_key { e.prevent_default(); e.stop_immediate_propagation(); }
                        }
                        if is_loading || is_fading_out { e.prevent_default(); e.stop_immediate_propagation(); return; }
                        
                        // Alt + L (エディタ/Markdownレンダリング切り替え)
                        if modifier_active && is_l_key && !is_overlay_active {
                            e.prevent_default(); e.stop_immediate_propagation();
                            let new_val = !*is_preview_c;
                            is_preview_c.set(new_val);
                            // アクティブシートのis_previewを更新
                            if let Some(id) = (*aid_ref_c.borrow()).clone() {
                                let mut us = (*rs_c.borrow()).clone();
                                if let Some(sheet) = us.iter_mut().find(|s| s.id == id) {
                                    sheet.is_preview = new_val;
                                    let js = sheet.to_js();
                                    let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                    if let Ok(v) = js.serialize(&ser) { spawn_local(async move { let _ = save_sheet(v).await; }); }
                                }
                                *rs_c.borrow_mut() = us.clone();
                                sheets_c.set(us);
                            }
                            if !new_val { focus_editor(); }
                            return;
                        }

                        // Markdownモード中のキー操作（スクロール等）
                        if _preview && !is_overlay_active {
                            // ESCで編集モードに戻る
                            if key == "Escape" {
                                e.prevent_default(); e.stop_immediate_propagation();
                                is_preview_c.set(false);
                                // アクティブシートのis_previewを更新
                                if let Some(id) = (*aid_ref_c.borrow()).clone() {
                                    let mut us = (*rs_c.borrow()).clone();
                                    if let Some(sheet) = us.iter_mut().find(|s| s.id == id) {
                                        sheet.is_preview = false;
                                        let js = sheet.to_js();
                                        let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                        if let Ok(v) = js.serialize(&ser) { spawn_local(async move { let _ = save_sheet(v).await; }); }
                                    }
                                    *rs_c.borrow_mut() = us.clone();
                                    sheets_c.set(us);
                                }
                                focus_editor();
                                return;
                            }

                            let is_up = key == "PageUp";
                            let is_down = key == "PageDown";
                            let is_arrow_up = key == "ArrowUp";
                            let is_arrow_down = key == "ArrowDown";
                            let is_space = key == " ";
                            let is_home = key == "Home";
                            let is_end = key == "End";

                            // Alt+フォントサイズ変更はプレビュー用に処理
                            if modifier_active && is_font_size_shortcut {
                                e.prevent_default(); e.stop_immediate_propagation();
                                let current = get_account_storage(PREVIEW_FONT_SIZE_KEY)
                                    .and_then(|s| s.parse::<i32>().ok())
                                    .unwrap_or(14);
                                let delta = if is_plus_key { 1 } else { -1 };
                                let new_size = std::cmp::max(8, std::cmp::min(72, current + delta));
                                pfs_c.set(new_size);
                                set_account_storage(PREVIEW_FONT_SIZE_KEY, &new_size.to_string());
                                return;
                            }
                            // Alt+タブ切り替え/閉じるはそのまま通す
                            if modifier_active { /* fall through to normal shortcut handling */ }
                            else if is_up || is_down || is_arrow_up || is_arrow_down || is_home || is_end || is_space {
                                e.prevent_default(); e.stop_immediate_propagation();
                                if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                    // Markdownレンダリングのスクロールコンテナを取得（z-20の最初のdiv）
                                    if let Ok(Some(el)) = doc.query_selector(".absolute.inset-0.z-20.overflow-y-auto") {
                                        let client_height = el.client_height();
                                        let current_scroll = el.scroll_top();
                                        if is_up { el.set_scroll_top(current_scroll - client_height / 2); }
                                        else if is_down || is_space { el.set_scroll_top(current_scroll + client_height / 2); }
                                        else if is_arrow_up { el.set_scroll_top(current_scroll - 40); }
                                        else if is_arrow_down { el.set_scroll_top(current_scroll + 40); }
                                        else if is_home { el.set_scroll_top(0); }
                                        else if is_end { el.set_scroll_top(el.scroll_height()); }
                                    }
                                }
                                return;
                            } else if key == "Tab" {
                                e.prevent_default(); e.stop_immediate_propagation();
                                return;
                            } else {
                                // その他のキー入力をブロック（エディタに渡さない）
                                let is_printable = key.len() == 1 && !ke.ctrl_key() && !ke.meta_key() && !modifier_active;
                                if is_printable { e.prevent_default(); e.stop_immediate_propagation(); return; }
                            }
                        }

                        // Alt + M (FileOpen) のトグル
                        if modifier_active && is_m_key && (!is_overlay_active || *is_file_open_c) {
                            e.prevent_default(); e.stop_immediate_propagation();
                            if *is_file_open_c {
                                // 閉じる時はダイアログのアニメーション付きclose処理をトリガー
                                file_close_trigger_c.set(*file_close_trigger_c + 1);
                            } else {
                                is_file_open_c.set(true); sp_c.set(true);
                            }
                            return;
                        }
                        
                        // Alt + H (Help) のトグル
                        if modifier_active && is_h_key && (!is_overlay_active || *is_help_c) {
                            e.prevent_default(); e.stop_immediate_propagation(); 
                            let val = !*is_help_c;
                            is_help_c.set(val); 
                            if !val { focus_editor(); }
                            return; 
                        }

                        if modifier_active && !is_overlay_active {
                            if is_font_size_shortcut { e.prevent_default(); e.stop_immediate_propagation(); if is_plus_key { crate::js_interop::change_font_size(1); } else { crate::js_interop::change_font_size(-1); } return; }
                            let is_o = code == "KeyO" || key_lower == "o" || key_lower == "ø";
                            let is_f = code == "KeyF" || key_lower == "f" || key_lower == "ƒ";
                            let is_s = code == "KeyS" || key_lower == "s" || key_lower == "ß";
                            let is_n = code == "KeyN" || key_lower == "n" || key_lower == "˜";
                            let is_shift_n = (code == "KeyN" || key_lower == "n" || key_lower == "˜") && ke.shift_key();
                            if is_o { e.prevent_default(); e.stop_immediate_propagation(); oi_c.emit(()); return; }
                            if is_f { e.prevent_default(); e.stop_immediate_propagation(); crate::js_interop::focus_editor(); crate::js_interop::exec_editor_command("find"); return; }
                            if is_s { e.prevent_default(); e.stop_immediate_propagation(); crate::js_interop::exec_editor_command("saveSheet"); return; }
                            if is_shift_n { e.prevent_default(); e.stop_immediate_propagation(); crate::js_interop::exec_editor_command("newLocalSheet"); return; }
                            if is_n && !ke.shift_key() {
                                e.prevent_default(); e.stop_immediate_propagation();
                                // 現在のシートに変更があれば保存
                                if let Some(aid) = (*aid_c).clone() {
                                    if let Some(sheet) = sheets_c.iter().find(|s| s.id == aid) {
                                        if sheet.is_modified {
                                            gloo::console::log!("[Leaf-SYSTEM] Current sheet is modified. Saving before creating new sheet...");
                                            os_c.emit(false); // サイレント保存
                                        }
                                    }
                                }
                                crate::js_interop::exec_editor_command("newSheet");
                                return;
                            }
                            // Alt + [ / ] : タブ切り替え（ループ）
                            let is_bracket_left = code == "BracketLeft";
                            let is_bracket_right = code == "BracketRight";
                            if is_bracket_left || is_bracket_right {
                                e.prevent_default(); e.stop_immediate_propagation();
                                let current_sheets = (*rs_c.borrow()).clone();
                                if current_sheets.len() <= 1 { return; }
                                // RefCellから最新のactive_idを取得
                                let current_id = (*aid_ref_c.borrow()).clone();
                                if let Some(current_id) = current_id {
                                    if let Some(cur_idx) = current_sheets.iter().position(|s| s.id == current_id) {
                                        let new_idx = if is_bracket_left {
                                            if cur_idx == 0 { current_sheets.len() - 1 } else { cur_idx - 1 }
                                        } else {
                                            if cur_idx == current_sheets.len() - 1 { 0 } else { cur_idx + 1 }
                                        };
                                        let new_id = current_sheets[new_idx].id.clone();
                                        if new_id == current_id { return; }
                                        // 現在のエディタ内容を保存
                                        sp_c.set(true);
                                        let cur_c_val = get_editor_content();
                                        if let Some(cur_c) = cur_c_val.as_string() {
                                            let mut us = current_sheets;
                                            if let Some(sheet) = us.iter_mut().find(|x| x.id == current_id) {
                                                if sheet.content != cur_c {
                                                    sheet.content = cur_c;
                                                    if sheet.drive_id.is_some() || sheet.guid.is_some() { sheet.is_modified = true; }
                                                }
                                            }
                                            *rs_c.borrow_mut() = us.clone();
                                            sheets_c.set(us);
                                        }
                                        // 新タブの内容をロード
                                        let sheets_list = (*rs_c.borrow()).clone();
                                        if let Some(sheet) = sheets_list.iter().find(|s| s.id == new_id) {
                                            load_editor_content(&sheet.content);
                                            crate::js_interop::set_editor_mode(&sheet.title);
                                            if sheet.drive_id.is_none() && sheet.guid.is_none() {
                                                if sheet.category == "__LOCAL__" { set_gutter_status("local"); } else { set_gutter_status("unsaved"); }
                                            } else if sheet.is_modified { set_gutter_status("unsaved"); } else { set_gutter_status("none"); }
                                            // タブ毎の表示モードを復元
                                            is_preview_c.set(sheet.is_preview);
                                        }
                                        aid_c.set(Some(new_id.clone()));
                                        *aid_ref_c.borrow_mut() = Some(new_id);
                                        let sp_inner = sp_c.clone();
                                        Timeout::new(100, move || { sp_inner.set(false); focus_editor(); }).forget();
                                    }
                                }
                                return;
                            }
                            // Alt + W : タブを閉じる
                            let is_w = code == "KeyW";
                            if is_w {
                                e.prevent_default(); e.stop_immediate_propagation();
                                let close_id = (*aid_ref_c.borrow()).clone();
                                if let Some(close_id) = close_id {
                                    // 現在のエディタ内容を反映
                                    let cur_c_val = get_editor_content();
                                    if let Some(cur_c) = cur_c_val.as_string() {
                                        let mut us = (*rs_c.borrow()).clone();
                                        if let Some(sheet) = us.iter_mut().find(|x| x.id == close_id) {
                                            if sheet.content != cur_c { sheet.content = cur_c; }
                                        }
                                        *rs_c.borrow_mut() = us.clone();
                                        sheets_c.set(us);
                                    }
                                    // 保存中チェック（RefCellから最新値を取得）
                                    let is_saving = (*saving_ref_c.borrow()).as_ref() == Some(&close_id);
                                    if is_saving {
                                        pending_save_close_c.set(Some(close_id));
                                        return;
                                    }
                                    // 未同期チェック（オフライン＋未保存変更あり）
                                    let sheets_list = (*rs_c.borrow()).clone();
                                    if let Some(sheet) = sheets_list.iter().find(|s| s.id == close_id) {
                                        if sheet.is_modified && !*nc_c {
                                            pending_close_unsynced_c.set(Some(close_id));
                                            return;
                                        }
                                        if sheet.is_modified {
                                            pending_close_tab_c.set(Some(close_id));
                                            return;
                                        }
                                    }
                                    // 直接閉じる
                                    close_tab_direct(close_id, rs_c.clone(), sheets_c.clone(), aid_c.clone(), sp_c.clone(), ncid_c.clone(), Some(aid_ref_c.clone()));
                                }
                                return;
                            }
                        }
                        if is_overlay_active {
                            let target = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok());
                            let is_target_in_editor = target.as_ref().map(|t| t.closest("#editor").unwrap_or(None).is_some()).unwrap_or(false);
                            let is_target_body = target.as_ref().map(|t| t.tag_name().to_lowercase() == "body").unwrap_or(false);
                            if key == "Tab" && is_target_body {
                                e.prevent_default(); e.stop_immediate_propagation(); let doc = web_sys::window().unwrap().document().unwrap();
                                if let Some(overlays) = doc.get_element_by_id("overlays-layer") { if let Ok(Some(el)) = overlays.query_selector("[tabindex='0']") { let html_el = el.dyn_into::<web_sys::HtmlElement>().unwrap(); let _ = html_el.focus(); let _ = html_el.dispatch_event(&web_sys::Event::new("leaf-focus-recovery").unwrap()); } }
                                return;
                            }
                            let is_nav_key = key == "ArrowUp" || key == "ArrowDown" || key == "ArrowLeft" || key == "ArrowRight" || key == "Enter" || key == " " || key == "Tab" || key == "PageUp" || key == "PageDown" || key == "Home" || key == "End";
                            let is_char_input = key.len() == 1 && !ke.ctrl_key() && !ke.meta_key() && !modifier_active;
                            let is_edit_key = ke.ctrl_key() || ke.meta_key() || (modifier_active && !is_toggle_shortcut && !is_font_size_shortcut) || is_char_input;
                            let skip_nav_block = help && is_nav_key;
                            if (is_nav_key || is_edit_key) && !skip_nav_block { if is_target_in_editor || is_target_body { e.stop_immediate_propagation(); let is_input = target.as_ref().map(|t| t.tag_name().to_lowercase() == "input" || t.tag_name().to_lowercase() == "textarea").unwrap_or(false); if !is_input { e.prevent_default(); } } }
                            if key == "Escape" {
                                if fd_sub || file_open {
                                    // FileOpenDialogが表示中は、ダイアログ自身のon_keydownに処理を委譲する。
                                    // これによりスライドアウト/フェードアウトアニメーションが正しく再生される。
                                    return;
                                }
                                // input要素にフォーカスがある場合（カテゴリー名編集中など）はスキップ
                                let esc_target = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok());
                                let is_input_focused = esc_target.as_ref().map(|t| { let tag = t.tag_name().to_lowercase(); tag == "input" || tag == "textarea" }).unwrap_or(false);
                                if is_input_focused { return; }
                                e.stop_immediate_propagation(); e.prevent_default();
                                if is_creating_cat { is_creating_cat_c.set(false); }
                                else if drop_open { is_drop_c.set(false); }
                                else if logout_conf { is_logout_conf_c.set(false); }
                                else if has_nc { ncq_esc_c.set(Vec::new()); }
                                else if has_del { pending_del_c.set(None); }
                                else if has_conf { conflicts_c.set(Vec::new()); }
                                else if has_fall { fallbacks_c.set(Vec::new()); }
                                else if settings_open { is_settings_c.set(false); }
                                else if help { is_help_c.set(false); }
                                else if file_open { is_file_open_c.set(false); sp_c.set(false); }
                                focus_editor(); return;
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
            let cn = if sheet.category == "__LOCAL__" { "__LOCAL__".to_string() } else if sheet.category.is_empty() { "".to_string() } else { categories.iter().find(|c| c.id == sheet.category).map(|c| if c.name == "OTHERS" { i18n::t("OTHERS", lang) } else { c.name.clone() }).unwrap_or_else(|| i18n::t("OTHERS", lang)) };
            let mut file_name = sheet.title.clone();
            let mut file_ext = file_name.split('.').last().unwrap_or("txt").to_string().to_lowercase();
            let supported_exts = vec!["txt", "md", "js", "ts", "rs", "c", "cpp", "h", "m", "cs", "java", "php", "rb", "pl", "py", "sh", "coffee", "toml", "json", "xml", "html", "css", "sql", "yaml"];
            if !supported_exts.contains(&file_ext.as_str()) { file_ext = "txt".to_string(); }
            if sheet.drive_id.is_none() && sheet.guid.is_none() {
                if sheet.category == "__LOCAL__" && file_name == "Untitled.txt" { file_name = i18n::t("filename_not_specified", lang); file_ext = "txt".to_string(); }
                else if sheet.category != "__LOCAL__" && file_name.starts_with("Untitled") { file_name = "----".to_string(); file_ext = "txt".to_string(); }
            }
            (cn, file_name, file_ext)
        } else { ("".to_string(), "".to_string(), "txt".to_string()) }
    } else { ("".to_string(), "".to_string(), "txt".to_string()) };

    let is_current_new_sheet = if let Some(aid) = active_sheet_id.as_ref() { let rs = sheets_ref.borrow(); rs.iter().find(|s| s.id == *aid).map(|s| s.title.starts_with("Untitled.txt")).unwrap_or(false) } else { false };
    let is_sub_overlay_active = *is_creating_category || (*pending_delete_category).is_some() || !(*conflict_queue).is_empty() || !(*fallback_queue).is_empty() || !(*name_conflict_queue).is_empty() || *is_logout_confirm_visible || *is_install_confirm_visible || *is_install_manual_visible || (*pending_close_tab).is_some() || (*pending_close_unsynced_tab).is_some() || (*pending_save_close_tab).is_some();

    // --- Tab Bar ---
    let tab_infos: Vec<TabInfo> = {
        let rs = sheets_ref.borrow();
        let active_id = active_sheet_id.as_ref();
        // アクティブタブはエディタの最新内容から一行目を取得
        let editor_content = active_id.and_then(|_| get_editor_content().as_string());
        rs.iter().map(|s| {
            let content_for_display = if active_id == Some(&s.id) {
                editor_content.as_deref().unwrap_or(&s.content)
            } else {
                &s.content
            };
            let first_line = content_for_display.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            let is_unsaved_new = s.drive_id.is_none() && s.guid.is_none() && s.category != "__LOCAL__";
            let display = if first_line.is_empty() || is_unsaved_new { "---".to_string() } else { first_line };
            TabInfo {
                id: s.id.clone(),
                title: display,
                is_modified: s.is_modified,
                tab_color: s.tab_color.clone(),
            }
        }).collect()
    };

    let on_tab_select_cb = {
        let aid = active_sheet_id.clone();
        let aid_ref = active_id_ref.clone();
        let rs = sheets_ref.clone();
        let s_state = sheets.clone();
        let sp = is_suppressing_changes.clone();
        let ip = is_preview_visible.clone();
        Callback::from(move |new_id: String| {
            // RefCellから最新のactive_idを取得
            let current_aid = (*aid_ref.borrow()).clone();
            if current_aid.as_ref() == Some(&new_id) { return; }
            // 現在のエディタ内容を保存
            sp.set(true);
            if let Some(old_id) = current_aid {
                let cur_c_val = get_editor_content();
                if let Some(cur_c) = cur_c_val.as_string() {
                    let mut us = (*rs.borrow()).clone();
                    if let Some(sheet) = us.iter_mut().find(|x| x.id == old_id) {
                        if sheet.content != cur_c {
                            sheet.content = cur_c;
                            if sheet.drive_id.is_some() || sheet.guid.is_some() {
                                sheet.is_modified = true;
                            }
                        }
                    }
                    *rs.borrow_mut() = us.clone();
                    s_state.set(us);
                }
            }
            // 新タブの内容をロード
            let sheets_list = (*rs.borrow()).clone();
            if let Some(sheet) = sheets_list.iter().find(|s| s.id == new_id) {
                load_editor_content(&sheet.content);
                crate::js_interop::set_editor_mode(&sheet.title);
                if sheet.drive_id.is_none() && sheet.guid.is_none() {
                    if sheet.category == "__LOCAL__" { set_gutter_status("local"); } else { set_gutter_status("unsaved"); }
                } else if sheet.is_modified {
                    set_gutter_status("unsaved");
                } else {
                    set_gutter_status("none");
                }
                // タブ毎の表示モードを復元
                ip.set(sheet.is_preview);
            }
            aid.set(Some(new_id.clone()));
            *aid_ref.borrow_mut() = Some(new_id);
            let sp_inner = sp.clone();
            Timeout::new(100, move || { sp_inner.set(false); focus_editor(); }).forget();
        })
    };

    let on_tab_close_cb = {
        let rs = sheets_ref.clone();
        let pending = pending_close_tab.clone();
        let pending_unsynced = pending_close_unsynced_tab.clone();
        let s_state = sheets.clone();
        let aid = active_sheet_id.clone();
        let aid_ref = active_id_ref.clone();
        let sp = is_suppressing_changes.clone();
        let ncid = no_category_folder_id.clone();
        let nc = network_connected.clone();
        Callback::from(move |close_id: String| {
            // 現在のエディタ内容を反映
            if let Some(current_aid) = (*aid).clone() {
                if current_aid == close_id {
                    let cur_c_val = get_editor_content();
                    if let Some(cur_c) = cur_c_val.as_string() {
                        let mut us = (*rs.borrow()).clone();
                        if let Some(sheet) = us.iter_mut().find(|x| x.id == close_id) {
                            if sheet.content != cur_c { sheet.content = cur_c; }
                        }
                        *rs.borrow_mut() = us.clone();
                        s_state.set(us);
                    }
                }
            }
            // 未同期チェック（オフライン＋未保存変更あり）
            let sheets_list = (*rs.borrow()).clone();
            if let Some(sheet) = sheets_list.iter().find(|s| s.id == close_id) {
                if sheet.is_modified && !*nc {
                    pending_unsynced.set(Some(close_id));
                    return;
                }
                if sheet.is_modified {
                    pending.set(Some(close_id));
                    return;
                }
            }
            // 未保存でなければ直接閉じる
            close_tab_direct(close_id, rs.clone(), s_state.clone(), aid.clone(), sp.clone(), ncid.clone(), Some(aid_ref.clone()));
        })
    };

    let on_close_tab_confirm = {
        let pending = pending_close_tab.clone();
        let rs = sheets_ref.clone();
        let s_state = sheets.clone();
        let aid = active_sheet_id.clone();
        let aid_ref = active_id_ref.clone();
        let sp = is_suppressing_changes.clone();
        let ncid = no_category_folder_id.clone();
        Callback::from(move |_: ()| {
            if let Some(close_id) = (*pending).clone() {
                pending.set(None);
                close_tab_direct(close_id, rs.clone(), s_state.clone(), aid.clone(), sp.clone(), ncid.clone(), Some(aid_ref.clone()));
            }
        })
    };

    let on_close_unsynced_tab_confirm = {
        let pending = pending_close_unsynced_tab.clone();
        let rs = sheets_ref.clone();
        let s_state = sheets.clone();
        let aid = active_sheet_id.clone();
        let aid_ref = active_id_ref.clone();
        let sp = is_suppressing_changes.clone();
        let ncid = no_category_folder_id.clone();
        Callback::from(move |_: ()| {
            if let Some(close_id) = (*pending).clone() {
                pending.set(None);
                close_tab_direct(close_id, rs.clone(), s_state.clone(), aid.clone(), sp.clone(), ncid.clone(), Some(aid_ref.clone()));
            }
        })
    };

    html! {
        <div class="relative h-screen w-screen overflow-hidden bg-gray-950" key="app-root">
            <main key="main-editor-surface" class={classes!("absolute", "inset-0", "flex", "flex-col", "text-white", "transition-opacity", "duration-300", if !*is_authenticated && *network_connected { "opacity-0" } else { "opacity-100" } )}>
                                <ButtonBar 
                                    key="top-button-bar"
                                    on_new_sheet={on_new_sheet_cb.clone()} 
                                    on_open={on_open_dialog} 
                                    on_import={on_import_cb} 
                                    on_change_font_size={on_change_font_size.clone()} 
                                    on_change_category={on_change_category_cb} 
                                    on_preview={on_preview_cb} on_help={on_help_cb} on_logout={on_logout} current_category={current_cat.clone()} categories={(*categories).clone()} is_new_sheet={is_current_new_sheet} is_dropdown_open={*is_category_dropdown_open} on_toggle_dropdown={let id = is_category_dropdown_open.clone(); Callback::from(move |v| id.set(v))} vim_mode={*vim_mode} on_open_settings={let sv = is_settings_visible.clone(); Callback::from(move |_| sv.set(true))} file_extension={current_file_ext.clone()} on_change_extension={on_change_extension_cb.clone()} sheet_count={tab_infos.len()} on_open_sheet_list={let sl = is_sheet_list_visible.clone(); Callback::from(move |_| sl.set(true))} />
                <TabBar sheets={tab_infos.clone()} active_sheet_id={(*active_sheet_id).clone()} on_select_tab={on_tab_select_cb.clone()} on_close_tab={on_tab_close_cb.clone()} />
                <div class="flex-1 relative overflow-hidden bg-gray-900">
                    // エディタ本体（常に表示、プレビュー時はレンダリングが上に重なる）
                    <div id="editor" key="ace-editor-fixed-node" class="absolute inset-0 z-10 bg-transparent" style="width: 100%; height: 100%;"></div>

                    // Markdownレンダリング表示（インライン）
                    if *is_preview_visible {
                        { render_preview_inline(&active_sheet_id, &sheets, &current_file_ext, *preview_font_size) }
                    }

                    // フォールバック表示（エディタがロードできなかった場合用）
                    <div class="absolute inset-0 flex flex-col items-center justify-center text-gray-600 bg-gray-900 z-0">
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-16 w-12 mb-4 opacity-20" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                        </svg>
                        <p class="text-sm font-bold uppercase tracking-widest opacity-40">{ "Editor not loaded (Offline)" }</p>
                        <p class="text-[10px] mt-2 opacity-30">{ "Please reconnect to the internet to initialize the editor." }</p>
                    </div>
                </div>
                                <StatusBar 
                                    key="bottom-status-bar" 
                                    network_status={*network_connected} 
                                    is_saving={(*saving_sheet_id).as_ref() == active_sheet_id.as_ref() && active_sheet_id.is_some()}
                                    on_open_settings={let sv = is_settings_visible.clone(); Callback::from(move |_| sv.set(true))}
                                    category_name={current_cat_name}
                                    file_name={current_file_name}
                                />
            </main>
            <div id="overlays-layer" class="pointer-events-none fixed inset-0 z-[100]">
                if !*is_authenticated && *network_connected && !crate::auth_interop::is_signed_in() {
                    <div class="pointer-events-auto fixed inset-0 flex items-center justify-center bg-gray-900 overflow-y-auto p-4">
                        <div class="text-center max-w-2xl">
                            <img src="icon.svg" class="mx-auto mb-8 shadow-2xl" style="width: 15vmin; height: 15vmin;" alt="Leaf Icon" />
                            <h1 class="text-4xl font-extrabold text-white mb-6 tracking-tight">{ i18n::t("welcome_headline", lang) }</h1>
                            <div class="mb-10 text-gray-300 text-sm leading-relaxed whitespace-pre-wrap opacity-80 bg-gray-800/30 p-6 rounded-lg border border-white/5 shadow-inner text-left">{ Html::from_html_unchecked(i18n::t("app_policy_description", lang).into()) }</div>
                                                                                                                <button onclick={on_login} class="bg-emerald-600 hover:bg-emerald-700 text-white font-bold py-3 px-8 rounded-md transition-colors shadow-lg text-lg">
                                                                                                                    { i18n::t("signin_with_google", lang) }
                                                                                                                </button>
                                                                                                                <div class="mt-6 flex flex-row items-center justify-center space-x-4">
                                                                                                                    <a href={if lang == Language::Ja { "about_ja.html" } else { "about.html" }} target="_blank" class="text-gray-500 hover:text-emerald-400 text-xs underline transition-colors">
                                                                                                                        { i18n::t("about", lang) }
                                                                                                                    </a>
                                                                                                                    <a href="terms.html" target="_blank" class="text-gray-500 hover:text-emerald-400 text-xs underline transition-colors">
                                                                                                                        { "Terms / 利用規約" }
                                                                                                                    </a>
                                                                                                                    <a href="privacy.html" target="_blank" class="text-gray-500 hover:text-emerald-400 text-xs underline transition-colors">
                                                                                                                        { "Privacy / ポリシー" }
                                                                                                                    </a>
                                                                                                                    <a href="licenses.html" target="_blank" class="text-gray-500 hover:text-emerald-400 text-xs underline transition-colors">
                                                                                                                        { i18n::t("oss_licenses", lang) }
                                                                                                                    </a>
                                                                                                                </div>
                                                                                                                                                                                                                                                                                                                                                                                                        <div class="mt-4 text-gray-500 text-[10px]">{ i18n::t("login_required", lang) }</div>
                                                    </div>
                                                </div>
                }
                if *is_file_open_dialog_visible && *is_authenticated {
                    if let Some(ldid) = (*leaf_data_folder_id).clone() {
                        <div class="pointer-events-auto">
                            <FileOpenDialog 
                                on_close={let iv = is_file_open_dialog_visible.clone(); let sp = is_suppressing_changes.clone(); let aid = active_id_ref.clone(); let rs = sheets_ref.clone(); let s_state = sheets.clone(); move |_| { iv.set(false); sp.set(false); focus_editor(); let aid_val = (*aid.borrow()).clone(); let rs_c = rs.clone(); let s_state_c = s_state.clone(); if let Some(id) = aid_val { let sheets_list = (*rs_c.borrow()).clone(); if let Some(sheet) = sheets_list.iter().find(|s| s.id == id) { if !sheet.category.is_empty() && sheet.category != "__LOCAL__" { if let Some(did) = sheet.drive_id.clone() { let sheet_id = id.clone(); spawn_local(async move { if let Err(_) = crate::drive_interop::get_file_metadata(&did).await { let mut us = (*rs_c.borrow()).clone(); if let Some(s) = us.iter_mut().find(|x| x.id == sheet_id) { s.drive_id = None; s.category = "OTHERS".to_string(); s.is_modified = true; set_gutter_status("unsaved"); let js = s.to_js(); let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } } *rs_c.borrow_mut() = us.clone(); s_state_c.set(us); } }); } } } } } } 
                                on_select={on_file_sel_cb} leaf_data_id={ldid} categories={(*categories).clone()} on_refresh={on_refresh_cats_cb} on_delete_category={on_delete_category_cb} on_rename_category={on_rename_category_cb} on_delete_file={on_delete_file_cb} on_move_file={on_move_file_cb} on_start_processing={let lmk = loading_message_key.clone(); move |_| { lmk.set("synchronizing"); }} on_preview_toggle={let ifds = is_file_dialog_sub_active.clone(); Callback::from(move |v| ifds.set(v))} 
                                on_sub_active_change={let ifds = is_file_dialog_sub_active.clone(); Callback::from(move |v| ifds.set(v))}
                                is_sub_dialog_open={is_sub_overlay_active} is_creating_category={*is_creating_category} on_create_category_toggle={let ic = is_creating_category.clone(); Callback::from(move |v| ic.set(v))} 
                                refresh_files_trigger={*file_refresh_trigger} is_loading={*is_file_list_loading} on_loading_change={let l = is_file_list_loading.clone(); Callback::from(move |v| l.set(v))} 
                                on_network_status_change={let nc = network_connected.clone(); Callback::from(move |v| nc.set(v))}
                                font_size={*preview_font_size} on_change_font_size={on_change_preview_font_size.clone()}
                                is_processing={*is_processing_dialog}
                                show_ads={!*is_ad_free && !crate::js_interop::is_tauri()}
                                close_trigger={*file_close_trigger}
                                active_category_id={current_cat.clone()}
                                active_drive_id={active_sheet_id.as_ref().and_then(|id| sheets.iter().find(|s| s.id == *id).and_then(|s| s.drive_id.clone()))}
                            />
                        </div>
                    }
                }
                if let Some(help_preview) = if *is_help_visible {
                    let ih = is_help_visible.clone();
                    let c = i18n::t("help_shortcuts", lang);
                    let on_install = if !crate::js_interop::is_tauri() {
                        let is_conf = is_install_confirm_visible.clone();
                        let is_man = is_install_manual_visible.clone();
                        let ih_for_install = ih.clone();
                        Some(Callback::from(move |_: ()| { ih_for_install.set(false); if crate::js_interop::can_install_pwa() { is_conf.set(true); } else { is_man.set(true); } }))
                    } else { None };
                    Some(html! { <Preview content={c} lang={"md".to_string()} on_close={Callback::from(move |_| { ih.set(false); focus_editor(); })} on_install={on_install} is_help={true} is_sub_dialog_open={is_sub_overlay_active} font_size={*preview_font_size} on_change_font_size={on_change_font_size.clone()} /> })
                } else { None } { <div class="pointer-events-auto">{ help_preview }</div> }
                if *is_install_confirm_visible { <div class="pointer-events-auto"><ConfirmDialog title={i18n::t("install_title", lang)} message={i18n::t("install_confirm", lang)} on_confirm={let ic = is_install_confirm_visible.clone(); move |_| { ic.set(false); spawn_local(async move { crate::js_interop::trigger_pwa_install().await; }); }} on_cancel={let ic = is_install_confirm_visible.clone(); move |_| ic.set(false)} /></div> }
                if *is_install_manual_visible { <div class="pointer-events-auto"><ConfirmDialog title={i18n::t("install_manual_title", lang)} message={i18n::t("install_manual_message", lang)} ok_label={i18n::t("ok", lang)} on_confirm={let im = is_install_manual_visible.clone(); move |_| im.set(false)} on_cancel={let im = is_install_manual_visible.clone(); move |_| im.set(false)} /></div> }
                if let Some(del_diag) = if let Some(_) = *pending_delete_category { let title = i18n::t("delete", lang); let message = i18n::t("confirm_delete_category", lang); let pending = pending_delete_category.clone(); let on_cfm = on_delete_category_cfm.clone(); Some(html! { <ConfirmDialog title={title} message={message} on_confirm={move |_| { on_cfm.emit(1); }} on_cancel={move |_| { pending.set(None); }} /> }) } else { None } { <div class="pointer-events-auto">{ del_diag }</div> }
                if let Some(conf_diag) = if !conflict_queue.is_empty() { let conflict = conflict_queue.first().unwrap(); let title = if conflict.is_missing_on_drive { i18n::t("file_not_found", lang) } else { i18n::t("conflict_detected", lang) }; let message = if conflict.is_missing_on_drive { i18n::t("missing_file_message", lang).replace("{}", &conflict.title) } else { i18n::t("conflict_message", lang).replace("{}", &conflict.title) }; let options = if conflict.is_missing_on_drive { vec![DialogOption { id: 1, label: i18n::t("opt_reupload", lang) }, DialogOption { id: 3, label: i18n::t("opt_delete_local", lang) }] } else { vec![DialogOption { id: 0, label: i18n::t("opt_load_drive", lang) }, DialogOption { id: 1, label: i18n::t("opt_overwrite_drive", lang) }, DialogOption { id: 2, label: i18n::t("opt_save_new", lang) }] }; let on_cfm = on_conf_cfm.clone(); Some(html! { <CustomDialog title={title} message={message} options={options} on_confirm={on_cfm} /> }) } else { None } { <div class="pointer-events-auto">{ conf_diag }</div> }
                if let Some(fb_alert) = if let Some(_) = fallback_queue.first() { let on_cfm = on_fallback_cfm.clone(); Some(html! { <CustomDialog title={i18n::t("category_not_found_title", lang)} message={i18n::t("category_not_found_fallback", lang)} options={vec![DialogOption { id: 0, label: i18n::t("ok", lang) }]} on_confirm={on_cfm} on_cancel={let fq = fallback_queue.clone(); Some(Callback::from(move |_| { fq.set(Vec::new()); }))} /> }) } else { None } { <div class="pointer-events-auto">{ fb_alert }</div> }
                if let Some(nc_diag) = if !name_conflict_queue.is_empty() { let conflict = name_conflict_queue.first().unwrap(); let title = i18n::t("filename_conflict", lang); let message = i18n::t("filename_conflict_message", lang).replace("{}", &conflict.filename); let on_cfm = on_name_conflict_cfm.clone(); let ncq = name_conflict_queue.clone(); let labels = vec![i18n::t("opt_nc_overwrite", lang), i18n::t("opt_nc_new_guid", lang), i18n::t("opt_nc_rename", lang)]; Some(html! { <NameConflictDialog title={title} message={message} current_name={conflict.filename.clone()} labels={labels} on_confirm={on_cfm} on_cancel={move |_| { ncq.set(Vec::new()); }} /> }) } else { None } { <div class="pointer-events-auto">{ nc_diag }</div> }
                <LoadingOverlay is_visible={*is_import_lock} message={i18n::t("synchronizing", lang)} is_fading_out={*is_import_fading_out} z_index="z-[90]" />
                if *is_loading { <div class={classes!("fixed", "inset-0", "z-[200]", "flex", "items-center", "justify-center", "bg-gray-900", "transition-opacity", "duration-300", "pointer-events-auto", if *is_fading_out { "opacity-0" } else { "opacity-100" } )}><div class="flex flex-col items-center">if *is_initial_load { <img src="icon.svg" class="mb-8 shadow-2xl animate-in fade-in zoom-in duration-500" style="width: 20vmin; height: 20vmin;" alt="Leaf Icon" /> }<div class="w-12 h-12 border-4 border-emerald-500 border-t-transparent rounded-full animate-spin"></div>if *is_authenticated { <p class="mt-4 text-white font-bold text-lg animate-pulse">{ i18n::t(*loading_message_key, lang) }</p> }</div></div> }
                if *is_logout_confirm_visible { <div class="pointer-events-auto"><ConfirmDialog title={i18n::t("logout", lang)} message={i18n::t("confirm_logout", lang)} on_confirm={let ic = is_logout_confirm_visible.clone(); let il = is_loading.clone(); let lmk = loading_message_key.clone(); let ifo = is_fading_out.clone(); move |_| { ic.set(false); lmk.set("logging_out"); il.set(true); ifo.set(false); spawn_local(async move { crate::auth_interop::sign_out().await; Timeout::new(800, move || { web_sys::window().unwrap().location().set_href("/").unwrap(); }).forget(); }); } } on_cancel={let ic = is_logout_confirm_visible.clone(); move |_| ic.set(false)} /></div> }
                if (*pending_close_tab).is_some() { <div class="pointer-events-auto"><ConfirmDialog title={i18n::t("close_tab", lang)} message={i18n::t("confirm_close_unsaved_tab", lang)} on_confirm={on_close_tab_confirm} on_cancel={let pc = pending_close_tab.clone(); move |_| pc.set(None)} /></div> }
                if (*pending_close_unsynced_tab).is_some() { <div class="pointer-events-auto"><ConfirmDialog title={i18n::t("close_tab", lang)} message={i18n::t("confirm_close_unsynced_tab", lang)} ok_label={i18n::t("close_anyway", lang)} cancel_label={i18n::t("cancel", lang)} on_confirm={on_close_unsynced_tab_confirm} on_cancel={let pc = pending_close_unsynced_tab.clone(); move |_| pc.set(None)} /></div> }
                if (*pending_save_close_tab).is_some() {
                    <div class="pointer-events-auto fixed inset-0 z-[250] flex items-center justify-center bg-black/60">
                        <div class="flex flex-col items-center">
                            <div class="w-10 h-10 border-4 border-red-500 border-t-transparent rounded-full animate-spin mb-4"></div>
                            <p class="text-red-400 font-bold text-2xl animate-pulse">{ i18n::t("saving_please_wait", lang) }</p>
                        </div>
                    </div>
                }
                if *is_sheet_list_visible {
                    <div class="pointer-events-auto">
                        <SheetListPanel
                            sheets={tab_infos.clone()}
                            active_sheet_id={(*active_sheet_id).clone()}
                            on_select_tab={on_tab_select_cb.clone()}
                            on_close_tab={on_tab_close_cb.clone()}
                            on_close_panel={let sl = is_sheet_list_visible.clone(); Callback::from(move |_| sl.set(false))}
                        />
                    </div>
                }
                if *is_settings_visible {
                    <div class="pointer-events-auto">
                        <SettingsDialog
                            vim_mode={*vim_mode}
                            on_toggle_vim={on_toggle_vim}
                            on_close={let sv = is_settings_visible.clone(); Callback::from(move |_| { sv.set(false); focus_editor(); })}
                        />
                    </div>
                }
            </div>
        </div>
    }
}
