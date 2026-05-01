use yew::prelude::*;
use crate::components::button_bar::ButtonBar;
use crate::components::status_bar::StatusBar;
use crate::components::tab_bar::{TabBar, TabInfo, SheetListPanel, ReorderEvent};
use crate::components::dialog::{CustomDialog, DialogOption, ConfirmDialog, NameConflictDialog, LoadingOverlay};
use crate::components::file_open_dialog::FileOpenDialog;
use crate::components::settings_dialog::{SettingsDialog, EmptySaveBehavior};
use crate::components::shortcut_help::ShortcutHelp;
use crate::components::char_code_dialog::CharCodeDialog;
use crate::components::sheet_info_dialog::SheetInfoDialog;
use crate::components::empty_sheet_dialog::EmptySheetDialog;
use crate::components::tab_select_dialog::{TabSelectDialog, TabSelectItem};
use crate::js_interop::{init_editor, set_vim_mode, get_editor_content, load_editor_content, focus_editor, set_gutter_status, set_preview_active, generate_uuid, open_local_file, save_local_file, clear_local_handle};
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
    pub is_split: bool,              // メモリのみ（スプリットプレビュー状態）
    pub editor_state: Option<String>, // メモリのみ（カーソル/スクロール位置）
    pub preview_scroll_top: f64,     // メモリのみ（プレビュースクロール位置）
    pub created_at: Option<u64>,
    pub local_path: Option<String>,  // メモリのみ（ローカルファイルの絶対パス、Tauriのみ）
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
            created_at: self.created_at,
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
const EDITOR_THEME_KEY: &str = "leaf_editor_theme";
const EMPTY_SAVE_KEY: &str = "leaf_empty_save_behavior";
const WINDOW_OPACITY_KEY: &str = "leaf_window_opacity";
const WINDOW_BLUR_KEY: &str = "leaf_window_blur";
const TERMINAL_FONT_SIZE_KEY: &str = "leaf_terminal_font_size";
const GUEST_MODE_KEY: &str = "leaf_guest_mode";
const LOCAL_AUTO_SAVE_KEY: &str = "leaf_local_auto_save";

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

#[derive(Properties, PartialEq)]
struct InlinePreviewProps {
    pub content: String,
    pub file_ext: String,
    pub font_size: i32,
    #[prop_or_default]
    pub initial_scroll_top: f64,
    #[prop_or_default]
    pub is_split: bool,
}

#[function_component(InlinePreview)]
fn inline_preview(props: &InlinePreviewProps) -> Html {
    use yew::AttrValue;
    let node_ref = use_node_ref();
    let scroll_ref = use_node_ref();
    let is_markdown = props.file_ext == "md" || props.file_ext == "markdown";
    let libs_ready = use_state(|| crate::js_interop::is_marked_loaded());
    let file_ext = props.file_ext.clone();
    let initial_scroll = props.initial_scroll_top;

    // Markdownライブラリの遅延ロード＋ポーリング
    {
        let libs_ready = libs_ready.clone();
        use_effect_with(file_ext, move |ext| {
            let is_md = ext == "md" || ext == "markdown";
            if !is_md {
                libs_ready.set(true);
                return Box::new(|| ()) as Box<dyn FnOnce()>;
            }
            if crate::js_interop::is_marked_loaded() {
                libs_ready.set(true);
                return Box::new(|| ()) as Box<dyn FnOnce()>;
            }
            crate::js_interop::preload_markdown_libs();
            let libs_ready = libs_ready.clone();
            let interval = gloo::timers::callback::Interval::new(100, move || {
                if crate::js_interop::is_marked_loaded() {
                    libs_ready.set(true);
                }
            });
            Box::new(move || drop(interval)) as Box<dyn FnOnce()>
        });
    }

    let loading = is_markdown && !*libs_ready;

    let rendered_html = if loading {
        "".to_string()
    } else if is_markdown {
        crate::js_interop::render_markdown(&props.content)
    } else {
        // Markdown以外のプレビュー: markdown-bodyと同じ見た目に揃えるため、code blockとして描画
        let code_html = crate::js_interop::highlight_code(&props.content, &props.file_ext);
        format!(r#"<pre><code class="hljs language-{}">{}</code></pre>"#, props.file_ext, code_html)
    };

    // Mermaid初期化
    {
        let node_ref = node_ref.clone();
        let is_md = is_markdown;
        let ready = *libs_ready;
        let content = props.content.clone();
        use_effect_with((content, ready), move |_| {
            if is_md && ready {
                if let Some(el) = node_ref.cast::<web_sys::Element>() {
                    crate::js_interop::init_mermaid(&el);
                }
            }
            || ()
        });
    }

    // スクロール位置復元
    {
        let scroll_ref = scroll_ref.clone();
        let ready = *libs_ready;
        use_effect_with((ready, initial_scroll), move |(ready, scroll)| {
            if *ready && *scroll > 0.0 {
                let s = *scroll;
                let sr = scroll_ref.clone();
                Timeout::new(50, move || {
                    if let Some(el) = sr.cast::<web_sys::Element>() {
                        el.set_scroll_top(s as i32);
                    }
                }).forget();
            }
            || ()
        });
    }

    let container_class = if props.is_split {
        "h-full w-full overflow-y-auto bg-[#fdf6e3]"
    } else {
        "absolute inset-0 overflow-y-auto bg-[#fdf6e3]"
    };
    let container_id = if props.is_split { "split-preview-scroll" } else { "" };
    html! {
        <div ref={scroll_ref} id={container_id} class={container_class}>
            // ローディング表示
            <div class={classes!("absolute", "inset-0", "flex", "items-center", "justify-center", if loading { "" } else { "hidden" })}>
                <div class="flex flex-col items-center">
                    <div class="w-10 h-10 border-4 border-emerald-500 border-t-transparent rounded-full animate-spin mb-4"></div>
                    <p class="text-emerald-500/70 text-sm font-bold animate-pulse">{ "Loading..." }</p>
                </div>
            </div>
            // コンテンツ（Markdown以外のプレビューもmarkdown-bodyスタイルで統一）
            <div
                ref={node_ref}
                class={classes!("markdown-body", "max-w-none", "p-6", "sm:p-12", if loading { "hidden" } else { "" })}
                style={format!("font-size: {}pt;", props.font_size)}
            >
                { Html::from_html_unchecked(AttrValue::from(rendered_html)) }
            </div>
        </div>
    }
}

#[derive(Clone)]
struct TerminalSplitHandles {
    ts_state: UseStateHandle<bool>,
    ts_ref: Rc<RefCell<bool>>,
    tse_state: UseStateHandle<bool>,
    tse_ref: Rc<RefCell<bool>>,
    sps_state: UseStateHandle<Option<String>>,
    sps_ref: Rc<RefCell<Option<String>>>,
    skip_fade: Rc<RefCell<bool>>,
    map: Rc<RefCell<std::collections::HashMap<String, (bool, bool, Option<String>)>>>,
}

fn close_tab_direct(
    close_id: String,
    rs: Rc<RefCell<Vec<Sheet>>>,
    s_state: UseStateHandle<Vec<Sheet>>,
    aid: UseStateHandle<Option<String>>,
    sp: UseStateHandle<bool>,
    ncid: UseStateHandle<Option<String>>,
    aid_ref: Option<Rc<RefCell<Option<String>>>>,
    tab_order: Vec<String>,
    atid_handle: Option<UseStateHandle<Option<String>>>,
    atref_handle: Option<Rc<RefCell<Option<String>>>>,
    tsh: Option<TerminalSplitHandles>,
) {
    let ts_map = tsh.as_ref().map(|h| h.map.clone());
    sp.set(true);
    // 閉じるシートのUndo履歴をクリア
    crate::js_interop::clear_undo_state(&close_id);
    crate::js_interop::destroy_sheet_session(&close_id);
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
                total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0,
                created_at: Some(js_sys::Date::now() as u64),
                local_path: None,

            };
            us.push(ns.clone());
            *rs.borrow_mut() = us.clone();
            s_state.set(us);
            aid.set(Some(nid.clone()));
            if let Some(ref r) = aid_ref { *r.borrow_mut() = Some(nid.clone()); }
            if let Some(ref h) = atid_handle { h.set(None); }
            if let Some(ref h) = atref_handle { *h.borrow_mut() = None; }
            crate::js_interop::activate_sheet_session(&nid, "", "Untitled.txt");
            set_gutter_status("unsaved");
            let sp_inner = sp.clone();
            Timeout::new(100, move || { sp_inner.set(false); focus_editor(); }).forget();
            spawn_local(async move {
                let js = ns.to_js();
                let ser = serde_wasm_bindgen::Serializer::json_compatible();
                if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
            });
        } else {
            // 現在ターミナルがアクティブかどうか
            let on_terminal = if let Some(ref h) = atref_handle {
                h.borrow().is_some()
            } else { false };
            // RefCellから最新のactive_idを取得（UseStateHandleはstaleの可能性がある）
            // ターミナルアクティブ時は sheet close でタブ切り替えを行わない（ターミナルのコンテキストを維持）
            let was_active = if on_terminal {
                false
            } else if let Some(ref r) = aid_ref {
                r.borrow().as_ref() == Some(&close_id)
            } else {
                aid.as_ref() == Some(&close_id)
            };
            *rs.borrow_mut() = us.clone();
            s_state.set(us.clone());

            // ターミナル上では aid を変更しない（変更すると use_effect_with(aid) が発火し
            // is_preview_visible が書き換わってスプリットビューに影響するため）。
            // aid が閉じたシートを指していても、ユーザーが次にシートに切り替える時に解決される。

            if was_active {
                // tab_orderから左隣を取得（ロックされたシートはスキップ）
                // ターミナルタブと非ロックのシートタブのみ採用
                let is_id_valid = |id: &str| -> bool {
                    if id.starts_with("__TERM__") { return true; }
                    if let Some(ref tsm) = ts_map {
                        !is_sheet_locked_by_terminal(id, &tsm.borrow())
                    } else { true }
                };
                let order_idx = tab_order.iter().position(|x| x == &close_id);
                // 左隣から順に有効なタブを探す、見つからなければ右隣から
                let mut next_id: Option<String> = None;
                if let Some(idx) = order_idx {
                    for i in (0..idx).rev() {
                        if let Some(candidate) = tab_order.get(i) {
                            if is_id_valid(candidate) {
                                next_id = Some(candidate.clone());
                                break;
                            }
                        }
                    }
                    if next_id.is_none() {
                        for i in (idx + 1)..tab_order.len() {
                            if let Some(candidate) = tab_order.get(i) {
                                if is_id_valid(candidate) {
                                    next_id = Some(candidate.clone());
                                    break;
                                }
                            }
                        }
                    }
                }
                // 上記で見つからなければ us から近い位置を選ぶ（フォールバック）
                if next_id.is_none() {
                    let new_idx = if pos > 0 { pos - 1 } else { 0 };
                    next_id = us.get(new_idx).and_then(|s| {
                        if is_id_valid(&s.id) { Some(s.id.clone()) } else { None }
                    });
                }

                let focus_terminal_id: Option<String> = if let Some(ref next) = next_id {
                    if next.starts_with("__TERM__") {
                        // ターミナルをアクティブにし、スプリット状態を ts_map から復元
                        if let Some(ref h) = tsh {
                            let (t_split, t_edit, t_pane) = h.map.borrow().get(next.as_str()).cloned().unwrap_or((false, false, None));
                            *h.skip_fade.borrow_mut() = true;
                            h.ts_state.set(t_split);
                            *h.ts_ref.borrow_mut() = t_split;
                            h.tse_state.set(t_edit);
                            *h.tse_ref.borrow_mut() = t_edit;
                            h.sps_state.set(t_pane.clone());
                            *h.sps_ref.borrow_mut() = t_pane;
                        }
                        if let Some(ref h) = atid_handle { h.set(Some(next.clone())); }
                        if let Some(ref h) = atref_handle { *h.borrow_mut() = Some(next.clone()); }
                        Some(next.clone())
                    } else {
                        // シートをアクティブに: ターミナルスプリット関連の state をリセット
                        if let Some(ref h) = tsh {
                            *h.skip_fade.borrow_mut() = true;
                            let sheet_is_split = us.iter().find(|s| s.id == *next).map(|s| s.is_split).unwrap_or(false);
                            h.ts_state.set(sheet_is_split);
                            *h.ts_ref.borrow_mut() = sheet_is_split;
                            h.tse_state.set(false);
                            *h.tse_ref.borrow_mut() = false;
                            h.sps_state.set(None);
                            *h.sps_ref.borrow_mut() = None;
                        }
                        if let Some(ref h) = atid_handle { h.set(None); }
                        if let Some(ref h) = atref_handle { *h.borrow_mut() = None; }
                        if let Some(sheet) = us.iter().find(|s| s.id == *next) {
                            crate::js_interop::activate_sheet_session(next, &sheet.content, &sheet.title);
                            if sheet.drive_id.is_none() && sheet.guid.is_none() {
                                if sheet.category == "__LOCAL__" { set_gutter_status("local"); } else { set_gutter_status("unsaved"); }
                            } else if sheet.is_modified {
                                set_gutter_status("unsaved");
                            } else {
                                set_gutter_status("none");
                            }
                        }
                        aid.set(Some(next.clone()));
                        if let Some(ref r) = aid_ref { *r.borrow_mut() = Some(next.clone()); }
                        None
                    }
                } else {
                    None
                };

                let sp_inner = sp.clone();
                Timeout::new(100, move || {
                    sp_inner.set(false);
                    if let Some(ref tid) = focus_terminal_id {
                        crate::js_interop::terminal_focus(tid);
                    } else {
                        focus_editor();
                    }
                }).forget();
            } else {
                // 非アクティブタブを閉じた場合: アクティブタブは変更しない
                let sp_inner = sp.clone();
                Timeout::new(100, move || { sp_inner.set(false); focus_editor(); }).forget();
            }
        }

        // IndexedDBから削除
        spawn_local(async move {
            let _ = crate::db_interop::delete_sheet(&sheet_id).await;
        });
    }
}

// 指定シートが、いずれかのターミナルのスプリットペインに選択されているかを判定
// ts_mapのエントリから、split_enabled=true かつ pane_sheet_id が一致するものを探す
fn is_sheet_locked_by_terminal(
    sheet_id: &str,
    ts_map: &std::collections::HashMap<String, (bool, bool, Option<String>)>,
) -> bool {
    ts_map.values().any(|(split_enabled, _edit, pane)| {
        *split_enabled && pane.as_deref() == Some(sheet_id)
    })
}

fn trigger_conflict_check(
    aid_ref: Rc<RefCell<Option<String>>>,
    s_ref: Rc<RefCell<Vec<Sheet>>>,
    s_state: UseStateHandle<Vec<Sheet>>,
    ild: UseStateHandle<bool>,
    ifo: UseStateHandle<bool>,
    lmk: UseStateHandle<&'static str>,
    is_init: Option<UseStateHandle<bool>>,
    on_save: Callback<(bool, Option<String>)>
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
                                        // 現在アクティブなシートが同期対象と同じ場合のみエディタにロード（他のシートへの切り替え中は更新しない）
                                        // セッション管理下では update_sheet_content_external で session の content を更新する
                                        crate::js_interop::update_sheet_content_external(&sheet_id, &drive_content);
                                    }
                                    let ild = ild_inner.clone(); let ifo = ifo_inner.clone(); let isi = is_init_inner.clone();
                                    ifo.set(true);
                                    Timeout::new(300, move || { ild.set(false); ifo.set(false); if let Some(h) = isi { h.set(false); } }).forget();
                                } else {
                                    // ローカルの方が新しい、または一致
                                    if is_modified || local_time > drive_time + 1000 {
                                        gloo::console::log!(format!("[Leaf-SYSTEM] Local is newer. Triggering silent auto-upload..."));
                                        on_save_inner.emit((false, None));
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
    let config_str = include_str!("../config/application.toml");
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
    let tab_closing_id = use_state(|| None::<String>);
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
    let preview_overlay_opacity = use_state(|| false); // プレビューオーバーレイのopacity制御
    let split_pane_sheet_id: UseStateHandle<Option<String>> = use_state(|| None); // 分割ペインに表示するシートID (None=アクティブシート)
    let split_pane_sheet_id_ref: Rc<RefCell<Option<String>>> = use_mut_ref(|| None); // 上記のコールバックから常に最新値を読むためのref
    let is_tab_select_dialog_visible = use_state(|| false); // タブ選択ダイアログ（デスクトップ版）
    let is_split_close_dialog_visible = use_state(|| false); // スプリットクローズ選択ダイアログ
    let split_close_selected = use_state(|| 0usize); // 0=ターミナル, 1=プレビュー
    let split_close_selected_ref = use_mut_ref(|| 0usize);
    let terminal_font_size = use_state(|| {
        get_account_storage(TERMINAL_FONT_SIZE_KEY)
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(14)
    });
    let terminal_font_size_ref = use_mut_ref(|| {
        get_account_storage(TERMINAL_FONT_SIZE_KEY)
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(14)
    });
    let split_ratio = use_state(|| 0.5f64);
    let split_ratio_ref = use_mut_ref(|| 0.5f64);
    let is_splitter_dragging = use_mut_ref(|| false);
    let terminal_split_enabled = use_state(|| false);
    let terminal_split_ref = use_mut_ref(|| false);
    let terminal_split_map = use_mut_ref(|| std::collections::HashMap::<String, (bool, bool, Option<String>)>::new()); // (スプリット状態, 編集モード, スプリットペインシートID)
    let split_pane_mounted = use_state(|| false);   // 右ペインをDOMに保持するか
    let split_pane_opacity = use_state(|| false);   // 右ペインのopacity (true=100, false=0)
    let skip_split_fade = use_mut_ref(|| false);    // タブ切り替え時はフェードをスキップ
    let split_pane_is_terminal = use_state(|| false); // フェードアウト中のコンテンツ種別
    let split_content_opacity = use_state(|| true); // 右ペイン内コンテンツのopacity（編集モードトグル時フェード用）
    let split_pane_cached_content = use_mut_ref(|| "".to_string()); // フェードアウト中に保持するコンテンツ
    let terminal_split_edit_mode = use_state(|| false); // ターミナルスプリット右ペイン編集モード
    let terminal_split_edit_ref = use_mut_ref(|| false);
    let split_edit_debounce = use_mut_ref(|| None::<Timeout>);
    let is_help_visible = use_state(|| false);
    let is_sheet_info_visible = use_state(|| false);
    let is_char_code_visible = use_state(|| false);
    let char_code_char = use_state(|| String::new());
    let is_suppressing_changes = use_state(|| false);
    let pending_delete_category = use_state(|| None::<String>);
    let is_processing_dialog = use_state(|| false);
    let is_install_confirm_visible = use_state(|| false);
    let is_settings_visible = use_state(|| false);
    let editor_theme = use_state(|| {
        get_account_storage(EDITOR_THEME_KEY).unwrap_or_else(|| "gruvbox".to_string())
    });
    let empty_save_behavior = use_state(|| {
        get_account_storage(EMPTY_SAVE_KEY).map(|v| EmptySaveBehavior::from_str(&v)).unwrap_or(EmptySaveBehavior::Confirm)
    });
    let pending_empty_delete = use_state(|| None::<String>); // 空データ削除確認用のsheet_id
    let window_opacity = use_state(|| {
        get_account_storage(WINDOW_OPACITY_KEY).and_then(|v| v.parse::<i32>().ok()).unwrap_or(100)
    });
    let window_blur = use_state(|| {
        get_account_storage(WINDOW_BLUR_KEY).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0)
    });
    let is_install_manual_visible = use_state(|| false);

    let is_ad_free = use_state(|| false);
    let is_guest_mode = use_state(|| false);
    let local_auto_save = use_state(|| {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item(LOCAL_AUTO_SAVE_KEY).ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(false)
    });
    let local_auto_save_ref = use_mut_ref(|| {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item(LOCAL_AUTO_SAVE_KEY).ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(false)
    });
    let pending_close_tab = use_state(|| None::<String>);
    let pending_close_unsynced_tab = use_state(|| None::<String>);
    let pending_save_close_tab = use_state(|| None::<String>);
    let is_sheet_list_visible = use_state(|| false);
    let terminal_ids_ref = use_mut_ref(|| Vec::<String>::new());
    let active_terminal_id = use_state(|| None::<String>);
    let active_terminal_ref = use_mut_ref(|| None::<String>);
    let terminal_counter = use_mut_ref(|| 0u32);
    let terminal_tab_ids = use_state(|| Vec::<String>::new());
    // 統合タブ順序（シート+ターミナルのIDを表示順に保持）
    let tab_order_ref = use_mut_ref(|| Vec::<String>::new());
    let tab_order_state = use_state(|| Vec::<String>::new());

    let sheets_ref = use_mut_ref(|| Vec::<Sheet>::new());
    let active_id_ref = use_mut_ref(|| None::<String>);
    let no_category_id_ref = use_mut_ref(|| None::<String>);
    let is_loading_ref = use_mut_ref(|| true);
    let saving_id_ref = use_mut_ref(|| None::<String>);
    let is_suppressing_ref = use_mut_ref(|| false);
    let empty_save_ref = use_mut_ref(|| EmptySaveBehavior::Confirm);
    let is_first_edit_done_ref = use_mut_ref(|| false);
    let is_preview_ref = use_mut_ref(|| false);
    let is_file_open_ref = use_mut_ref(|| false);
    let is_help_ref = use_mut_ref(|| false);

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

        let r_esb = empty_save_ref.clone();
        let esb = empty_save_behavior.clone();
        use_effect_with((((*s).clone(), (*aid).clone(), (*ncid).clone()), (*ld, *sp, *prev, *open, *help, (*saving_sheet_id).clone()), *esb), move |deps| {
            let ((s_val, aid_val, ncid_val), (ld_val, sp_val, prev_val, open_val, help_val, saving_val), esb_val) = deps;
            *r_s.borrow_mut() = s_val.clone(); *r_aid.borrow_mut() = aid_val.clone();
            *r_ncid.borrow_mut() = ncid_val.clone(); *r_ld.borrow_mut() = *ld_val; *r_sp.borrow_mut() = *sp_val;
            *r_prev.borrow_mut() = *prev_val; *r_open.borrow_mut() = *open_val; *r_help.borrow_mut() = *help_val;
            *r_saving.borrow_mut() = saving_val.clone();
            *r_esb.borrow_mut() = *esb_val;
            || ()
        });
    }

    let on_login = Callback::from(|_: MouseEvent| { request_access_token(); });

    let on_guest_login_cb = {
        let is_auth = is_authenticated.clone();
        let is_guest = is_guest_mode.clone();
        let is_ld = is_loading.clone();
        let is_fo = is_fading_out.clone();
        let is_in = is_initial_load.clone();
        let is_auth_flag_g = is_auth_flag.clone();
        let s_state = sheets.clone();
        let rs = sheets_ref.clone();
        let aid = active_sheet_id.clone();
        let db_ready = db_ready_state.clone();
        let vim_g = vim_mode.clone();
        let pfs_g = preview_font_size.clone();
        let et_g = editor_theme.clone();
        Callback::from(move |_: MouseEvent| {
            // ゲストモードをlocalStorageに保存
            if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
                let _ = storage.set_item(GUEST_MODE_KEY, "true");
            }
            // URLを /editor に更新 (Web版)
            if !crate::js_interop::is_tauri() {
                if let Some(win) = web_sys::window() {
                    if let Ok(hist) = win.history() {
                        let path = win.location().pathname().unwrap_or_default();
                        if path == "/login" {
                            let _ = hist.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/"));
                        }
                    }
                }
            }
            *is_auth_flag_g.borrow_mut() = true;
            is_auth.set(true);
            is_guest.set(true);

            let s = s_state.clone(); let r = rs.clone(); let a = aid.clone();
            let ld = is_ld.clone(); let fo = is_fo.clone(); let ini = is_in.clone();
            let db_r = db_ready.clone();
            let vim = vim_g.clone(); let pfs = pfs_g.clone(); let et = et_g.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(_) = crate::db_interop::init_db("LeafDB").await {
                    gloo::console::error!("Guest DB init failed");
                }
                let mut has_sheets = false;
                if let Ok(val) = crate::db_interop::load_sheets().await {
                    if let Ok(loaded) = serde_wasm_bindgen::from_value::<Vec<JSSheet>>(val) {
                        if !loaded.is_empty() {
                            let mapped: Vec<Sheet> = loaded.into_iter().map(|s| Sheet {
                                id: s.id, guid: s.guid, category: "__LOCAL__".to_string(), title: s.title,
                                content: s.temp_content.clone().unwrap_or(s.content),
                                is_modified: s.temp_timestamp.is_some(), drive_id: None,
                                temp_content: s.temp_content, temp_timestamp: s.temp_timestamp,
                                last_sync_timestamp: s.last_sync_timestamp,
                                tab_color: if s.tab_color.is_empty() { generate_random_color() } else { s.tab_color },
                                total_size: s.total_size, loaded_bytes: s.loaded_bytes,
                                needs_bom: s.needs_bom, is_preview: s.is_preview,
                                is_split: false, editor_state: None, preview_scroll_top: 0.0,
                                created_at: s.created_at,
                                local_path: None,
                
                            }).collect();
                            let saved_active = web_sys::window()
                                .and_then(|w| w.local_storage().ok().flatten())
                                .and_then(|st| st.get_item(ACTIVE_TAB_KEY).ok().flatten());
                            let active_id = saved_active
                                .and_then(|id| mapped.iter().find(|s| s.id == id).map(|s| s.id.clone()))
                                .or_else(|| mapped.last().map(|s| s.id.clone()));
                            *r.borrow_mut() = mapped.clone();
                            s.set(mapped);
                            a.set(active_id);
                            has_sheets = true;
                        }
                    }
                }
                if !has_sheets {
                    let nid = js_sys::Date::now().to_string();
                    let ns = Sheet { id: nid.clone(), guid: None, category: "__LOCAL__".to_string(), title: "Untitled 1.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: Some(js_sys::Date::now() as u64), local_path: None };
                    *r.borrow_mut() = vec![ns.clone()]; s.set(vec![ns]); a.set(Some(nid));
                }
                // 設定の読み込み
                let vim_val = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).and_then(|st| st.get_item(VIM_MODE_KEY).ok().flatten()).map(|v| v == "true").unwrap_or(true);
                vim.set(vim_val);
                crate::js_interop::set_vim_mode(vim_val);
                if let Some(fs_str) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).and_then(|st| st.get_item("leaf_preview_font_size").ok().flatten()) {
                    if let Ok(fs) = fs_str.parse::<i32>() { pfs.set(fs); }
                }
                let theme_val = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).and_then(|st| st.get_item(EDITOR_THEME_KEY).ok().flatten()).unwrap_or_else(|| "gruvbox".to_string());
                crate::js_interop::set_editor_theme(&theme_val);
                et.set(theme_val);
                db_r.set(true);
                fo.set(true);
                let ld2 = ld.clone(); let fo2 = fo.clone(); let ini2 = ini.clone();
                Timeout::new(300, move || { ld2.set(false); fo2.set(false); ini2.set(false); }).forget();
            });
        })
    };

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
                        let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: Some(js_sys::Date::now() as u64), local_path: None };
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

    let os_handle: Rc<RefCell<Option<Callback<(bool, Option<String>)>>>> = Rc::new(RefCell::new(None));
    let on_save_cb = {
        let r_aid = active_id_ref.clone(); let r_s = sheets_ref.clone(); let s_state = sheets.clone();
        let r_ncid = no_category_id_ref.clone(); let nc_h = network_connected.clone();
        let ild_h = is_loading.clone();
        let lock_h = is_import_lock.clone();
        let lock_fade_h = is_import_fading_out.clone();
        let lmk_h = loading_message_key.clone();
        let ris_h = saving_id_ref.clone(); let is_saving_h = saving_sheet_id.clone();
        let ncq_h = name_conflict_queue.clone();
        let esb_ref_h = empty_save_ref.clone();
        let ped_h = pending_empty_delete.clone();
        let osh_cb = os_handle.clone();
        let cq_save = conflict_queue.clone();
        let ifo_save = is_fading_out.clone();
        Callback::from(move |(is_manual, override_id): (bool, Option<String>)| {
            // override_id があればそれを使う（タブ切替時など、aid と異なるシートを保存する場合）
            // None の場合は現在の aid を使う
            let id = match override_id.clone() {
                Some(o) => {
                    if let Some(ref saving_id) = *ris_h.borrow() { if saving_id == &o { return; } }
                    o
                },
                None => {
                    let aid_opt = (*r_aid.borrow()).clone();
                    match aid_opt {
                        Some(id) => {
                            if let Some(ref saving_id) = *ris_h.borrow() { if saving_id == &id { return; } }
                            id
                        },
                        None => return,
                    }
                }
            };

            // 同一シートの並行保存をブロックするため ris_h を即時マーク
            // （保存完了後またはエラー時に必ず None に戻す）
            *ris_h.borrow_mut() = Some(id.clone());
            is_saving_h.set(Some(id.clone()));

            // 保存対象シートの content は IndexedDB と同期した sheets_ref から読む。
            // override_id がある場合（タブ切替後等）は live editor 内容と異なるため必須。
            // override_id が None でも、change handler が sheets_ref[aid] を即時更新するので
            // 常に sheets_ref から読めば一貫性が保たれる。
            let captured_content = {
                let s_ref = r_s.borrow();
                match s_ref.iter().find(|s| s.id == id) {
                    Some(sheet) => sheet.content.clone(),
                    None => { *ris_h.borrow_mut() = None; is_saving_h.set(None); return; }
                }
            };
            gloo::console::log!(format!("[Leaf-DBG] on_save_cb ENTER id={} captured.first20={:?} is_manual={} override={}", id, captured_content.chars().take(20).collect::<String>(), is_manual, override_id.is_some()));

            let r_aid = r_aid.clone(); let r_s = r_s.clone(); let s_state = s_state.clone();
            let r_ncid = r_ncid.clone(); let nc_h = nc_h.clone();
            let ild_h = ild_h.clone();
            let lmk_h = lmk_h.clone();
            let lock_h = lock_h.clone();
            let lock_fade_h = lock_fade_h.clone();
            let ris_h = ris_h.clone(); let is_saving_h = is_saving_h.clone();
            let ncq_h = ncq_h.clone();
            let esb_ref = esb_ref_h.clone();
            let ped_h = ped_h.clone();
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

                        // 自動保存時：キャプチャした内容が空だが、sheet.contentに内容がある場合はsheet.contentを使う
                        // （リロード直後等でエディタが未描画の場合のフォールバック）
                        let cur_c = if cur_c.trim().is_empty() && !sheet.content.trim().is_empty() {
                            sheet.content.clone()
                        } else {
                            cur_c
                        };

                        // 空データ処理（自動保存・手動保存共通）
                        // cur_cが空で、シートがDriveまたはGUIDに紐付いている場合（一度は保存されたファイル）
                        if cur_c.trim().is_empty() && (sheet.drive_id.is_some() || sheet.guid.is_some()) {
                            let behavior = *esb_ref.borrow();
                            match behavior {
                                EmptySaveBehavior::Nothing => {
                                    gloo::console::log!("[Leaf-SYSTEM] Auto-save: empty content, doing nothing (user setting).");
                                    *ris_h.borrow_mut() = None; is_saving_h.set(None);
                                    return;
                                },
                                EmptySaveBehavior::Confirm => {
                                    gloo::console::log!("[Leaf-SYSTEM] Auto-save: empty content, showing confirm dialog.");
                                    ped_h.set(Some(id.clone()));
                                    *ris_h.borrow_mut() = None; is_saving_h.set(None);
                                    return;
                                },
                                EmptySaveBehavior::Delete => {
                                    gloo::console::log!("[Leaf-SYSTEM] Auto-save: empty content, deleting sheet (user setting).");
                                    // 直接削除（ダイアログなし）
                                    let rs_del = r_s.clone(); let s_del = s_state.clone();
                                    let aid_del = r_aid.clone();
                                    let ncid_del = r_ncid.clone();
                                    let sheet_id = id.clone();
                                    // Timeoutで次のイベントループで実行（借用競合回避）
                                    Timeout::new(0, move || {
                                        let aid_val = (*aid_del.borrow()).clone();
                                        let ncid_val = (*ncid_del.borrow()).clone();
                                        let mut us = (*rs_del.borrow()).clone();
                                        if let Some(pos) = us.iter().position(|s| s.id == sheet_id) {
                                            us.remove(pos);
                                            if us.is_empty() {
                                                let nid = js_sys::Date::now().to_string();
                                                let ns = Sheet { id: nid.clone(), guid: None, category: ncid_val.unwrap_or_default(), title: "Untitled.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: Some(js_sys::Date::now() as u64), local_path: None };
                                                us.push(ns.clone());
                                                *rs_del.borrow_mut() = us.clone();
                                                s_del.set(us);
                                                load_editor_content("");
                                                set_gutter_status("unsaved");
                                                spawn_local(async move { let js = ns.to_js(); let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } });
                                            } else {
                                                let was_active = aid_val.as_ref() == Some(&sheet_id);
                                                *rs_del.borrow_mut() = us.clone();
                                                s_del.set(us.clone());
                                                if was_active {
                                                    let new_idx = if pos > 0 { pos - 1 } else { 0 };
                                                    let new_sheet = &us[new_idx];
                                                    load_editor_content(&new_sheet.content);
                                                    crate::js_interop::set_editor_mode(&new_sheet.title);
                                                }
                                            }
                                            spawn_local(async move { let _ = crate::db_interop::delete_sheet(&sheet_id).await; });
                                        }
                                    }).forget();
                                    *ris_h.borrow_mut() = None; is_saving_h.set(None);
                                    return;
                                },
                            }
                        }

                        if !is_manual && !sheet.is_modified && sheet.content == cur_c { *ris_h.borrow_mut() = None; is_saving_h.set(None); return; }
                        
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
                            let ris_local = ris_h.clone();
                            let ild_inner = ild_h.clone();
                            let lock_inner = lock_h.clone();
                            let lock_fade_inner = lock_fade_h.clone();
                            let sheet_id = id.clone();
                            let rs_cb_inner = r_s.clone();
                            let s_state_inner = s_state.clone();
                            let n_bom = sheet.needs_bom;
                            let local_path_for_save = sheet.local_path.clone();

                            s_state.set(cur_s.clone());
                            if is_manual { is_saving_h.set(Some(id.clone())); } // マニュアル時のみIDセット
                            // シートに保存されたローカルパスをJSグローバルに設定（正しいパスに保存するため）
                            // local_pathが無ければ空文字でクリアし、保存ダイアログを出させる
                            crate::js_interop::set_local_file_path(local_path_for_save.as_deref().unwrap_or(""));
                            spawn_local(async move {
                                let result = save_local_file(&content_to_save, n_bom).await;
                                // Tauri版はオブジェクト {name, path}、Web版は文字列で返る
                                let (fname_opt, saved_path) = if result.is_null() || result.is_undefined() {
                                    (None, None)
                                } else if let Some(s) = result.as_string() {
                                    (Some(s), None)
                                } else {
                                    let n = js_sys::Reflect::get(&result, &JsValue::from_str("name")).ok().and_then(|v| v.as_string());
                                    let p = js_sys::Reflect::get(&result, &JsValue::from_str("path")).ok().and_then(|v| v.as_string());
                                    (n, p)
                                };
                                if let Some(fname) = fname_opt {
                                    let mut us = (*rs_cb_inner.borrow()).clone();
                                    if let Some(s) = us.iter_mut().find(|x| x.id == sheet_id) {
                                        s.category = "__LOCAL__".to_string();
                                        s.title = fname.clone();
                                        if saved_path.is_some() { s.local_path = saved_path.clone(); }
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
                                    *ris_local.borrow_mut() = None;
                                    is_saving_inner.set(None);
                                    ild_inner.set(false);
                                    if *lock_inner {
                                        lock_fade_inner.set(true);
                                        let l = lock_inner.clone(); let lf = lock_fade_inner.clone();
                                        let _il = ild_inner.clone();
                                        Timeout::new(300, move || { lf.set(false); l.set(false); _il.set(false); }).forget();
                                    }
                                } else {
                                    *ris_local.borrow_mut() = None;
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
                                ild_h.set(false); lock_h.set(false); *ris_h.borrow_mut() = None; is_saving_h.set(None); return;
                            }
                        }
                    }

                    // local_save_triggered の場合、ris_h は spawn_local 内で後処理する
                    if local_save_triggered { return; }
                    if !drive_save_prepared { *ris_h.borrow_mut() = None; is_saving_h.set(None); return; }

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
                         gloo::console::log!(format!("[Leaf-DBG] spawn_local START sheet.id={} drive_id={:?} category={} title={} content.len={}", sheet.id, sheet.drive_id, sheet.category, sheet.title, sheet.content.len()));
                         let _structure = match ensure_directory_structure().await { Ok(res) => res, Err(_) => {
                             gloo::console::warn!(format!("[Leaf-DBG] ensure_directory_structure FAILED sheet.id={}", sheet.id));
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
                                 gloo::console::warn!(format!("[Leaf-DBG] category metadata FAILED sheet.id={} category={}", sheet.id, sheet.category));
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
                                             gloo::console::log!(format!("[Leaf-DBG] Pre-save check: drive_time={} sync_ts={} diff={} ts_str={}", drive_time, sync_ts, drive_time as i64 - sync_ts as i64, ts));
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
                         gloo::console::log!(format!("[Leaf-DBG] upload_file CALL sheet.id={} fname={} drive_id={:?} content.len={}", sheet.id, fname, sheet.drive_id, final_content.len()));
                         let res = upload_file(&fname, &JsValue::from_str(final_content), &target_folder_id, sheet.drive_id.as_deref()).await;
                         let mut n_did = sheet.drive_id.clone(); let mut stime = sheet.last_sync_timestamp;
                         match res {
                             Ok(rv) => {
                                 if let Ok(iv) = js_sys::Reflect::get(&rv, &JsValue::from_str("id")) { if let Some(is) = iv.as_string() { n_did = Some(is); } }
                                 let mut new_stime_str: Option<String> = None;
                                 if let Ok(tv) = js_sys::Reflect::get(&rv, &JsValue::from_str("modifiedTime")) { if let Some(ts) = tv.as_string() { new_stime_str = Some(ts.clone()); stime = Some(crate::drive_interop::parse_date(&ts) as u64); } }
                                 // PATCH 応答の modifiedTime と直後の GET metadata の modifiedTime が
                                 // 数十秒ずれることがあるため、再取得して権威的な値を sync_ts として保存する
                                 if let Some(ref did) = n_did {
                                     if let Ok(metadata) = get_file_metadata(did).await {
                                         if let Ok(tv) = js_sys::Reflect::get(&metadata, &JsValue::from_str("modifiedTime")) {
                                             if let Some(ts) = tv.as_string() {
                                                 let auth_ts = crate::drive_interop::parse_date(&ts) as u64;
                                                 // 通常 PATCH 応答 ≦ GET の値。GET 側を採用
                                                 if auth_ts >= stime.unwrap_or(0) {
                                                     gloo::console::log!(format!("[Leaf-DBG] Upload sync_ts adjust: patch_resp={:?} authoritative={} ts_str={:?}", new_stime_str, auth_ts, ts));
                                                     new_stime_str = Some(ts);
                                                     stime = Some(auth_ts);
                                                 }
                                             }
                                         }
                                     }
                                 }
                                 // 応答全体のキー一覧を出力（modifiedTime が無い場合の診断用）
                                 let keys: Vec<String> = js_sys::Object::keys(&rv.clone().unchecked_into::<js_sys::Object>()).iter().filter_map(|k| k.as_string()).collect();
                                 gloo::console::log!(format!("[Leaf-DBG] Upload OK: sheet_id={} new_stime={:?} ts_str={:?} response_keys={:?}", sheet.id, stime, new_stime_str, keys));
                                 nc_inner.set(true); // 成功したのでオンラインに
                             },
                             Err(ref e) => {
                                 gloo::console::warn!(format!("[Leaf-DBG] upload_file FAILED sheet.id={} err={:?}", sheet.id, e));
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
                                     let id_for_retry = sheet.id.clone();
                                     Timeout::new(1000, move || { cb.emit((false, Some(id_for_retry))); }).forget();
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
            let os_retry = os.clone(); Timeout::new(100, move || { os_retry.emit((true, None)); }).forget();
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
        let atid_new = active_terminal_id.clone();
        let atref_new = active_terminal_ref.clone();
        let ts_new = terminal_split_enabled.clone();
        let ts_ref_new = terminal_split_ref.clone();
        let ssf_new = skip_split_fade.clone();
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
                        Timeout::new(200, move || { os_retry.emit((false, None)); }).forget();
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
                os_cb.emit((false, None));
            }

            sp.set(true); 
            // 保存が必要な場合は確実にキャプチャされるまで少し待機
            let delay = if needs_save { 150 } else { 0 };
            let ncid_for_new = ncid_h.clone();
            let is_ld_inner = is_loading_handle.clone();
            let ifo_inner = ifo_handle.clone();
            let is_creating_inner = is_creating_handle.clone();
            let atid_new_inner = atid_new.clone();
            let atref_new_inner = atref_new.clone();
            let ts_new_inner = ts_new.clone();
            let ts_ref_new_inner = ts_ref_new.clone();
            let ssf_new_inner = ssf_new.clone();
            Timeout::new(delay, move || {
                clear_local_handle();
                let nid = js_sys::Date::now().to_string();
                let cat_id = (*ncid_for_new).clone().unwrap_or_else(|| "".to_string());
                let ns = Sheet { id: nid.clone(), guid: None, category: cat_id, title: "Untitled.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: Some(js_sys::Date::now() as u64), local_path: None };
                load_editor_content(""); set_gutter_status("unsaved");

                let mut current_sheets = (*rs.borrow()).clone();
                current_sheets.push(ns.clone());
                *rs.borrow_mut() = current_sheets.clone();
                s.set(current_sheets);
                // ターミナルをクリアして新規シートをアクティブに
                atid_new_inner.set(None);
                *atref_new_inner.borrow_mut() = None;
                // スプリット状態をリセット（フェードなし）
                *ssf_new_inner.borrow_mut() = true;
                ts_new_inner.set(false);
                *ts_ref_new_inner.borrow_mut() = false;
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
            *rs.borrow_mut() = us.clone(); s_state.set(us); fq.set(q); os.emit((true, None));
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
                    let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: Some(js_sys::Date::now() as u64), local_path: None };
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
                        
                        // 自動保存の監視を再開（3秒後にチェック）
                        if let Some(osa_cb) = (*osa.borrow()).as_ref() {
                            let osa_cb = osa_cb.clone();
                            timer.set(Some(Timeout::new(3000, move || { osa_cb.emit((false, None)); })));
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
        let ts_sel = terminal_split_enabled.clone();
        let ts_ref_sel = terminal_split_ref.clone();
        let ssf_sel = skip_split_fade.clone();
        let atid_fo = active_terminal_id.clone();
        let atref_fo = active_terminal_ref.clone();
        let ts_map_fo = terminal_split_map.clone();
        let tse_fo = terminal_split_edit_mode.clone();
        let tse_ref_fo = terminal_split_edit_ref.clone();
        let spid_fo = split_pane_sheet_id.clone();
        let spid_ref_fo = split_pane_sheet_id_ref.clone();
        Callback::from(move |(did, title, cat_id): (String, String, String)| {
            // アクティブシートに未保存変更があれば、aid 切替前に Drive 保存を発火
            // （on_save_cb は override_id 付きで切替元シートを sheets_ref から読んで保存）
            let aid_val = (*aid).clone();
            let mut save_target_id: Option<String> = None;
            if let Some(id) = aid_val.clone() {
                let cur_s = (*rs.borrow()).clone();
                if let Some(sheet) = cur_s.iter().find(|x| x.id == id) {
                    let cur_c_val = get_editor_content();
                    if let Some(cur_c) = cur_c_val.as_string() { if !cur_c.trim().is_empty() && (sheet.is_modified || sheet.content != cur_c) { save_target_id = Some(id); } }
                }
            }
            if let Some(target_id) = save_target_id { os.emit((false, Some(target_id))); }

            // 既に同じdrive_idのシートが開かれている場合はそのシートをアクティブにして終了
            {
                let cur_s = (*rs.borrow()).clone();
                if let Some(existing) = cur_s.iter().find(|s| s.drive_id.as_ref() == Some(&did)) {
                    let existing_id = existing.id.clone();
                    iv.set(false); // ファイル選択ダイアログを閉じる
                    aid.set(Some(existing_id.clone()));
                    load_editor_content(&existing.content);
                    crate::js_interop::set_editor_mode(&existing.title);
                    focus_editor();
                    return;
                }
            }

            iv.set(false); lmk.set("synchronizing"); il.set(true); ifo.set(false); sp.set(true);
            // ターミナルがアクティブな場合、スプリット状態をts_mapに保存してターミナルコンテキストを抜ける
            let prev_tid = atref_fo.borrow().clone();
            if let Some(tid) = prev_tid {
                let ts_val = *ts_ref_sel.borrow();
                let tse_val = *tse_ref_fo.borrow();
                let spid_val = spid_ref_fo.borrow().clone();
                ts_map_fo.borrow_mut().insert(tid, (ts_val, tse_val, spid_val));
                atid_fo.set(None);
                *atref_fo.borrow_mut() = None;
                *tse_ref_fo.borrow_mut() = false;
                tse_fo.set(false);
                spid_fo.set(None);
                *spid_ref_fo.borrow_mut() = None;
            }

            let ss_inner = ss.clone(); let aid_inner = aid.clone(); let sp_inner = sp.clone();
            let il_inner = il.clone(); let ifo_inner = ifo.clone(); let rs_inner = rs.clone();
            let ts_sel_inner = ts_sel.clone();
            let ts_ref_sel_inner = ts_ref_sel.clone();
            let ssf_sel_inner = ssf_sel.clone();
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
                    let ns = Sheet { id: nid.clone(), guid: guid.clone(), category: cat_id.clone(), title: title.clone(), content: c.clone(), is_modified: false, drive_id: Some(did.clone()), temp_content: None, temp_timestamp: None, last_sync_timestamp: sync_ts, tab_color: if let Some(idx) = tidx { cs[idx].tab_color.clone() } else if let Some(idx) = existing_idx { cs[idx].tab_color.clone() } else { generate_random_color() }, total_size: c_len, loaded_bytes: c_len, needs_bom: has_bom, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: None, local_path: None };
                    load_editor_content(&c); set_gutter_status("none");
                    if let Some(idx) = tidx { cs[idx] = ns.clone(); } else if let Some(idx) = existing_idx { cs[idx] = ns.clone(); } else { cs.push(ns.clone()); }
                    *rs_inner.borrow_mut() = cs.clone(); ss_inner.set(cs); aid_inner.set(Some(nid.clone()));
                    // スプリット状態をリセット（フェードなし）
                    *ssf_sel_inner.borrow_mut() = true;
                    ts_sel_inner.set(false);
                    *ts_ref_sel_inner.borrow_mut() = false;
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
        let ts_imp = terminal_split_enabled.clone();
        let ts_ref_imp = terminal_split_ref.clone();
        let ssf_imp = skip_split_fade.clone();
        let atid_imp = active_terminal_id.clone();
        let atref_imp = active_terminal_ref.clone();
        let ts_map_imp = terminal_split_map.clone();
        let tse_imp = terminal_split_edit_mode.clone();
        let tse_ref_imp = terminal_split_edit_ref.clone();
        let spid_imp = split_pane_sheet_id.clone();
        let spid_ref_imp = split_pane_sheet_id_ref.clone();
        Callback::from(move |_| {
            // ターミナルがアクティブな場合、スプリット状態をts_mapに保存してターミナルコンテキストを抜ける
            let prev_tid = atref_imp.borrow().clone();
            if let Some(tid) = prev_tid {
                let ts_val = *ts_ref_imp.borrow();
                let tse_val = *tse_ref_imp.borrow();
                let spid_val = spid_ref_imp.borrow().clone();
                ts_map_imp.borrow_mut().insert(tid, (ts_val, tse_val, spid_val));
                atid_imp.set(None);
                *atref_imp.borrow_mut() = None;
                *tse_ref_imp.borrow_mut() = false;
                tse_imp.set(false);
                spid_imp.set(None);
                *spid_ref_imp.borrow_mut() = None;
            }
            let aid_val = (*aid_state).clone();
            let mut save_target_id: Option<String> = None;
            if let Some(id) = aid_val.clone() {
                let cur_s = (*r_s.borrow()).clone();
                if let Some(sheet) = cur_s.iter().find(|x| x.id == id) {
                    let cur_c_val = get_editor_content();
                    if let Some(cur_c) = cur_c_val.as_string() { if !cur_c.trim().is_empty() && (sheet.is_modified || sheet.content != cur_c) { save_target_id = Some(id); } }
                }
            }
            if let Some(target_id) = save_target_id { os.emit((false, Some(target_id))); }

            let s_state_c = s_state.clone(); let aid_state_c = aid_state.clone();
            let sp_state_c = sp_state.clone(); let r_s_c = r_s.clone();
            let lock_cb = lock_h.clone(); let il_cb = il_h.clone(); let ifo_cb = ifo_h.clone();
            let lock_fade_cb = lock_fade_h.clone(); let lmk_cb = lmk_h.clone();
            let ts_imp_c = ts_imp.clone();
            let ts_ref_imp_c = ts_ref_imp.clone();
            let ssf_imp_c = ssf_imp.clone();
            spawn_local(async move {
                let res = open_local_file().await; if res.is_null() || res.is_undefined() { return; }

                let content_val = js_sys::Reflect::get(&res, &JsValue::from_str("content")).ok().and_then(|v| v.as_string());
                let bytes_val = js_sys::Reflect::get(&res, &JsValue::from_str("bytes")).ok();
                let name_val = js_sys::Reflect::get(&res, &JsValue::from_str("name")).ok().and_then(|v| v.as_string());
                let path_val = js_sys::Reflect::get(&res, &JsValue::from_str("path")).ok().and_then(|v| v.as_string());

                // 既に同じファイル名のローカルシートが開かれていればそのタブをアクティブにして終了
                if let Some(ref name) = name_val {
                    let cur_s = (*r_s_c.borrow()).clone();
                    if let Some(existing) = cur_s.iter().find(|s| s.category == "__LOCAL__" && s.title == *name) {
                        let existing_id = existing.id.clone();
                        aid_state_c.set(Some(existing_id));
                        load_editor_content(&existing.content);
                        set_gutter_status("local");
                        crate::js_interop::set_editor_mode(&existing.title);
                        focus_editor();
                        lock_cb.set(false);
                        il_cb.set(false);
                        return;
                    }
                }

                if let (Some(name), Some(content), Some(bytes_js)) = (name_val, content_val, bytes_val) {
                    let bytes = js_sys::Uint8Array::new(&bytes_js).to_vec();
                    let has_bom = has_utf8_bom(&bytes);

                    lmk_cb.set("synchronizing"); ifo_cb.set(false); lock_fade_cb.set(false); il_cb.set(true); lock_cb.set(true);
                    let nid = js_sys::Date::now().to_string();
                    let ns = Sheet { id: nid.clone(), guid: None, category: "__LOCAL__".to_string(), title: name.clone(), content: content.clone(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: content.len() as u64, loaded_bytes: content.len() as u64, needs_bom: has_bom, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: Some(js_sys::Date::now() as u64), local_path: path_val };
                    sp_state_c.set(true);
                    let mut current = (*r_s_c.borrow()).clone();
                    // 未保存の新規シート1枚のみなら置換、それ以外はpush
                    if current.len() == 1 && current[0].drive_id.is_none() && current[0].content.is_empty() {
                        current[0] = ns.clone();
                    } else {
                        current.push(ns.clone());
                    }
                    *r_s_c.borrow_mut() = current.clone(); s_state_c.set(current); aid_state_c.set(Some(nid.clone()));
                    // スプリット状態をリセット（フェードなし）
                    *ssf_imp_c.borrow_mut() = true;
                    ts_imp_c.set(false);
                    *ts_ref_imp_c.borrow_mut() = false;
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
                        Timeout::new(0, move || { os_inner.emit((true, None)); }).forget(); return;
                    }
                    if new_cat_id == "__LOCAL__" { sheet.category = "__LOCAL__".to_string(); let mut us = current_sheets; us[pos] = sheet; *r_s_inner.borrow_mut() = us.clone(); s_state_inner.set(us); Timeout::new(0, move || { os_inner.emit((true, None)); }).forget(); return; }

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
    let on_sheet_info_cb = { let iv = is_sheet_info_visible.clone(); Callback::from(move |_| { iv.set(true); }) };
    let on_terminal_split_cb = {
        let ts = terminal_split_enabled.clone();
        let ts_ref = terminal_split_ref.clone();
        let spid = split_pane_sheet_id.clone();
        let spid_ref = split_pane_sheet_id_ref.clone();
        let tab_sel = is_tab_select_dialog_visible.clone();
        let aid_ref = active_id_ref.clone();
        let atref_ts = active_terminal_ref.clone();
        let ts_map_ts = terminal_split_map.clone();
        Callback::from(move |_| {
            let split_open = *ts_ref.borrow();
            if split_open {
                *ts_ref.borrow_mut() = false;
                ts.set(false);
                spid.set(None);
                *spid_ref.borrow_mut() = None;
                // ts_map のエントリも更新してシートのロック解除
                if let Some(tid) = atref_ts.borrow().as_ref().cloned() {
                    ts_map_ts.borrow_mut().insert(tid, (false, false, None));
                }
            } else if aid_ref.borrow().is_some() {
                tab_sel.set(true);
            }
        })
    };
    let close_preview = {
        let fo = is_preview_fading_out.clone();
        let iv = is_preview_visible.clone();
        let op = preview_overlay_opacity.clone();
        Callback::from(move |_: ()| {
            fo.set(true);
            op.set(false); // フェードアウト開始
            let iv = iv.clone();
            let fo = fo.clone();
            gloo::timers::callback::Timeout::new(300, move || {
                iv.set(false);
                fo.set(false);
                crate::js_interop::focus_editor();
            }).forget();
        })
    };
    let on_preview_cb = {
        let ip = is_preview_visible.clone();
        let op = preview_overlay_opacity.clone();
        let rs = sheets_ref.clone();
        let s_state = sheets.clone();
        let aid_ref = active_id_ref.clone();
        Callback::from(move |_| {
            let new_val = !*ip;
            ip.set(new_val);
            if new_val {
                // フェードイン: opacity-0 で表示してから opacity-100 へ
                op.set(false);
                let op_c = op.clone();
                gloo::timers::callback::Timeout::new(10, move || { op_c.set(true); }).forget();
            }
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
        let is_guest_mode_init = is_guest_mode.clone();
        let is_auth_flag_init = is_auth_flag.clone();
        let vim_mode_init = vim_mode.clone();
        let pfs_init = preview_font_size.clone();
        let et_init = editor_theme.clone();

        use_effect_with((), move |_| {
            spawn_local(async move {
                // ゲストモードの自動初期化チェック
                let is_guest_from_storage = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .and_then(|s| s.get_item(GUEST_MODE_KEY).ok().flatten())
                    .map(|v| v == "true")
                    .unwrap_or(false);
                // Web版でパス "/" でアクセスかつGoogle未ログインの場合もゲストモード自動起動
                let current_path = web_sys::window()
                    .and_then(|w| w.location().pathname().ok())
                    .unwrap_or_default();
                let is_guest = is_guest_from_storage
                    || (current_path == "/" && !crate::auth_interop::is_signed_in() && !crate::js_interop::is_tauri());

                if is_guest {
                    *is_auth_flag_init.borrow_mut() = true;
                    if let Err(_) = crate::db_interop::init_db("LeafDB").await {
                        gloo::console::error!("Guest DB init failed");
                    }
                    let mut has_sheets = false;
                    if let Ok(val) = crate::db_interop::load_sheets().await {
                        if let Ok(loaded) = serde_wasm_bindgen::from_value::<Vec<JSSheet>>(val) {
                            if !loaded.is_empty() {
                                let mapped: Vec<Sheet> = loaded.into_iter().map(|s| Sheet {
                                    id: s.id, guid: s.guid, category: "__LOCAL__".to_string(), title: s.title,
                                    content: s.temp_content.clone().unwrap_or(s.content),
                                    is_modified: s.temp_timestamp.is_some(), drive_id: None,
                                    temp_content: s.temp_content, temp_timestamp: s.temp_timestamp,
                                    last_sync_timestamp: s.last_sync_timestamp,
                                    tab_color: if s.tab_color.is_empty() { generate_random_color() } else { s.tab_color },
                                    total_size: s.total_size, loaded_bytes: s.loaded_bytes,
                                    needs_bom: s.needs_bom, is_preview: s.is_preview,
                                    is_split: false, editor_state: None, preview_scroll_top: 0.0,
                                    created_at: s.created_at,
                                local_path: None,
                    
                                }).collect();
                                let saved_active = web_sys::window()
                                    .and_then(|w| w.local_storage().ok().flatten())
                                    .and_then(|s| s.get_item(ACTIVE_TAB_KEY).ok().flatten());
                                let active_id = saved_active
                                    .and_then(|id| mapped.iter().find(|s| s.id == id).map(|s| s.id.clone()))
                                    .or_else(|| mapped.last().map(|s| s.id.clone()));
                                *rs.borrow_mut() = mapped.clone();
                                s_handle.set(mapped);
                                aid_handle.set(active_id);
                                has_sheets = true;
                            }
                        }
                    }
                    if !has_sheets {
                        let nid = js_sys::Date::now().to_string();
                        let ns = Sheet { id: nid.clone(), guid: None, category: "__LOCAL__".to_string(), title: "Untitled 1.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: Some(js_sys::Date::now() as u64), local_path: None };
                        *rs.borrow_mut() = vec![ns.clone()]; s_handle.set(vec![ns]); aid_handle.set(Some(nid));
                    }
                    // 設定の読み込み
                    let vim_val = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).and_then(|s| s.get_item(VIM_MODE_KEY).ok().flatten()).map(|v| v == "true").unwrap_or(true);
                    vim_mode_init.set(vim_val);
                    crate::js_interop::set_vim_mode(vim_val);
                    if let Some(fs_str) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).and_then(|s| s.get_item("leaf_preview_font_size").ok().flatten()) {
                        if let Ok(fs) = fs_str.parse::<i32>() { pfs_init.set(fs); }
                    }
                    let theme_val = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).and_then(|s| s.get_item(EDITOR_THEME_KEY).ok().flatten()).unwrap_or_else(|| "gruvbox".to_string());
                    crate::js_interop::set_editor_theme(&theme_val);
                    et_init.set(theme_val);
                    // URLを /editor に更新 (Web版)
                    if !crate::js_interop::is_tauri() {
                        if let Some(win) = web_sys::window() {
                            if let Ok(hist) = win.history() {
                                let path = win.location().pathname().unwrap_or_default();
                                if path == "/login" {
                                    let _ = hist.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/"));
                                }
                            }
                        }
                    }
                    is_auth_init.set(true);
                    is_guest_mode_init.set(true);
                    is_fo_init.set(true);
                    let ild = is_ld_init.clone(); let ifo = is_fo_init.clone(); let isi = is_in_init.clone();
                    Timeout::new(300, move || { ild.set(false); ifo.set(false); isi.set(false); }).forget();
                    db_loaded_init.set(true);
                    return;
                }

                let db_name = account_db_name();
                let has_account = db_name != "LeafDB"; // メールが取得できた場合のみDBを開く

                if has_account {
                    if let Err(_) = crate::db_interop::init_db(&db_name).await { gloo::console::error!("DB init failed"); }
                    if let Ok(c_val) = crate::db_interop::load_categories().await { if let Ok(loaded_cats) = serde_wasm_bindgen::from_value::<Vec<JSCategory>>(c_val) { cats_handle.set(loaded_cats); } }
                    let mut initial = true;
                    if let Ok(val) = crate::db_interop::load_sheets().await {
                        if let Ok(loaded) = serde_wasm_bindgen::from_value::<Vec<JSSheet>>(val) {
                            if !loaded.is_empty() {
                                let mapped: Vec<Sheet> = loaded.into_iter().map(|s| Sheet { id: s.id, guid: s.guid, category: s.category, title: s.title, content: s.temp_content.clone().unwrap_or(s.content), is_modified: s.temp_timestamp.is_some(), drive_id: s.drive_id, temp_content: s.temp_content, temp_timestamp: s.temp_timestamp, last_sync_timestamp: s.last_sync_timestamp, tab_color: if s.tab_color.is_empty() { generate_random_color() } else { s.tab_color }, total_size: s.total_size, loaded_bytes: s.loaded_bytes, needs_bom: s.needs_bom, is_preview: s.is_preview, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: s.created_at, local_path: None }).collect();
                                // 保存されたアクティブタブIDを復元、なければ最後のシート
                                let saved_active = get_account_storage(ACTIVE_TAB_KEY);
                                let active_id = saved_active.and_then(|id| mapped.iter().find(|s| s.id == id).map(|s| s.id.clone())).or_else(|| mapped.last().map(|s| s.id.clone()));
                                *rs.borrow_mut() = mapped.clone(); s_handle.set(mapped); aid_handle.set(active_id); initial = false;
                            }
                        }
                    }
                    if initial {
                        let nid = js_sys::Date::now().to_string();
                        let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: Some(js_sys::Date::now() as u64), local_path: None };
                        *rs.borrow_mut() = vec![ns.clone()]; s_handle.set(vec![ns]); aid_handle.set(Some(nid));
                    }
                }

                // オフラインの場合は、認証を待たずに起動
                if !is_online_init {
                    gloo::console::log!("[Leaf-SYSTEM] Offline startup. revealing editor UI.");
                    is_auth_init.set(true);
                    // Web版: URLを /editor に更新
                    if !crate::js_interop::is_tauri() {
                        if let Some(win) = web_sys::window() {
                            if let Ok(hist) = win.history() {
                                let path = win.location().pathname().unwrap_or_default();
                                if path == "/login" {
                                    let _ = hist.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/"));
                                }
                            }
                        }
                    }
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
        let et_auth = editor_theme.clone();

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
                    // Web版: URLを /editor に更新
                    if !crate::js_interop::is_tauri() {
                        if let Some(win) = web_sys::window() {
                            if let Ok(hist) = win.history() {
                                let path = win.location().pathname().unwrap_or_default();
                                if path == "/login" {
                                    let _ = hist.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/"));
                                }
                            }
                        }
                    }
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
                    let et_a = et_auth.clone();
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
                                            total_size: s.total_size, loaded_bytes: s.loaded_bytes, needs_bom: s.needs_bom, is_preview: s.is_preview, is_split: false, editor_state: None, preview_scroll_top: 0.0,
                                            created_at: s.created_at,
                                local_path: None,
                            
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
                                let ns = Sheet { id: nid.clone(), guid: None, category: "".to_string(), title: "Untitled 1.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: Some(js_sys::Date::now() as u64), local_path: None };
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
                            let theme_val = get_account_storage(EDITOR_THEME_KEY).unwrap_or_else(|| "gruvbox".to_string());
                            crate::js_interop::set_editor_theme(&theme_val);
                            et_a.set(theme_val);
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
        let local_auto_save_ref_cb = local_auto_save_ref.clone();
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
                let las_ref_i = local_auto_save_ref_cb.clone();
                let callback = Closure::wrap(Box::new(move |cmd: String| {
                    if cmd == "save" { os_i.emit((true, None)); }
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
                        if needs_save { os_cb.emit((false, None)); }
                        sp.set(true); let delay = if needs_save { 100 } else { 0 };
                        Timeout::new(delay, move || {
                            clear_local_handle();
                            let nid = js_sys::Date::now().to_string();
                            let ns = Sheet { id: nid.clone(), guid: None, category: "__LOCAL__".to_string(), title: "Untitled.txt".to_string(), content: "".to_string(), is_modified: false, drive_id: None, temp_content: None, temp_timestamp: None, last_sync_timestamp: None, tab_color: generate_random_color(), total_size: 0, loaded_bytes: 0, needs_bom: true, is_preview: false, is_split: false, editor_state: None, preview_scroll_top: 0.0, created_at: Some(js_sys::Date::now() as u64), local_path: None };
                            load_editor_content(""); set_gutter_status("local");
                            let mut current_sheets = (*rs.borrow()).clone(); current_sheets.push(ns.clone());
                            *rs.borrow_mut() = current_sheets.clone(); s.set(current_sheets); aid_ref.borrow_mut().replace(nid.clone()); aid_state.set(Some(nid.clone()));
                            focus_editor(); let spr = sp.clone(); Timeout::new(500, move || { spr.set(false); }).forget();
                            let os_final = os_cb.clone();
                            spawn_local(async move { let js = ns.to_js(); let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } os_final.emit((true, None)); });
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
                        gloo::console::log!(format!("[Leaf-DBG] CHANGE aid={:?} cur_c.len={} first20={:?}", aid, cur_c.len(), cur_c.chars().take(20).collect::<String>()));
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
                                    gloo::console::log!(format!("[Leaf-DBG] CHANGE update sheet.id={} OLD.first20={:?} NEW.first20={:?}", sheet.id, sheet.content.chars().take(20).collect::<String>(), cur_c.chars().take(20).collect::<String>()));
                                    let now = js_sys::Date::now() as u64;
                                    sheet.content = cur_c.clone();
                                    sheet.is_modified = true;
                                    sheet.temp_content = Some(cur_c.clone());
                                    sheet.temp_timestamp = Some(now);
                                    needs_upd = true; 
                                    let js = JSSheet { id: sheet.id.clone(), guid: sheet.guid.clone(), category: sheet.category.clone(), title: sheet.title.clone(), content: sheet.content.clone(), is_modified: true, drive_id: sheet.drive_id.clone(), temp_content: Some(cur_c.clone()), temp_timestamp: Some(now), last_sync_timestamp: sheet.last_sync_timestamp, tab_color: sheet.tab_color.clone(), total_size: sheet.total_size, loaded_bytes: sheet.loaded_bytes, needs_bom: sheet.needs_bom, is_preview: sheet.is_preview, created_at: sheet.created_at };
                                    spawn_local(async move { let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } });
                                }
                                trigger_drive_sync = (sheet.category != "__LOCAL__" && !sheet.category.is_empty()) || (sheet.category != "__LOCAL__" && sheet.category.is_empty() && !sheet.title.starts_with("Untitled.txt")) || (sheet.category == "__LOCAL__" && *las_ref_i.borrow());
                            }
                            if needs_upd { *r_s_i.borrow_mut() = cur_s.clone(); s_state.set(cur_s); }
                            // 3秒間変更がなければファイル保存。captured id (= 編集時の aid) を渡し、
                            // タイマー発火時に aid が変わっていても元のシートを保存する。
                            if trigger_drive_sync && needs_upd {
                                let osa = os_i.clone();
                                let id_for_timer = id.clone();
                                timer.set(Some(Timeout::new(3000, move || { osa.emit((false, Some(id_for_timer))); })));
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

                        // シート別セッションに切替（既存なら content/undo を保持して再アクティブ化）
                        crate::js_interop::activate_sheet_session(id, &s.content, &s.title);
                        let mode = if s.category == "__LOCAL__" { "local" } else if s.category.is_empty() { if s.title.starts_with("Untitled.txt") { "unsaved" } else { "local" } } else if s.drive_id.is_none() && s.guid.is_none() { "unsaved" } else { "none" };
                        set_gutter_status(mode); focus_editor();
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
        let to = tab_order_ref.clone();
        let atid = active_terminal_id.clone();
        let atref = active_terminal_ref.clone();
        let tsh_save_close = TerminalSplitHandles {
            ts_state: terminal_split_enabled.clone(),
            ts_ref: terminal_split_ref.clone(),
            tse_state: terminal_split_edit_mode.clone(),
            tse_ref: terminal_split_edit_ref.clone(),
            sps_state: split_pane_sheet_id.clone(),
            sps_ref: split_pane_sheet_id_ref.clone(),
            skip_fade: skip_split_fade.clone(),
            map: terminal_split_map.clone(),
        };
        use_effect_with(((*saving).clone(), (*psc).clone()), move |deps| {
            let (saving_val, psc_val) = deps;
            if saving_val.is_none() {
                if let Some(close_id) = psc_val.clone() {
                    psc.set(None);
                    close_tab_direct(close_id, rs.clone(), s_state.clone(), aid.clone(), sp.clone(), ncid.clone(), Some(aid_ref.clone()), to.borrow().clone(), Some(atid.clone()), Some(atref.clone()), Some(tsh_save_close.clone()));
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
        let atref_obscured = active_terminal_ref.clone();
        use_effect_with( ((is_auth, is_ld, is_file_open, is_prev, is_help, is_logout_conf), (has_del, has_conf, has_nc, has_fall, is_imp_lock, is_drop)), move |deps| {
                let ((auth, ld, file_open, prev, help, logout_conf), (del, conf, nc, fall, imp_lock, drop_open)) = deps;
                let obscured = !**auth || **ld || **file_open || **prev || **help || **logout_conf || (*del).is_some() || !(*conf).is_empty() || !(*nc).is_empty() || !(*fall).is_empty() || **imp_lock || **drop_open;
                if *last_obscured && !obscured {
                    if let Some(ref tid) = *atref_obscured.borrow() {
                        crate::js_interop::terminal_focus(tid);
                    } else {
                        focus_editor();
                    }
                }
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
                let preview_overlay_opacity_ev = preview_overlay_opacity.clone();
                let sheets_ref_ev = sheets_ref.clone();
                let active_id_ref_ev = active_id_ref.clone();
                let saving_id_ref_ev = saving_id_ref.clone();
                let ncid_ev = no_category_folder_id.clone();
                let pending_close_tab_ev = pending_close_tab.clone();
                let terminal_ids_ref_ev = terminal_ids_ref.clone();
                let active_terminal_id_ev = active_terminal_id.clone();
                let active_terminal_ref_ev = active_terminal_ref.clone();
                let tab_order_ref_ev = tab_order_ref.clone();
                let terminal_counter_ev = terminal_counter.clone();
                let terminal_tab_ids_ev = terminal_tab_ids.clone();
                let pfs_ev = preview_font_size.clone();
                let pending_close_unsynced_tab_ev = pending_close_unsynced_tab.clone();
                let nc_ev = network_connected.clone();
                let pending_save_close_tab_ev = pending_save_close_tab.clone();
                let is_tab_select_ev = is_tab_select_dialog_visible.clone();
                let is_split_close_ev = is_split_close_dialog_visible.clone();
                let split_pane_sheet_id_ev = split_pane_sheet_id.clone();
                let split_pane_sheet_id_ref_ev = split_pane_sheet_id_ref.clone();
                let split_content_opacity_ev = split_content_opacity.clone();
                let terminal_split_ev = terminal_split_enabled.clone();
                let terminal_split_ref_ev = terminal_split_ref.clone();
                let terminal_split_map_ev = terminal_split_map.clone();
                let skip_split_fade_ev = skip_split_fade.clone();
                let terminal_split_edit_ev = terminal_split_edit_mode.clone();
                let terminal_split_edit_ref_ev = terminal_split_edit_ref.clone();
                let tci_ev = tab_closing_id.clone();
                let tfs_ev = terminal_font_size.clone();
                let tfs_ref_ev = terminal_font_size_ref.clone();
                let is_guest_mode_ev = is_guest_mode.clone();
                let is_char_code_ev = is_char_code_visible.clone();
                let char_code_char_ev = char_code_char.clone();
                let is_sheet_info_ev = is_sheet_info_visible.clone();
                let local_auto_save_ref_ev = local_auto_save_ref.clone();
                use_effect_with((*is_auth, (*is_file_open, *is_preview, *is_help, *is_logout_conf, *is_imp_lock, *is_drop_ev, *is_fd_sub, *is_creating_cat_ev, *is_ld_ev, *is_fo_ev, *is_tab_select_ev, *is_split_close_ev), ((*pending_del).is_some(), !(*conflicts).is_empty(), !(*fallbacks).is_empty(), !(*ncq_esc).is_empty(), *is_settings_ev)), move |deps| {
                    let (auth, (file_open, _preview, help, logout_conf, imp_lock, drop_open, fd_sub, is_creating_cat, is_loading, is_fading_out, is_tab_select, is_split_close_dialog), (has_del, has_conf, has_fall, has_nc, settings_open)) = *deps;
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
                    let las_c = local_auto_save_ref_ev.clone();
                    let file_close_trigger_c = file_close_trigger_ev.clone();
                    let _close_preview_c = close_preview_ev.clone();
                    let preview_overlay_opacity_c = preview_overlay_opacity_ev.clone();
                    let rs_c = sheets_ref_ev.clone();
                    let aid_ref_c = active_id_ref_ev.clone();
                    let saving_ref_c = saving_id_ref_ev.clone();
                    let ncid_c = ncid_ev.clone();
                    let pending_close_tab_c = pending_close_tab_ev.clone();
                    let term_ids_ref_c = terminal_ids_ref_ev.clone();
                    let atid_c = active_terminal_id_ev.clone();
                    let atref_c = active_terminal_ref_ev.clone();
                    let tab_order_ref_c = tab_order_ref_ev.clone();
                    let term_counter_c = terminal_counter_ev.clone();
                    let term_tab_ids_c = terminal_tab_ids_ev.clone();
                    let tci_c = tci_ev.clone();
                    let pfs_c = pfs_ev.clone();
                    let pending_close_unsynced_c = pending_close_unsynced_tab_ev.clone();
                    let nc_c = nc_ev.clone();
                    let pending_save_close_c = pending_save_close_tab_ev.clone();
                    let is_tab_select_c = is_tab_select_ev.clone();
                    let is_split_close_c = is_split_close_ev.clone();
                    let split_pane_sheet_id_c = split_pane_sheet_id_ev.clone();
                    let split_pane_sheet_id_ref_c = split_pane_sheet_id_ref_ev.clone();
                    let terminal_split_c = terminal_split_ev.clone();
                    let terminal_split_ref_c = terminal_split_ref_ev.clone();
                    let ts_map_c = terminal_split_map_ev.clone();
                    let ssf_c = skip_split_fade_ev.clone();
                    let terminal_split_edit_c = terminal_split_edit_ev.clone();
                    let terminal_split_edit_ref_c = terminal_split_edit_ref_ev.clone();
                    let tfs_c = tfs_ev.clone();
                    let tfs_ref_c = tfs_ref_ev.clone();
                    let split_content_opacity_c = split_content_opacity_ev.clone();
                    let is_guest_c = is_guest_mode_ev.clone();
                    let is_char_code_c = is_char_code_ev.clone();
                    let char_code_char_c = char_code_char_ev.clone();
                    let is_sheet_info_c = is_sheet_info_ev.clone();
                    let mut opts = EventListenerOptions::run_in_capture_phase(); opts.passive = false;
                    let listener = EventListener::new_with_options(&window, "keydown", opts, move |e| {
                        let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                        let key = ke.key(); let code = ke.code();
                        // Cmd/Ctrl 併用時はアプリショートカットとして扱わない（DevTools の Cmd+Opt+I 等を通す）
                        let modifier_active = ke.alt_key() && !ke.meta_key() && !ke.ctrl_key();
                        let is_dialog_open = file_open || help || has_del || has_conf || has_fall || logout_conf || has_nc || drop_open || is_loading || is_fading_out || is_creating_cat || settings_open || is_tab_select || is_split_close_dialog;
                        let is_overlay_active = is_dialog_open || imp_lock;
                        let key_lower = key.to_lowercase();
                        let is_l_key = code == "KeyL" || key_lower == "l" || key_lower == "¬";
                        let is_h_key = code == "KeyH" || key_lower == "h" || key_lower == "˙";
                        let is_m_key = code == "KeyM" || key_lower == "m" || key_lower == "µ";
                        let is_e_key = code == "KeyE" || key_lower == "e" || key_lower == "´";
                        let is_c_key = code == "KeyC" || key_lower == "c" || key == "©";
                        let is_i_key = code == "KeyI" || key_lower == "i" || key_lower == "ˆ";
                        let is_plus_key = code == "Equal" || key == "=" || key == "+" || key == "≠";
                        let is_minus_key = code == "Minus" || key == "-" || key == "–";
                        let is_toggle_shortcut = modifier_active && (is_l_key || is_h_key || is_m_key);
                        let is_font_size_shortcut = modifier_active && (is_plus_key || is_minus_key);
                        // Modifier+アプリショートカットキーはブラウザデフォルト動作を先にブロック
                        // 注意: Cmd+N, Cmd+W はブラウザが最優先で処理するため preventDefault では防げない
                        if modifier_active {
                            let is_app_key = is_l_key || is_h_key || is_m_key || is_plus_key || is_minus_key
                                || code == "KeyN" || code == "KeyS" || code == "KeyO" || code == "KeyF" || code == "KeyW"
                                || code == "BracketLeft" || code == "BracketRight"
                                || code == "Comma" || code == "KeyT" || code == "KeyE" || code == "KeyI";
                            if is_app_key { e.prevent_default(); e.stop_immediate_propagation(); }
                        }
                        if is_loading || is_fading_out { e.prevent_default(); e.stop_immediate_propagation(); return; }
                        
                        // Alt + L
                        // [シート] 編集 ↔ フル画面プレビュー のトグル（スプリット状態は変えない）
                        // [ターミナル] フル画面ターミナル ↔ スプリットプレビュー のトグル（フル画面時はタブ選択ダイアログ経由）
                        // ターミナルスプリットの判定: split_pane_sheet_id が Some = ターミナルから開いたスプリット
                        if modifier_active && is_l_key && !is_overlay_active {
                            e.prevent_default(); e.stop_immediate_propagation();
                            let split_open = *terminal_split_ref_c.borrow();
                            let is_terminal_ctx = atref_c.borrow().is_some()
                                || (split_open && (*split_pane_sheet_id_c).is_some());
                            if is_terminal_ctx {
                                if split_open {
                                    // ターミナルスプリット中 → 閉じる
                                    *terminal_split_ref_c.borrow_mut() = false;
                                    terminal_split_c.set(false);
                                    split_pane_sheet_id_c.set(None);
                                    *split_pane_sheet_id_ref_c.borrow_mut() = None;
                                    // ts_map のエントリも更新してシートのロック解除
                                    if let Some(tid) = atref_c.borrow().as_ref().cloned() {
                                        ts_map_c.borrow_mut().insert(tid, (false, false, None));
                                    }
                                } else {
                                    // スプリット未表示 → タブ選択ダイアログ
                                    if aid_ref_c.borrow().is_some() {
                                        is_tab_select_c.set(true);
                                    }
                                }
                            } else {
                                // シートがアクティブ: フル画面プレビュー切り替え
                                let new_val = !*is_preview_c;
                                if new_val {
                                    is_preview_c.set(true);
                                    preview_overlay_opacity_c.set(false);
                                    let op_c = preview_overlay_opacity_c.clone();
                                    gloo::timers::callback::Timeout::new(10, move || { op_c.set(true); }).forget();
                                } else {
                                    preview_overlay_opacity_c.set(false);
                                    let iv_c = is_preview_c.clone();
                                    gloo::timers::callback::Timeout::new(300, move || { iv_c.set(false); focus_editor(); }).forget();
                                }
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
                            }
                            return;
                        }

                        // Alt + E
                        // [シート] フル画面編集 ↔ スプリット（左:編集、右:プレビュー） のトグル
                        // [ターミナル] スプリット中のみ、プレビュー ↔ Ace編集モード のトグル
                        // ターミナルがスプリット中はシートのプレビュー状態に関わらず通す
                        let terminal_split_active = atref_c.borrow().is_some() && *terminal_split_ref_c.borrow();
                        if modifier_active && is_e_key && !is_overlay_active && (!_preview || terminal_split_active) {
                            e.prevent_default(); e.stop_immediate_propagation();
                            let split_open = *terminal_split_ref_c.borrow();
                            if atref_c.borrow().is_some() {
                                // ターミナルがアクティブ: スプリット中のみAce編集モードをトグル（300msフェード）
                                if split_open {
                                    let new_val = !*terminal_split_edit_ref_c.borrow();
                                    *terminal_split_edit_ref_c.borrow_mut() = new_val;
                                    // 編集モード終了時の保存は destroy_split_editor 経由の
                                    // split-editor-changed { final: true } ディスパッチに委譲する。
                                    // ここで get_split_editor_content を呼ぶと _splitEditor 未初期化の
                                    // タイミング（連打など）で空文字列を誤保存してしまうため。
                                    // フェードアウト → モード切替 → フェードイン
                                    split_content_opacity_c.set(false);
                                    let tse_c2 = terminal_split_edit_c.clone();
                                    let atref_c2 = atref_c.clone();
                                    let sco_c = split_content_opacity_c.clone();
                                    gloo::timers::callback::Timeout::new(300, move || {
                                        tse_c2.set(new_val);
                                        if !new_val {
                                            if let Some(tid) = atref_c2.borrow().as_ref().cloned() {
                                                crate::js_interop::terminal_focus(&tid);
                                            }
                                        }
                                        let sco_c2 = sco_c.clone();
                                        gloo::timers::callback::Timeout::new(10, move || {
                                            sco_c2.set(true);
                                        }).forget();
                                    }).forget();
                                }
                                // スプリット未表示時は何もしない
                            } else {
                                // シートがアクティブ: スプリットプレビューのトグル（Web/Desktop共通）
                                if split_open {
                                    *terminal_split_ref_c.borrow_mut() = false;
                                    terminal_split_c.set(false);
                                    split_pane_sheet_id_c.set(None);
                                    *split_pane_sheet_id_ref_c.borrow_mut() = None;
                                } else if aid_ref_c.borrow().is_some() {
                                    split_pane_sheet_id_c.set(None);
                                    *split_pane_sheet_id_ref_c.borrow_mut() = None;
                                    *terminal_split_ref_c.borrow_mut() = true;
                                    terminal_split_c.set(true);
                                }
                            }
                            return;
                        }

                        // Alt + [ / ] : タブ切り替え（全画面プレビューモード含む全モードで有効）
                        if modifier_active && (code == "BracketLeft" || code == "BracketRight") && !is_overlay_active {
                            e.prevent_default(); e.stop_immediate_propagation();
                            let is_bracket_left = code == "BracketLeft";
                            let current_sheets = (*rs_c.borrow()).clone();
                            let all_tab_ids: Vec<String> = tab_order_ref_c.borrow().clone();
                            if all_tab_ids.len() <= 1 { return; }
                            let current_id = if let Some(ref tid) = *atref_c.borrow() {
                                tid.clone()
                            } else if let Some(ref sid) = *aid_ref_c.borrow() {
                                sid.clone()
                            } else { return; };
                            if let Some(cur_idx) = all_tab_ids.iter().position(|id| *id == current_id) {
                                // ロックされたシートをスキップしながら次のタブを探す
                                let len = all_tab_ids.len();
                                let mut new_idx = cur_idx;
                                let ts_map_snap = ts_map_c.borrow().clone();
                                for _ in 0..len {
                                    new_idx = if is_bracket_left {
                                        if new_idx == 0 { len - 1 } else { new_idx - 1 }
                                    } else {
                                        if new_idx == len - 1 { 0 } else { new_idx + 1 }
                                    };
                                    let candidate = &all_tab_ids[new_idx];
                                    // ターミナルタブ、または非ロックのシートタブなら採用
                                    if candidate.starts_with("__TERM__")
                                        || !is_sheet_locked_by_terminal(candidate, &ts_map_snap) {
                                        break;
                                    }
                                }
                                let new_id = all_tab_ids[new_idx].clone();
                                if new_id == current_id { return; }
                                if new_id.starts_with("__TERM__") {
                                    // 現在のタブのスプリット状態を保存
                                    if current_id.starts_with("__TERM__") {
                                        ts_map_c.borrow_mut().insert(current_id.clone(), (*terminal_split_ref_c.borrow(), *terminal_split_edit_ref_c.borrow(), (*split_pane_sheet_id_ref_c.borrow()).clone()));
                                    } else {
                                        let mut us = current_sheets.clone();
                                        if let Some(sheet) = us.iter_mut().find(|x| x.id == current_id) {
                                            sheet.is_split = *terminal_split_ref_c.borrow();
                                        }
                                        *rs_c.borrow_mut() = us;
                                    }
                                    // ターミナルのスプリット状態・編集モード・ペインシートIDを復元（フェードなし）
                                    let (terminal_split, terminal_edit, pane_sheet) = ts_map_c.borrow().get(&new_id).cloned().unwrap_or((false, false, None));
                                    *ssf_c.borrow_mut() = true;
                                    terminal_split_c.set(terminal_split);
                                    *terminal_split_ref_c.borrow_mut() = terminal_split;
                                    *terminal_split_edit_ref_c.borrow_mut() = terminal_edit;
                                    terminal_split_edit_c.set(terminal_edit);
                                    split_pane_sheet_id_c.set(pane_sheet.clone());
                                    *split_pane_sheet_id_ref_c.borrow_mut() = pane_sheet;
                                    atid_c.set(Some(new_id));
                                } else {
                                    let was_on_terminal = current_id.starts_with("__TERM__");
                                    if was_on_terminal {
                                        ts_map_c.borrow_mut().insert(current_id.clone(), (*terminal_split_ref_c.borrow(), *terminal_split_edit_ref_c.borrow(), (*split_pane_sheet_id_ref_c.borrow()).clone()));
                                        // シートに戻る際は編集モードとペインシートIDをリセット
                                        *terminal_split_edit_ref_c.borrow_mut() = false;
                                        terminal_split_edit_c.set(false);
                                        split_pane_sheet_id_c.set(None);
                                        *split_pane_sheet_id_ref_c.borrow_mut() = None;
                                    }
                                    atid_c.set(None);
                                    sp_c.set(true);
                                    if !was_on_terminal {
                                        // Undo履歴を保存
                                        crate::js_interop::save_undo_state(&current_id);
                                        let editor_state = crate::js_interop::get_editor_state();
                                        let preview_scroll = web_sys::window()
                                            .and_then(|w| w.document())
                                            .and_then(|d| {
                                                d.query_selector(".absolute.inset-0.overflow-y-auto").ok().flatten()
                                                    .or_else(|| d.get_element_by_id("split-preview-scroll"))
                                            })
                                            .map(|el| el.scroll_top() as f64)
                                            .unwrap_or(0.0);
                                        let cur_c_val = get_editor_content();
                                        if let Some(cur_c) = cur_c_val.as_string() {
                                            let mut us = current_sheets.clone();
                                            let mut should_drive_save = false;
                                            if let Some(sheet) = us.iter_mut().find(|x| x.id == current_id) {
                                                sheet.editor_state = Some(editor_state);
                                                sheet.preview_scroll_top = preview_scroll;
                                                sheet.is_split = *terminal_split_ref_c.borrow(); // スプリット状態を保存
                                                if sheet.content != cur_c {
                                                    sheet.content = cur_c.clone();
                                                    if sheet.drive_id.is_some() || sheet.guid.is_some() { sheet.is_modified = true; }
                                                }
                                                if sheet.is_modified {
                                                    sheet.temp_content = Some(sheet.content.clone());
                                                    sheet.temp_timestamp = Some(js_sys::Date::now() as u64);
                                                    let js = sheet.to_js();
                                                    let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                                    if let Ok(v) = js.serialize(&ser) { spawn_local(async move { let _ = save_sheet(v).await; }); }
                                                    let is_drive = (sheet.category != "__LOCAL__" && !sheet.category.is_empty())
                                                        || (sheet.category != "__LOCAL__" && sheet.category.is_empty() && !sheet.title.starts_with("Untitled.txt"))
                                                        || (sheet.category == "__LOCAL__" && *las_c.borrow());
                                                    if is_drive {
                                                        should_drive_save = true;
                                                    }
                                                }
                                            }
                                            *rs_c.borrow_mut() = us.clone();
                                            sheets_c.set(us);
                                            // aid 切替前に Drive 保存を発火（override_id で current_id を明示指定）
                                            if should_drive_save {
                                                os_c.emit((false, Some(current_id.clone())));
                                            }
                                        }
                                    }
                                    let sheets_list = (*rs_c.borrow()).clone();
                                    if let Some(sheet) = sheets_list.iter().find(|s| s.id == new_id) {
                                        crate::js_interop::activate_sheet_session(&new_id, &sheet.content, &sheet.title);
                                        if let Some(ref state) = sheet.editor_state {
                                            crate::js_interop::set_editor_state(state);
                                        }
                                        if sheet.drive_id.is_none() && sheet.guid.is_none() {
                                            if sheet.category == "__LOCAL__" { set_gutter_status("local"); } else { set_gutter_status("unsaved"); }
                                        } else if sheet.is_modified { set_gutter_status("unsaved"); } else { set_gutter_status("none"); }
                                        is_preview_c.set(sheet.is_preview);
                                        // シートのスプリット状態を復元（フェードなし）
                                        *ssf_c.borrow_mut() = true;
                                        terminal_split_c.set(sheet.is_split);
                                        *terminal_split_ref_c.borrow_mut() = sheet.is_split;
                                    }
                                    aid_c.set(Some(new_id.clone()));
                                    *aid_ref_c.borrow_mut() = Some(new_id);
                                    let sp_inner = sp_c.clone();
                                    Timeout::new(100, move || { sp_inner.set(false); focus_editor(); }).forget();
                                }
                            }
                            return;
                        }

                        // Markdownモード中のキー操作（全画面プレビューのみ・分割モード・ターミナルアクティブ時は除く）
                        if _preview && !is_overlay_active && atref_c.borrow().is_none() {
                            // ESCで編集モードに戻る
                            if key == "Escape" {
                                e.prevent_default(); e.stop_immediate_propagation();
                                // フェードアウト
                                preview_overlay_opacity_c.set(false);
                                let iv_esc = is_preview_c.clone();
                                gloo::timers::callback::Timeout::new(300, move || { iv_esc.set(false); focus_editor(); }).forget();
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
                            // Alt+許可ショートカットのみ通す（L,H,M,[,],W,,,フォントサイズ）
                            if modifier_active {
                                let is_allowed = is_l_key || is_h_key || is_m_key
                                    || code == "BracketLeft" || code == "BracketRight"
                                    || code == "KeyW" || code == "Comma"
                                    || code == "KeyN" || code == "KeyT";
                                if is_allowed {
                                    /* fall through to normal shortcut handling */
                                } else {
                                    e.prevent_default(); e.stop_immediate_propagation();
                                    return;
                                }
                            }
                            // スクロール操作
                            else if is_up || is_down || is_arrow_up || is_arrow_down || is_home || is_end || is_space {
                                e.prevent_default(); e.stop_immediate_propagation();
                                if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                    if let Ok(Some(el)) = doc.query_selector(".absolute.inset-0.overflow-y-auto") {
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
                            }
                            // その他すべてのキー入力をブロック（エディタに渡さない）
                            else {
                                e.prevent_default(); e.stop_immediate_propagation();
                                return;
                            }
                        }

                        // Alt + T (ターミナル) Tauri版のみ
                        if modifier_active && code == "KeyT" && !is_overlay_active {
                            if crate::js_interop::is_tauri() {
                                e.prevent_default(); e.stop_immediate_propagation();
                                let mut c = term_counter_c.borrow_mut();
                                *c += 1;
                                let tid = format!("__TERM__{}", *c);
                                term_ids_ref_c.borrow_mut().push(tid.clone());
                                term_tab_ids_c.set(term_ids_ref_c.borrow().clone());
                                // アクティブがシートの場合、シートタブの左（同じ位置）に挿入
                                let is_terminal_active_now = atref_c.borrow().is_some();
                                if !is_terminal_active_now {
                                    if let Some(aid) = (*aid_ref_c.borrow()).clone() {
                                        let mut order = tab_order_ref_c.borrow_mut();
                                        if let Some(pos) = order.iter().position(|x| x == &aid) {
                                            order.insert(pos, tid.clone());
                                        } else {
                                            order.push(tid.clone());
                                        }
                                    } else {
                                        tab_order_ref_c.borrow_mut().push(tid.clone());
                                    }
                                } else {
                                    tab_order_ref_c.borrow_mut().push(tid.clone());
                                }
                                // 新規ターミナルは非スプリット状態で開く（フェードなし）
                                *ssf_c.borrow_mut() = true;
                                terminal_split_c.set(false);
                                *terminal_split_ref_c.borrow_mut() = false;
                                atid_c.set(Some(tid.clone()));
                                *atref_c.borrow_mut() = Some(tid);
                                return;
                            }
                        }

                        // Alt + , (設定) のトグル
                        if modifier_active && (code == "Comma" || key == ",") && (!is_overlay_active || settings_open) {
                            e.prevent_default(); e.stop_immediate_propagation();
                            let new_val = !settings_open;
                            is_settings_c.set(new_val);
                            if !new_val {
                                if let Some(ref tid) = *atref_c.borrow() {
                                    crate::js_interop::terminal_focus(tid);
                                } else {
                                    focus_editor();
                                }
                            }
                            return;
                        }

                        // Alt + M (FileOpen) のトグル
                        if modifier_active && is_m_key && (!is_overlay_active || *is_file_open_c) && !*is_guest_c {
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
                            if !val {
                                if let Some(ref tid) = *atref_c.borrow() {
                                    crate::js_interop::terminal_focus(tid);
                                } else {
                                    focus_editor();
                                }
                            }
                            return;
                        }

                        // Alt + I: シート情報ダイアログ（シート表示中またはターミナルスプリット中）
                        let sheet_info_allowed = atref_c.borrow().is_none()
                            || (*terminal_split_ref_c.borrow() && (*split_pane_sheet_id_c).is_some());
                        if modifier_active && is_i_key && !is_overlay_active && sheet_info_allowed {
                            e.prevent_default(); e.stop_immediate_propagation();
                            is_sheet_info_c.set(true);
                            return;
                        }

                        // Alt + C: カーソル位置の文字コードダイアログ
                        // シート表示中、またはターミナルスプリットの右ペイン編集中に使用可能
                        let terminal_split_edit_active =
                            atref_c.borrow().is_some() && *terminal_split_edit_ref_c.borrow();
                        if modifier_active && is_c_key && !is_overlay_active
                            && (atref_c.borrow().is_none() || terminal_split_edit_active)
                        {
                            e.prevent_default(); e.stop_immediate_propagation();
                            let ch = if terminal_split_edit_active {
                                crate::js_interop::get_char_at_split_editor_cursor()
                            } else {
                                crate::js_interop::get_char_at_cursor()
                            };
                            if !ch.is_empty() {
                                char_code_char_c.set(ch);
                                is_char_code_c.set(true);
                            }
                            return;
                        }

                        // ターミナルアクティブ時: Alt+=/- でターミナルフォントサイズ変更
                        if modifier_active && is_font_size_shortcut && !is_overlay_active && atref_c.borrow().is_some() {
                            e.prevent_default(); e.stop_immediate_propagation();
                            let delta = if is_plus_key { 1 } else { -1 };
                            let current = *tfs_ref_c.borrow();
                            let new_size = crate::js_interop::terminal_set_font_size(current + delta);
                            *tfs_ref_c.borrow_mut() = new_size;
                            tfs_c.set(new_size);
                            set_account_storage(TERMINAL_FONT_SIZE_KEY, &new_size.to_string());
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
                                            os_c.emit((false, Some(aid.clone()))); // 切替元シートを明示指定して保存
                                        }
                                    }
                                }
                                crate::js_interop::exec_editor_command("newSheet");
                                return;
                            }
                            // Alt + W : タブを閉じる
                            let is_w = code == "KeyW";
                            if is_w {
                                e.prevent_default(); e.stop_immediate_propagation();
                                // ターミナルタブがアクティブかつスプリット中はどちらを閉じるか選択ダイアログを表示
                                if *terminal_split_ref_c.borrow() && atref_c.borrow().is_some() {
                                    is_split_close_c.set(true);
                                    return;
                                }
                                // 非スプリットのターミナルがアクティブな場合は何もしない
                                // （ターミナルを閉じるにはタブの×ボタンを使う）
                                if atref_c.borrow().is_some() {
                                    return;
                                }
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
                                    // フェードアウト後に閉じる
                                    tci_c.set(Some(close_id.clone()));
                                    let tci2 = tci_c.clone();
                                    let rs2 = rs_c.clone(); let sc2 = sheets_c.clone(); let ac2 = aid_c.clone(); let sp2 = sp_c.clone(); let nc2 = ncid_c.clone(); let ar2 = aid_ref_c.clone();
                                    let to2 = tab_order_ref_c.clone(); let atid2 = atid_c.clone(); let atref2 = atref_c.clone();
                                    let tsh_close2 = TerminalSplitHandles {
                                        ts_state: terminal_split_c.clone(),
                                        ts_ref: terminal_split_ref_c.clone(),
                                        tse_state: terminal_split_edit_c.clone(),
                                        tse_ref: terminal_split_edit_ref_c.clone(),
                                        sps_state: split_pane_sheet_id_c.clone(),
                                        sps_ref: split_pane_sheet_id_ref_c.clone(),
                                        skip_fade: ssf_c.clone(),
                                        map: ts_map_c.clone(),
                                    };
                                    Timeout::new(300, move || {
                                        tci2.set(None);
                                        close_tab_direct(close_id, rs2, sc2, ac2, sp2, nc2, Some(ar2), to2.borrow().clone(), Some(atid2.clone()), Some(atref2.clone()), Some(tsh_close2));
                                    }).forget();
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
                                // ただし設定ダイアログが開いている場合はESCで閉じる（スライダーのフォーカスは無視）
                                let esc_target = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok());
                                let is_input_focused = esc_target.as_ref().map(|t| { let tag = t.tag_name().to_lowercase(); tag == "input" || tag == "textarea" }).unwrap_or(false);
                                if is_input_focused && !settings_open { return; }
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
                                else if is_split_close_dialog { is_split_close_c.set(false); }
                                if let Some(ref tid) = *atref_c.borrow() {
                                    crate::js_interop::terminal_focus(tid);
                                } else {
                                    focus_editor();
                                }
                                return;
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
    let is_sub_overlay_active = *is_creating_category || (*pending_delete_category).is_some() || !(*conflict_queue).is_empty() || !(*fallback_queue).is_empty() || !(*name_conflict_queue).is_empty() || *is_logout_confirm_visible || *is_install_confirm_visible || *is_install_manual_visible || (*pending_close_tab).is_some() || (*pending_close_unsynced_tab).is_some() || (*pending_save_close_tab).is_some() || (*pending_empty_delete).is_some() || *is_split_close_dialog_visible;

    // --- Tab Bar ---
    // tab_order_stateの順序でtab_infosを構築（シート+ターミナル統合）
    // RefCellから直接読み、sheets/terminalsと即時同期して1フレーム遅延を防ぐ
    let tab_infos: Vec<TabInfo> = {
        let rs = sheets_ref.borrow();
        let active_id = active_sheet_id.as_ref();
        let editor_content = active_id.and_then(|_| get_editor_content().as_string());
        let term_ids_current = (*terminal_tab_ids).clone();
        let sheet_ids_current: Vec<String> = rs.iter().map(|s| s.id.clone()).collect();

        // tab_order_refをsheets/terminalsと即時同期
        {
            let mut order = tab_order_ref.borrow_mut();
            order.retain(|id| sheet_ids_current.contains(id) || term_ids_current.contains(id));
            for id in sheet_ids_current.iter().chain(term_ids_current.iter()) {
                if !order.contains(id) {
                    order.push(id.clone());
                }
            }
        }
        let order = tab_order_ref.borrow();

        let mut term_counter = 0u32;
        order.iter().filter_map(|id| {
            if id.starts_with("__TERM__") {
                term_counter += 1;
                Some(TabInfo {
                    id: id.clone(),
                    title: format!("{} {}", i18n::t("terminal", lang), term_counter),
                    is_modified: false,
                    tab_color: "".to_string(),
                })
            } else {
                rs.iter().find(|s| s.id == *id).map(|s| {
                    let content_for_display = if active_id == Some(&s.id) {
                        editor_content.as_deref().unwrap_or(&s.content)
                    } else {
                        &s.content
                    };
                    let is_local_file = s.category == "__LOCAL__";
                    let first_line = content_for_display.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
                    let is_unsaved_new = s.drive_id.is_none() && s.guid.is_none() && !is_local_file;
                    let display = if is_local_file {
                        s.title.clone()
                    } else if first_line.is_empty() || is_unsaved_new {
                        "---".to_string()
                    } else {
                        first_line
                    };
                    TabInfo {
                        id: s.id.clone(),
                        title: display,
                        is_modified: s.is_modified,
                        tab_color: s.tab_color.clone(),
                    }
                })
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
        let atid = active_terminal_id.clone();
        let ts = terminal_split_enabled.clone();
        let ts_ref = terminal_split_ref.clone();
        let ts_map = terminal_split_map.clone();
        let ssf = skip_split_fade.clone();
        let tse = terminal_split_edit_mode.clone();
        let tse_ref = terminal_split_edit_ref.clone();
        let sps_tab = split_pane_sheet_id.clone();
        let sps_tab_ref = split_pane_sheet_id_ref.clone();
        let atref_tab = active_terminal_ref.clone();
        let os_for_tab_select = on_save_cb.clone();
        let las_for_tab_select = local_auto_save_ref.clone();
        Callback::from(move |new_id: String| {
            // シートタブかつターミナルのスプリットペインに選択されている場合はクリックを無効化
            if !new_id.starts_with("__TERM__")
                && is_sheet_locked_by_terminal(&new_id, &ts_map.borrow()) {
                return;
            }
            // ターミナルタブ選択
            if new_id.starts_with("__TERM__") {
                // 現在がターミナルならそのスプリット状態を保存、シートならシートのis_splitを保存
                let prev_term = (*atref_tab.borrow()).clone();
                if let Some(ref prev_tid) = prev_term {
                    ts_map.borrow_mut().insert(prev_tid.clone(), (*ts_ref.borrow(), *tse_ref.borrow(), (*sps_tab_ref.borrow()).clone()));
                } else {
                    let current_sheet_id = (*aid_ref.borrow()).clone();
                    if let Some(sheet_id) = current_sheet_id {
                        let mut us = (*rs.borrow()).clone();
                        if let Some(sheet) = us.iter_mut().find(|x| x.id == sheet_id) {
                            sheet.is_split = *ts_ref.borrow();
                        }
                        *rs.borrow_mut() = us;
                    }
                }
                // ターミナルのスプリット状態・編集モード・ペインシートIDを復元（フェードなし）
                let (terminal_split, terminal_edit, pane_sheet) = ts_map.borrow().get(&new_id).cloned().unwrap_or((false, false, None));
                *ssf.borrow_mut() = true;
                ts.set(terminal_split);
                *ts_ref.borrow_mut() = terminal_split;
                *tse_ref.borrow_mut() = terminal_edit;
                tse.set(terminal_edit);
                sps_tab.set(pane_sheet.clone());
                *sps_tab_ref.borrow_mut() = pane_sheet;
                atid.set(Some(new_id.clone()));
                *atref_tab.borrow_mut() = Some(new_id);
                return;
            }
            // シートタブ選択
            let was_on_terminal = (*atref_tab.borrow()).is_some();
            let leaving_terminal_id = (*atref_tab.borrow()).clone();
            atid.set(None);
            *atref_tab.borrow_mut() = None;
            // ターミナルから来た場合: スプリット状態・編集モード・ペインシートIDを保存し、シートへはリセット
            if let Some(ref tid) = leaving_terminal_id {
                ts_map.borrow_mut().insert(tid.clone(), (*ts_ref.borrow(), *tse_ref.borrow(), (*sps_tab_ref.borrow()).clone()));
                *tse_ref.borrow_mut() = false;
                tse.set(false);
                sps_tab.set(None);
                *sps_tab_ref.borrow_mut() = None;
            }
            // RefCellから最新のactive_idを取得
            let current_aid = (*aid_ref.borrow()).clone();
            if current_aid.as_ref() == Some(&new_id) {
                // ターミナルから同じシートに戻る場合: スプリット状態を復元（フェードなし）
                if was_on_terminal {
                    let sheets_list = (*rs.borrow()).clone();
                    if let Some(sheet) = sheets_list.iter().find(|s| s.id == new_id) {
                        *ssf.borrow_mut() = true;
                        ts.set(sheet.is_split);
                        *ts_ref.borrow_mut() = sheet.is_split;
                    }
                }
                return;
            }
            // 現在のエディタ状態を保存し、変更があればIndexedDBに直接保存
            sp.set(true);
            if let Some(old_id) = current_aid {
                if was_on_terminal {
                    // ターミナルから来た場合: エディタ内容は変わっていないので保存不要
                } else {
                    // Undo履歴を保存
                    crate::js_interop::save_undo_state(&old_id);
                    let editor_state = crate::js_interop::get_editor_state();
                    let cur_c_val = get_editor_content();
                    if let Some(cur_c) = cur_c_val.as_string() {
                        // プレビュースクロール位置を保存（全画面・スプリット両対応）
                        let preview_scroll = web_sys::window()
                            .and_then(|w| w.document())
                            .and_then(|d| {
                                d.query_selector(".absolute.inset-0.overflow-y-auto").ok().flatten()
                                    .or_else(|| d.get_element_by_id("split-preview-scroll"))
                            })
                            .map(|el| el.scroll_top() as f64)
                            .unwrap_or(0.0);
                        let mut us = (*rs.borrow()).clone();
                        let mut should_drive_save = false;
                        if let Some(sheet) = us.iter_mut().find(|x| x.id == old_id) {
                            sheet.editor_state = Some(editor_state);
                            sheet.preview_scroll_top = preview_scroll;
                            sheet.is_split = *ts_ref.borrow(); // スプリット状態を保存
                            if sheet.content != cur_c {
                                sheet.content = cur_c.clone();
                                if sheet.drive_id.is_some() || sheet.guid.is_some() {
                                    sheet.is_modified = true;
                                }
                            }
                            // IndexedDBにtemp保存（on_save_cbを使わず直接保存）
                            if sheet.is_modified {
                                sheet.temp_content = Some(sheet.content.clone());
                                sheet.temp_timestamp = Some(js_sys::Date::now() as u64);
                                let js = sheet.to_js();
                                let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                if let Ok(v) = js.serialize(&ser) {
                                    spawn_local(async move { let _ = save_sheet(v).await; });
                                }
                                // タブ切り替え時の Drive/Local 保存トリガー条件
                                // （change handler の trigger_drive_sync と同じロジック）
                                let is_drive = (sheet.category != "__LOCAL__" && !sheet.category.is_empty())
                                    || (sheet.category != "__LOCAL__" && sheet.category.is_empty() && !sheet.title.starts_with("Untitled.txt"))
                                    || (sheet.category == "__LOCAL__" && *las_for_tab_select.borrow());
                                if is_drive {
                                    should_drive_save = true;
                                }
                            }
                        }
                        *rs.borrow_mut() = us.clone();
                        s_state.set(us);
                        // aid を切り替える前に Drive 保存を発火
                        // override_id で切替元シートを明示指定（on_save_cb は sheets_ref から content を読む）
                        if should_drive_save {
                            os_for_tab_select.emit((false, Some(old_id.clone())));
                        }
                    }
                }
            }
            // 新タブの内容をロード（シート別 EditSession に切替）
            let sheets_list = (*rs.borrow()).clone();
            if let Some(sheet) = sheets_list.iter().find(|s| s.id == new_id) {
                gloo::console::log!(format!("[Leaf-DBG] TAB_SELECT load new_id={} content.first20={:?}", new_id, sheet.content.chars().take(20).collect::<String>()));
                crate::js_interop::activate_sheet_session(&new_id, &sheet.content, &sheet.title);
                if let Some(ref state) = sheet.editor_state {
                    crate::js_interop::set_editor_state(state);
                }
                if sheet.drive_id.is_none() && sheet.guid.is_none() {
                    if sheet.category == "__LOCAL__" { set_gutter_status("local"); } else { set_gutter_status("unsaved"); }
                } else if sheet.is_modified {
                    set_gutter_status("unsaved");
                } else {
                    set_gutter_status("none");
                }
                // タブ毎の表示モードを復元（フェードなし）
                ip.set(sheet.is_preview);
                *ssf.borrow_mut() = true;
                ts.set(sheet.is_split);
                *ts_ref.borrow_mut() = sheet.is_split;
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
        let tids_ref_close = terminal_ids_ref.clone();
        let ttids_close = terminal_tab_ids.clone();
        let atid_close = active_terminal_id.clone();
        let atref_close = active_terminal_ref.clone();
        let aid = active_sheet_id.clone();
        let aid_ref = active_id_ref.clone();
        let tci_close = tab_closing_id.clone();
        let sp = is_suppressing_changes.clone();
        let ncid = no_category_folder_id.clone();
        let nc = network_connected.clone();
        let to_ref_close = tab_order_ref.clone();
        let tsh_close = TerminalSplitHandles {
            ts_state: terminal_split_enabled.clone(),
            ts_ref: terminal_split_ref.clone(),
            tse_state: terminal_split_edit_mode.clone(),
            tse_ref: terminal_split_edit_ref.clone(),
            sps_state: split_pane_sheet_id.clone(),
            sps_ref: split_pane_sheet_id_ref.clone(),
            skip_fade: skip_split_fade.clone(),
            map: terminal_split_map.clone(),
        };
        Callback::from(move |close_id: String| {
            // ターミナルタブの閉じ処理
            if close_id.starts_with("__TERM__") {
                let was_active = atref_close.borrow().as_ref() == Some(&close_id);
                // 閉じる前に隣接タブを特定（左優先）
                let next_tab = if was_active {
                    let order = to_ref_close.borrow();
                    let idx = order.iter().position(|x| x == &close_id);
                    idx.and_then(|i| {
                        if i > 0 { order.get(i - 1).cloned() } else { order.get(i + 1).cloned() }
                    })
                } else { None };
                // フェードアウト後にターミナルを閉じる
                tci_close.set(Some(close_id.clone()));
                let tci2 = tci_close.clone();
                let close_id2 = close_id.clone();
                let tids2 = tids_ref_close.clone();
                let ttids2 = ttids_close.clone();
                let atid2 = atid_close.clone();
                let atref2 = atref_close.clone();
                let aid2 = aid.clone();
                let aid_ref2 = aid_ref.clone();
                Timeout::new(300, move || {
                tci2.set(None);
                crate::js_interop::terminal_close(&close_id2);
                    tids2.borrow_mut().retain(|x| x != &close_id2);
                    ttids2.set(tids2.borrow().clone());
                    if was_active {
                        atid2.set(None);
                        *atref2.borrow_mut() = None;
                        if let Some(ref next) = next_tab {
                            if next.starts_with("__TERM__") {
                                atid2.set(Some(next.clone()));
                                *atref2.borrow_mut() = Some(next.clone());
                            } else {
                                aid2.set(Some(next.clone()));
                                *aid_ref2.borrow_mut() = Some(next.clone());
                            }
                        }
                        Timeout::new(50, || {
                            crate::js_interop::resize_editor();
                            crate::js_interop::focus_editor();
                        }).forget();
                    }
                }).forget();
                return;
            }
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
            close_tab_direct(close_id, rs.clone(), s_state.clone(), aid.clone(), sp.clone(), ncid.clone(), Some(aid_ref.clone()), to_ref_close.borrow().clone(), Some(atid_close.clone()), Some(atref_close.clone()), Some(tsh_close.clone()));
        })
    };

    // スプリットクローズダイアログのキーボードハンドラ
    {
        let is_open = *is_split_close_dialog_visible;
        let sel_state = split_close_selected.clone();
        let sel_ref = split_close_selected_ref.clone();
        let d = is_split_close_dialog_visible.clone();
        let atref = active_terminal_ref.clone();
        let tab_close = on_tab_close_cb.clone();
        let ts_enabled = terminal_split_enabled.clone();
        let ts_ref = terminal_split_ref.clone();
        let sp_sheet = split_pane_sheet_id.clone();
        let sp_sheet_ref = split_pane_sheet_id_ref.clone();
        let ts_map_split_close = terminal_split_map.clone();
        use_effect_with(is_open, move |&open| {
            if !open {
                return Box::new(|| ()) as Box<dyn FnOnce()>;
            }
            // ダイアログ表示時に選択をリセット
            *sel_ref.borrow_mut() = 0;
            sel_state.set(0);
            let window = web_sys::window().unwrap();
            let mut opts = gloo::events::EventListenerOptions::run_in_capture_phase();
            opts.passive = false;
            let listener = gloo::events::EventListener::new_with_options(&window, "keydown", opts, move |e| {
                let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                match ke.key().as_str() {
                    "ArrowLeft" | "ArrowRight" => {
                        e.stop_immediate_propagation(); e.prevent_default();
                        let cur = *sel_ref.borrow();
                        let new_sel = if cur == 0 { 1 } else { 0 };
                        *sel_ref.borrow_mut() = new_sel;
                        sel_state.set(new_sel);
                    }
                    "Enter" => {
                        e.stop_immediate_propagation(); e.prevent_default();
                        let sel = *sel_ref.borrow();
                        d.set(false);
                        if sel == 0 {
                            // ターミナルを閉じる
                            if let Some(tid) = atref.borrow().as_ref().cloned() {
                                *ts_ref.borrow_mut() = false;
                                ts_enabled.set(false);
                                sp_sheet.set(None);
                                *sp_sheet_ref.borrow_mut() = None;
                                ts_map_split_close.borrow_mut().insert(tid.clone(), (false, false, None));
                                tab_close.emit(tid);
                            }
                        } else {
                            // プレビューを閉じる
                            *ts_ref.borrow_mut() = false;
                            ts_enabled.set(false);
                            sp_sheet.set(None);
                            *sp_sheet_ref.borrow_mut() = None;
                            if let Some(tid) = atref.borrow().as_ref().cloned() {
                                ts_map_split_close.borrow_mut().insert(tid.clone(), (false, false, None));
                                gloo::timers::callback::Timeout::new(50, move || {
                                    crate::js_interop::terminal_focus(&tid);
                                }).forget();
                            }
                        }
                    }
                    "Escape" => {
                        e.stop_immediate_propagation(); e.prevent_default();
                        d.set(false);
                        if let Some(tid) = atref.borrow().as_ref().cloned() {
                            gloo::timers::callback::Timeout::new(50, move || {
                                crate::js_interop::terminal_focus(&tid);
                            }).forget();
                        }
                    }
                    _ => {}
                }
            });
            Box::new(move || drop(listener)) as Box<dyn FnOnce()>
        });
    }

    let on_close_tab_confirm = {
        let pending = pending_close_tab.clone();
        let rs = sheets_ref.clone();
        let s_state = sheets.clone();
        let aid = active_sheet_id.clone();
        let aid_ref = active_id_ref.clone();
        let sp = is_suppressing_changes.clone();
        let ncid = no_category_folder_id.clone();
        let to = tab_order_ref.clone();
        let atid = active_terminal_id.clone();
        let atref = active_terminal_ref.clone();
        let tsh_cc = TerminalSplitHandles {
            ts_state: terminal_split_enabled.clone(),
            ts_ref: terminal_split_ref.clone(),
            tse_state: terminal_split_edit_mode.clone(),
            tse_ref: terminal_split_edit_ref.clone(),
            sps_state: split_pane_sheet_id.clone(),
            sps_ref: split_pane_sheet_id_ref.clone(),
            skip_fade: skip_split_fade.clone(),
            map: terminal_split_map.clone(),
        };
        Callback::from(move |_: ()| {
            if let Some(close_id) = (*pending).clone() {
                pending.set(None);
                close_tab_direct(close_id, rs.clone(), s_state.clone(), aid.clone(), sp.clone(), ncid.clone(), Some(aid_ref.clone()), to.borrow().clone(), Some(atid.clone()), Some(atref.clone()), Some(tsh_cc.clone()));
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
        let to = tab_order_ref.clone();
        let atid = active_terminal_id.clone();
        let atref = active_terminal_ref.clone();
        let tsh_ucc = TerminalSplitHandles {
            ts_state: terminal_split_enabled.clone(),
            ts_ref: terminal_split_ref.clone(),
            tse_state: terminal_split_edit_mode.clone(),
            tse_ref: terminal_split_edit_ref.clone(),
            sps_state: split_pane_sheet_id.clone(),
            sps_ref: split_pane_sheet_id_ref.clone(),
            skip_fade: skip_split_fade.clone(),
            map: terminal_split_map.clone(),
        };
        Callback::from(move |_: ()| {
            if let Some(close_id) = (*pending).clone() {
                pending.set(None);
                close_tab_direct(close_id, rs.clone(), s_state.clone(), aid.clone(), sp.clone(), ncid.clone(), Some(aid_ref.clone()), to.borrow().clone(), Some(atid.clone()), Some(atref.clone()), Some(tsh_ucc.clone()));
            }
        })
    };

    let on_tab_reorder_cb = {
        let to_ref = tab_order_ref.clone();
        let to_state = tab_order_state.clone();
        Callback::from(move |(from_id, to_id): ReorderEvent| {
            let mut order = to_ref.borrow_mut();
            let fi = order.iter().position(|x| *x == from_id);
            let ti = order.iter().position(|x| *x == to_id);
            if let (Some(fi), Some(ti)) = (fi, ti) {
                if fi != ti {
                    let item = order.remove(fi);
                    order.insert(ti, item);
                    to_state.set(order.clone());
                }
            }
        })
    };
    let on_tab_drag_end_cb = {
        // シートの内部順序もtab_orderに合わせる
        let rs = sheets_ref.clone();
        let s_state = sheets.clone();
        let to_ref = tab_order_ref.clone();
        Callback::from(move |_: ()| {
            let order = to_ref.borrow();
            let mut us = (*rs.borrow()).clone();
            us.sort_by_key(|s| order.iter().position(|id| *id == s.id).unwrap_or(usize::MAX));
            *rs.borrow_mut() = us.clone();
            s_state.set(us);
        })
    };

    // tab_orderをsheets/terminalsと同期（新規追加・削除を検知）
    {
        let sheets_ids: Vec<String> = sheets.iter().map(|s| s.id.clone()).collect();
        let term_ids = (*terminal_tab_ids).clone();
        let to_ref = tab_order_ref.clone();
        let to_state = tab_order_state.clone();
        use_effect_with((sheets_ids, term_ids), move |(s_ids, t_ids)| {
            let mut order = to_ref.borrow_mut();
            // 存在しないIDを除去
            order.retain(|id| s_ids.contains(id) || t_ids.contains(id));
            // 新規IDを末尾に追加
            for id in s_ids.iter().chain(t_ids.iter()) {
                if !order.contains(id) {
                    order.push(id.clone());
                }
            }
            to_state.set(order.clone());
            || ()
        });
    }

    // active_terminal_idをRefCellに同期
    {
        let atref = active_terminal_ref.clone();
        let atid = (*active_terminal_id).clone();
        use_effect_with(atid, move |v| { *atref.borrow_mut() = v.clone(); || () });
    }

    // ターミナル表示切替
    {
        let atid = (*active_terminal_id).clone();
        let tfs_open = *terminal_font_size;
        use_effect_with(atid, move |atid| {
            if let Some(ref tid) = atid {
                let tid_clone = tid.clone();
                Timeout::new(50, move || {
                    let container_id = "terminal-area".to_string();
                    if !crate::js_interop::terminal_is_open(&tid_clone) {
                        spawn_local(async move {
                            crate::js_interop::terminal_open(&tid_clone, &container_id, 80, 24).await;
                            crate::js_interop::terminal_set_font_size(tfs_open);
                            crate::js_interop::terminal_focus(&tid_clone);
                        });
                    } else {
                        crate::js_interop::terminal_focus(&tid_clone);
                    }
                }).forget();
            }
            || ()
        });
    }

    // ターミナル終了イベント（シェルがCtrl+Dなどで終了した場合）
    {
        let tids_ref_exit = terminal_ids_ref.clone();
        let ttids_exit = terminal_tab_ids.clone();
        let atid_exit = active_terminal_id.clone();
        let atref_exit = active_terminal_ref.clone(); // RefCellで最新値を確実に取得
        let to_ref_exit = tab_order_ref.clone();
        let aid_exit = active_sheet_id.clone();
        let aid_ref_exit = active_id_ref.clone();
        let terminal_split_ref_exit = terminal_split_ref.clone();
        let terminal_split_exit = terminal_split_enabled.clone();
        let terminal_split_edit_ref_exit = terminal_split_edit_ref.clone();
        let terminal_split_edit_exit = terminal_split_edit_mode.clone();
        let split_pane_sheet_id_exit = split_pane_sheet_id.clone();
        let split_pane_sheet_id_ref_exit = split_pane_sheet_id_ref.clone();
        let sheets_ref_exit = sheets_ref.clone();
        let is_preview_exit = is_preview_visible.clone();
        let ts_map_exit = terminal_split_map.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let listener = EventListener::new(&window, "terminal-exit", move |e| {
                let ce = e.unchecked_ref::<web_sys::CustomEvent>();
                let detail = ce.detail();
                if let Some(id) = js_sys::Reflect::get(&detail, &JsValue::from_str("id")).ok().and_then(|v| v.as_string()) {
                    // RefCellから最新のactive_terminal_idを取得
                    let was_active = atref_exit.borrow().as_ref() == Some(&id);
                    // 隣接タブを特定（左優先）
                    let next_tab = if was_active {
                        let order = to_ref_exit.borrow();
                        let idx = order.iter().position(|x| x == &id);
                        idx.and_then(|i| {
                            if i > 0 { order.get(i - 1).cloned() } else { order.get(i + 1).cloned() }
                        })
                    } else { None };
                    crate::js_interop::terminal_close(&id);
                    tids_ref_exit.borrow_mut().retain(|x| x != &id);
                    ttids_exit.set(tids_ref_exit.borrow().clone());
                    // 終了したターミナルのスプリット状態エントリを削除
                    ts_map_exit.borrow_mut().remove(&id);
                    if was_active {
                        atid_exit.set(None);
                        *atref_exit.borrow_mut() = None;
                        // 分割ペインと編集モードをリセット
                        *terminal_split_ref_exit.borrow_mut() = false;
                        terminal_split_exit.set(false);
                        *terminal_split_edit_ref_exit.borrow_mut() = false;
                        terminal_split_edit_exit.set(false);
                        split_pane_sheet_id_exit.set(None);
                        *split_pane_sheet_id_ref_exit.borrow_mut() = None;
                        if let Some(ref next) = next_tab {
                            if next.starts_with("__TERM__") {
                                // ターミナルのスプリット状態・編集モード・ペインシートIDを復元
                                let (split, edit, pane_sheet) = ts_map_exit.borrow().get(next.as_str()).cloned().unwrap_or((false, false, None));
                                *terminal_split_ref_exit.borrow_mut() = split;
                                terminal_split_exit.set(split);
                                *terminal_split_edit_ref_exit.borrow_mut() = edit;
                                terminal_split_edit_exit.set(edit);
                                split_pane_sheet_id_exit.set(pane_sheet.clone());
                                *split_pane_sheet_id_ref_exit.borrow_mut() = pane_sheet;
                                atid_exit.set(Some(next.clone()));
                                *atref_exit.borrow_mut() = Some(next.clone());
                            } else {
                                // シートの表示モードを復元
                                let sheets_list = sheets_ref_exit.borrow();
                                if let Some(sheet) = sheets_list.iter().find(|s| s.id == *next) {
                                    is_preview_exit.set(sheet.is_preview);
                                    *terminal_split_ref_exit.borrow_mut() = sheet.is_split;
                                    terminal_split_exit.set(sheet.is_split);
                                }
                                drop(sheets_list);
                                aid_exit.set(Some(next.clone()));
                                *aid_ref_exit.borrow_mut() = Some(next.clone());
                            }
                        }
                        Timeout::new(50, || {
                            crate::js_interop::resize_editor();
                            crate::js_interop::focus_editor();
                        }).forget();
                    }
                }
            });
            move || { drop(listener); }
        });
    }

    // Tauri版: 起動時にウィンドウ透明度/ブラーを適用
    {
        let opacity = *window_opacity;
        let blur = *window_blur;
        use_effect_with((), move |_| {
            if (crate::js_interop::is_macos_tauri() || crate::js_interop::is_windows_tauri()) && opacity < 100 {
                crate::js_interop::set_window_opacity(opacity as f64 / 100.0);
            }
            if crate::js_interop::is_windows_tauri() && blur > 0 {
                crate::js_interop::set_window_blur(blur);
            }
            || ()
        });
    }

    // スプリッタードラッグコールバック
    let on_splitter_mousedown = {
        let is_dragging = is_splitter_dragging.clone();
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            *is_dragging.borrow_mut() = true;
        })
    };

    let on_container_mousemove = {
        let is_dragging = is_splitter_dragging.clone();
        let ratio = split_ratio.clone();
        let ratio_ref = split_ratio_ref.clone();
        Callback::from(move |e: MouseEvent| {
            if !*is_dragging.borrow() { return; }
            let target = e.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok());
            if let Some(el) = target {
                let rect = el.get_bounding_client_rect();
                let x = e.client_x() as f64;
                let new_ratio = ((x - rect.left()) / rect.width()).clamp(0.25, 0.75);
                if (*ratio_ref.borrow() - new_ratio).abs() > 0.002 {
                    *ratio_ref.borrow_mut() = new_ratio;
                    ratio.set(new_ratio);
                }
            }
        })
    };

    let on_container_mouseup = {
        let is_dragging = is_splitter_dragging.clone();
        Callback::from(move |_: MouseEvent| {
            *is_dragging.borrow_mut() = false;
        })
    };

    // 分割が解除された時に編集モードをリセット
    {
        let edit_mode = terminal_split_edit_mode.clone();
        let edit_ref = terminal_split_edit_ref.clone();
        let split_open_i = *terminal_split_enabled;
        use_effect_with(split_open_i, move |&active| {
            if !active {
                *edit_ref.borrow_mut() = false;
                edit_mode.set(false);
            }
            || ()
        });
    }

    // 分割比率・分割状態切り替え時にエディタをリサイズ
    {
        let ratio_i = (*split_ratio * 1000.0) as i32;
        let split_open_i = *terminal_split_enabled;
        use_effect_with((ratio_i, split_open_i), move |_| {
            crate::js_interop::resize_editor();
            || ()
        });
    }

    // 分割プレビュー有効時にカーソル同期を設定（ターミナルアクティブ時は解除）
    {
        let is_split_i = *terminal_split_enabled && (*active_terminal_id).is_none();
        use_effect_with(is_split_i, move |&is_split| {
            if is_split {
                crate::js_interop::setup_cursor_sync();
            } else {
                crate::js_interop::teardown_cursor_sync();
            }
            move || {
                crate::js_interop::teardown_cursor_sync();
            }
        });
    }

    // terminal_split_enabled が true で:
    //   - ターミナルアクティブ → ターミナル左・シート右（terminal split）
    //   - シートのみアクティブ → エディタ左・シートプレビュー右（sheet split）
    let is_split_view = *terminal_split_enabled && (*active_terminal_id).is_none() && (*active_sheet_id).is_some();
    let is_terminal_split = *terminal_split_enabled && (*active_terminal_id).is_some() && (*active_sheet_id).is_some();
    let active_content_id = if (*active_terminal_id).is_some() { (*active_terminal_id).clone() } else { (*active_sheet_id).clone() };
    let is_content_closing = (*tab_closing_id).is_some() && *tab_closing_id == active_content_id;
    let is_terminal_closing = is_content_closing && (*active_terminal_id).is_some();

    // 右ペインのフェードイン／アウト制御
    {
        let mounted = split_pane_mounted.clone();
        let opacity = split_pane_opacity.clone();
        let is_terminal = split_pane_is_terminal.clone();
        let showing = is_split_view || is_terminal_split;
        let is_term = is_terminal_split;
        let skip_fade = skip_split_fade.clone();
        use_effect_with(showing, move |&showing| {
            let no_fade = *skip_fade.borrow();
            *skip_fade.borrow_mut() = false;
            if no_fade {
                // タブ切り替え時: フェードなしで即座に表示/非表示
                is_terminal.set(is_term);
                mounted.set(showing);
                opacity.set(showing);
            } else if showing {
                is_terminal.set(is_term);
                mounted.set(true);
                let opacity2 = opacity.clone();
                gloo::timers::callback::Timeout::new(10, move || {
                    opacity2.set(true);
                }).forget();
            } else {
                // ブラウザが opacity-100 を描画した後に opacity-0 へ変更するため
                // 1フレーム待ってから opacity を下げ、その後 100ms でアンマウント
                let opacity2 = opacity.clone();
                let mounted2 = mounted.clone();
                gloo::timers::callback::Timeout::new(8, move || {
                    opacity2.set(false);
                    gloo::timers::callback::Timeout::new(300, move || {
                        mounted2.set(false);
                    }).forget();
                }).forget();
            }
            || ()
        });
    }

    // 編集モード: Aceエディタの初期化/破棄 + 変更イベントリスナー
    {
        let sheets_r = sheets_ref.clone();
        let sheets_s = sheets.clone();
        let aid_r = active_id_ref.clone();
        let split_pane_sid_r = split_pane_sheet_id.clone();
        let os = on_save_cb.clone();
        let debounce = split_edit_debounce.clone();
        let is_saving_split = saving_sheet_id.clone();
        // NOTE: saving_id_ref は on_save_cb の並行保存 lock 専用。split-editor-changed は触らない。
        let edit_mode = *terminal_split_edit_mode;
        use_effect_with(edit_mode, move |&editing| {
            if editing {
                // ターミナルスプリット時は split_pane_sheet_id を優先、なければ active_sheet_id
                let split_pane_id = (*split_pane_sid_r).clone();
                let is_terminal_split = split_pane_id.is_some();
                let (content, filename) = {
                    let id = split_pane_id.clone().or_else(|| aid_r.borrow().clone());
                    if let Some(id) = id {
                        let c = if is_terminal_split {
                            // ターミナルスプリット: メインエディタではなくsheets_rから取得
                            sheets_r.borrow().iter().find(|s| s.id == id)
                                .map(|s| s.content.clone()).unwrap_or_default()
                        } else {
                            get_editor_content().as_string().unwrap_or_else(|| {
                                sheets_r.borrow().iter().find(|s| s.id == id)
                                    .map(|s| s.content.clone()).unwrap_or_default()
                            })
                        };
                        let f = sheets_r.borrow().iter().find(|s| s.id == id)
                            .map(|s| s.title.clone()).unwrap_or_default();
                        (c, f)
                    } else { ("".to_string(), "".to_string()) }
                };
                // DOMが描画された後にAceエディタを初期化
                let content_c = content.clone();
                let filename_c = filename.clone();
                let sheet_id_c = split_pane_id.clone().or_else(|| aid_r.borrow().clone()).unwrap_or_default();
                gloo::timers::callback::Timeout::new(20, move || {
                    crate::js_interop::init_split_editor("split-edit-editor", &content_c, &filename_c, &sheet_id_c);
                }).forget();
                // "split-editor-changed" イベントで自動保存
                // detail.final=true の場合のみ Drive 保存 + メインエディタ再ロード
                let split_pane_id_for_listener = split_pane_id.clone();
                let window = web_sys::window().unwrap();
                let listener = gloo::events::EventListener::new(&window, "split-editor-changed", move |e: &web_sys::Event| {
                    let is_final = e
                        .dyn_ref::<web_sys::CustomEvent>()
                        .and_then(|ce| {
                            js_sys::Reflect::get(&ce.detail(), &JsValue::from_str("final")).ok()
                        })
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let content = crate::js_interop::get_split_editor_content();
                    let aid = split_pane_id_for_listener.clone().or_else(|| (*aid_r.borrow()).clone());
                    if let Some(id) = aid {
                        // 保存中インジケータを表示（UI用のみ）
                        is_saving_split.set(Some(id.clone()));
                        let mut cur = (*sheets_r.borrow()).clone();
                        let mut content_changed = false;
                        if let Some(sheet) = cur.iter_mut().find(|s| s.id == id) {
                            if sheet.content != content {
                                let now = js_sys::Date::now() as u64;
                                sheet.content = content.clone();
                                sheet.is_modified = true;
                                sheet.temp_content = Some(content.clone());
                                sheet.temp_timestamp = Some(now);
                                content_changed = true;
                                let js = JSSheet { id: sheet.id.clone(), guid: sheet.guid.clone(), category: sheet.category.clone(), title: sheet.title.clone(), content: content.clone(), is_modified: true, drive_id: sheet.drive_id.clone(), temp_content: Some(content.clone()), temp_timestamp: Some(now), last_sync_timestamp: sheet.last_sync_timestamp, tab_color: sheet.tab_color.clone(), total_size: sheet.total_size, loaded_bytes: sheet.loaded_bytes, needs_bom: sheet.needs_bom, is_preview: sheet.is_preview, created_at: sheet.created_at };
                                let is_saving_inner = is_saving_split.clone();
                                spawn_local(async move {
                                    let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                    if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; }
                                    // IndexedDB保存完了後にインジケータを解除
                                    is_saving_inner.set(None);
                                });
                            } else {
                                is_saving_split.set(None);
                            }
                        }
                        if content_changed {
                            *sheets_r.borrow_mut() = cur.clone();
                            sheets_s.set(cur);
                        }

                        // Drive 保存のスケジュール:
                        //  - 通常の入力（final=false）: 3 秒無入力で発火（バックグラウンド保存）
                        //  - スプリット解除（final=true）: メインエディタへ再ロード + 即座に Drive 保存
                        if content_changed || is_final {
                            let os_c = os.clone();
                            let delay_ms = if is_final { 200 } else { 3000 };
                            let id_for_debounce = id.clone();
                            *debounce.borrow_mut() = Some(gloo::timers::callback::Timeout::new(delay_ms, move || { os_c.emit((false, Some(id_for_debounce))); }));
                        }
                        if is_final {
                            load_editor_content(&content);
                        }
                    }
                });
                Box::new(move || {
                    // destroy_split_editor は dirty な場合に split-editor-changed を dispatch するため、
                    // listener を drop する前に呼ぶ必要がある。
                    crate::js_interop::destroy_split_editor();
                    drop(listener);
                }) as Box<dyn FnOnce()>
            } else {
                Box::new(|| ()) as Box<dyn FnOnce()>
            }
        });
    }

    let left_width_style = if *split_pane_mounted {
        format!("width: {}%", (*split_ratio * 100.0) as i32)
    } else {
        "width: 100%".to_string()
    };
    // 分割ペインのコンテンツ: タブ選択がある場合はそのシート、なければアクティブシート
    let split_pane_content = if is_split_view || is_terminal_split {
        if let Some(ref sid) = *split_pane_sheet_id {
            // 特定のシートを表示
            sheets_ref.borrow().iter().find(|s| s.id == *sid).map(|s| s.content.clone()).unwrap_or_default()
        } else {
            // アクティブシートを表示
            let aid = (*active_sheet_id).clone();
            if let Some(id) = aid {
                get_editor_content().as_string().unwrap_or_else(|| {
                    sheets.iter().find(|s| s.id == id).map(|s| s.content.clone()).unwrap_or_default()
                })
            } else { "".to_string() }
        }
    } else { "".to_string() };
    // 分割ペインで表示するシートの拡張子
    let split_pane_ext = {
        let supported_exts = vec!["txt","md","js","ts","rs","c","cpp","h","m","cs","java","php","rb","pl","py","sh","coffee","toml","json","xml","html","css","sql","yaml"];
        let raw = if let Some(ref sid) = *split_pane_sheet_id {
            sheets_ref.borrow().iter().find(|s| s.id == *sid).map(|s| s.title.split('.').last().unwrap_or("txt").to_lowercase()).unwrap_or_else(|| "txt".to_string())
        } else {
            current_file_ext.clone()
        };
        if supported_exts.contains(&raw.as_str()) { raw } else { "txt".to_string() }
    };
    let preview_scroll = active_sheet_id.as_ref().and_then(|id| sheets_ref.borrow().iter().find(|s| s.id == *id).map(|s| s.preview_scroll_top)).unwrap_or(0.0);

    let inline_preview_content = if *is_preview_visible {
        let aid = (*active_sheet_id).clone();
        if let Some(id) = aid {
            get_editor_content().as_string().unwrap_or_else(|| {
                sheets.iter().find(|s| s.id == id).map(|s| s.content.clone()).unwrap_or_default()
            })
        } else { "".to_string() }
    } else { "".to_string() };

    // 右ペインに表示するコンテンツ
    // 表示中は最新コンテンツを取得してキャッシュし、フェードアウト中はキャッシュを使用
    let showing_now = is_split_view || is_terminal_split;
    let split_right_content = if showing_now {
        // terminal/non-terminal 共に split_pane_content を使用
        let content = split_pane_content.clone();
        *split_pane_cached_content.borrow_mut() = content.clone();
        content
    } else {
        // フェードアウト中: キャッシュされたコンテンツを使用（DOM更新なし）
        split_pane_cached_content.borrow().clone()
    };

    let help_html: Html = if *is_help_visible {
        let ih = is_help_visible.clone();
        let atid_help = active_terminal_id.clone();
        let on_install: Option<Callback<()>> = if !crate::js_interop::is_tauri() {
            let is_conf = is_install_confirm_visible.clone();
            let is_man = is_install_manual_visible.clone();
            let ih_for_install = ih.clone();
            Some(Callback::from(move |_: ()| { ih_for_install.set(false); if crate::js_interop::can_install_pwa() { is_conf.set(true); } else { is_man.set(true); } }))
        } else { None };
        html! {
            <div class="pointer-events-auto">
                <ShortcutHelp
                    on_close={Callback::from(move |_| {
                        ih.set(false);
                        if let Some(ref tid) = *atid_help {
                            crate::js_interop::terminal_focus(tid);
                        } else {
                            focus_editor();
                        }
                    })}
                    on_install={on_install}
                    is_guest_mode={*is_guest_mode}
                />
            </div>
        }
    } else { html! {} };

    let char_code_html: Html = if *is_char_code_visible {
        let icc = is_char_code_visible.clone();
        let ch = (*char_code_char).clone();
        html! {
            <div class="pointer-events-auto">
                <CharCodeDialog
                    char_str={ch}
                    on_close={Callback::from(move |_| { icc.set(false); focus_editor(); })}
                />
            </div>
        }
    } else { html! {} };

    let sheet_info_html: Html = if *is_sheet_info_visible {
        let isiv = is_sheet_info_visible.clone();
        let (info_title, info_char_count, info_created_at, info_updated_at, info_needs_bom, info_category_name) = {
            let rs = sheets_ref.borrow();
            let info_sheet_id = (*split_pane_sheet_id).clone().or_else(|| (*active_sheet_id).clone());
            if let Some(aid) = info_sheet_id.as_ref() {
                if let Some(sheet) = rs.iter().find(|s| s.id == *aid) {
                    let cat_name = if sheet.category == "__LOCAL__" {
                        i18n::t("local_file", lang)
                    } else if sheet.category.is_empty() {
                        i18n::t("OTHERS", lang)
                    } else {
                        categories.iter().find(|c| c.id == sheet.category)
                            .map(|c| if c.name == "OTHERS" { i18n::t("OTHERS", lang) } else { c.name.clone() })
                            .unwrap_or_else(|| i18n::t("OTHERS", lang))
                    };
                    let updated = sheet.temp_timestamp.or(sheet.last_sync_timestamp).filter(|&t| t > 0);
                    (sheet.title.clone(), sheet.content.chars().count(), sheet.created_at, updated, sheet.needs_bom, cat_name)
                } else {
                    ("".to_string(), 0, None, None, true, "".to_string())
                }
            } else {
                ("".to_string(), 0, None, None, true, "".to_string())
            }
        };
        html! {
            <div class="pointer-events-auto">
                <SheetInfoDialog
                    title={info_title}
                    char_count={info_char_count}
                    created_at={info_created_at}
                    updated_at={info_updated_at}
                    needs_bom={info_needs_bom}
                    category_name={info_category_name}
                    on_close={Callback::from(move |_| { isiv.set(false); focus_editor(); })}
                />
            </div>
        }
    } else { html! {} };

    html! {
        <div id="app-root" class="relative h-screen w-screen overflow-hidden bg-gray-950" key="app-root">
            <main key="main-editor-surface" class={classes!("absolute", "inset-0", "flex", "flex-col", "text-white", "transition-opacity", "duration-300", if !*is_authenticated && *network_connected { "opacity-0" } else { "opacity-100" } )}>
                                <ButtonBar 
                                    key="top-button-bar"
                                    on_new_sheet={on_new_sheet_cb.clone()} 
                                    on_open={on_open_dialog} 
                                    on_import={on_import_cb} 
                                    on_change_font_size={on_change_font_size.clone()} 
                                    on_change_category={on_change_category_cb} 
                                    on_preview={on_preview_cb} on_help={on_help_cb} on_logout={on_logout} current_category={current_cat.clone()} categories={(*categories).clone()} is_new_sheet={is_current_new_sheet} is_dropdown_open={*is_category_dropdown_open} on_toggle_dropdown={let id = is_category_dropdown_open.clone(); Callback::from(move |v| id.set(v))} vim_mode={*vim_mode} on_open_settings={let sv = is_settings_visible.clone(); Callback::from(move |_| sv.set(true))} file_extension={current_file_ext.clone()} on_change_extension={on_change_extension_cb.clone()} sheet_count={tab_infos.len()} on_open_sheet_list={let sl = is_sheet_list_visible.clone(); Callback::from(move |_| sl.set(true))} is_guest_mode={*is_guest_mode} on_sheet_info={Some(on_sheet_info_cb)} is_terminal_active={(*active_terminal_id).is_some()} on_terminal_split={if crate::js_interop::is_tauri() { Some(on_terminal_split_cb) } else { None }} />
                <TabBar sheets={tab_infos.clone()} active_sheet_id={if (*active_terminal_id).is_some() { (*active_terminal_id).clone() } else { (*active_sheet_id).clone() }} on_select_tab={on_tab_select_cb.clone()} on_close_tab={on_tab_close_cb.clone()} on_reorder={on_tab_reorder_cb} on_drag_end={on_tab_drag_end_cb} on_new_tab={Some({ let cb = on_new_sheet_cb.clone(); Callback::from(move |_| cb.emit(())) })} />
                // 分割プレビューモード
                {html! {
                        <div class={classes!("flex-1", "flex", "overflow-hidden", "bg-gray-900", "transition-opacity", "duration-300", if is_content_closing { "opacity-0" } else { "opacity-100" })}
                             onmousemove={on_container_mousemove.clone()}
                             onmouseup={on_container_mouseup.clone()}>

                            // 左ペイン: エディタ（常に表示）
                            <div class="relative overflow-hidden" style={left_width_style.clone()}>
                                // エディタ本体
                                <div id="editor" key="ace-editor-fixed-node" class="absolute inset-0 z-10 bg-transparent" style="width: 100%; height: 100%;"></div>

                                // オーバーレイプレビュー（非分割モード時のみ）
                                if !is_split_view && *is_preview_visible {
                                    <div class={classes!("absolute", "inset-0", "z-20", "transition-opacity", "duration-300",
                                                         if *preview_overlay_opacity { "opacity-100" } else { "opacity-0" })}>
                                        <InlinePreview content={inline_preview_content.clone()} file_ext={current_file_ext.clone()} font_size={*preview_font_size} initial_scroll_top={preview_scroll} is_split=false />
                                    </div>
                                }

                                // ターミナル（Tauri版のみ）
                                if crate::js_interop::is_tauri() {
                                    <div id="terminal-area" key="terminal-area-fixed" class={classes!("absolute", "inset-0", "z-30", "bg-[#1d2021]", "transition-opacity", "duration-300", if (*active_terminal_id).is_none() { "hidden" } else { "" }, if is_terminal_closing { "opacity-0" } else { "opacity-100" })}></div>
                                }

                                // フォールバック表示
                                <div class="absolute inset-0 flex flex-col items-center justify-center text-gray-600 bg-gray-900 z-0">
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-16 w-12 mb-4 opacity-20" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                                    </svg>
                                    <p class="text-sm font-bold uppercase tracking-widest opacity-40">{ "Editor not loaded (Offline)" }</p>
                                    <p class="text-[10px] mt-2 opacity-30">{ "Please reconnect to the internet to initialize the editor." }</p>
                                </div>
                            </div>

                            // 分割バーと右ペイン（通常スプリット or ターミナルスプリット、フェードイン/アウト付き）
                            if *split_pane_mounted {
                                // 分割バー
                                <div class="w-1 flex-shrink-0 cursor-col-resize bg-[#504945] hover:bg-[#689d6a] transition-colors z-40 select-none"
                                     onmousedown={on_splitter_mousedown.clone()} />
                                // 右ペイン: opacity トランジションでフェードイン/アウト
                                <div class={classes!("flex-1", "overflow-hidden", "transition-opacity", "duration-300",
                                                     if *split_pane_opacity { "opacity-100" } else { "opacity-0" })}>
                                    <div class={classes!("w-full", "h-full", "transition-opacity", "duration-300",
                                                         if *split_content_opacity { "opacity-100" } else { "opacity-0" })}>
                                        if *terminal_split_edit_mode && (is_terminal_split || is_split_view) {
                                            <div id="split-edit-editor" class="w-full h-full" />
                                        } else {
                                            <InlinePreview content={split_right_content.clone()} file_ext={split_pane_ext.clone()} font_size={*font_size} initial_scroll_top={preview_scroll} is_split=true />
                                        }
                                    </div>
                                </div>
                            }
                        </div>
                    }
                }
                                <StatusBar 
                                    key="bottom-status-bar" 
                                    network_status={*network_connected} 
                                    is_saving={(*saving_sheet_id).as_ref() == active_sheet_id.as_ref() && active_sheet_id.is_some()}
                                    on_open_settings={let sv = is_settings_visible.clone(); Callback::from(move |_| sv.set(true))}
                                    on_toggle_terminal={if crate::js_interop::is_tauri() { Some({
                                        let tids_ref = terminal_ids_ref.clone();
                                        let counter = terminal_counter.clone();
                                        let atid = active_terminal_id.clone();
                                        let atref_tog = active_terminal_ref.clone();
                                        let ttids = terminal_tab_ids.clone();
                                        let ts_tog = terminal_split_enabled.clone();
                                        let ts_ref_tog = terminal_split_ref.clone();
                                        let ssf_tog = skip_split_fade.clone();
                                        let aid_ref_tog = active_id_ref.clone();
                                        let to_ref_tog = tab_order_ref.clone();
                                        Callback::from(move |_| {
                                            let mut c = counter.borrow_mut();
                                            *c += 1;
                                            let tid = format!("__TERM__{}", *c);
                                            tids_ref.borrow_mut().push(tid.clone());
                                            ttids.set(tids_ref.borrow().clone());
                                            // アクティブがシートの場合、シートタブの左（同じ位置）にターミナルタブを挿入
                                            let is_terminal_active = atref_tog.borrow().is_some();
                                            if !is_terminal_active {
                                                if let Some(aid) = (*aid_ref_tog.borrow()).clone() {
                                                    let mut order = to_ref_tog.borrow_mut();
                                                    if let Some(pos) = order.iter().position(|x| x == &aid) {
                                                        order.insert(pos, tid.clone());
                                                    } else {
                                                        order.push(tid.clone());
                                                    }
                                                } else {
                                                    to_ref_tog.borrow_mut().push(tid.clone());
                                                }
                                            } else {
                                                to_ref_tog.borrow_mut().push(tid.clone());
                                            }
                                            // 新規ターミナルは非スプリット状態で開く（フェードなし）
                                            *ssf_tog.borrow_mut() = true;
                                            ts_tog.set(false);
                                            *ts_ref_tog.borrow_mut() = false;
                                            atid.set(Some(tid));
                                        })
                                    }) } else { None }}
                                    is_terminal_open={(*active_terminal_id).is_some()}
                                    is_terminal_active={(*active_terminal_id).is_some()}
                                    category_name={current_cat_name}
                                    file_name={current_file_name}
                                />
            </main>
            <div id="overlays-layer" class="pointer-events-none fixed inset-0 z-[100]">
                if !*is_authenticated && *network_connected && !crate::auth_interop::is_signed_in() {
                    <div class="pointer-events-auto fixed inset-0 flex items-center justify-center bg-gray-900 overflow-y-auto p-4">
                        <div class="text-center max-w-2xl">
                            <img src="assets/image/icon.svg" class="mx-auto mb-8 shadow-2xl" style="width: 15vmin; height: 15vmin;" alt="Leaf Icon" />
                            <h1 class="text-4xl font-extrabold text-white mb-6 tracking-tight">{ i18n::t("welcome_headline", lang) }</h1>
                            <div class="mb-10 text-gray-300 text-sm leading-relaxed whitespace-pre-wrap opacity-80 bg-gray-800/30 p-6 rounded-lg border border-white/5 shadow-inner text-left">{ Html::from_html_unchecked(i18n::t("app_policy_description", lang).into()) }</div>
                                                                                                                <div class="flex flex-col sm:flex-row items-center justify-center gap-3">
                                                                                                                    <button onclick={on_login} class="bg-emerald-600 hover:bg-emerald-700 text-white font-bold py-3 px-8 rounded-md transition-colors shadow-lg text-lg">
                                                                                                                        { i18n::t("signin_with_google", lang) }
                                                                                                                    </button>
                                                                                                                    <button onclick={on_guest_login_cb} class="bg-gray-700 hover:bg-gray-600 text-gray-200 font-bold py-3 px-8 rounded-md transition-colors shadow-lg text-lg">
                                                                                                                        { i18n::t("use_without_login", lang) }
                                                                                                                    </button>
                                                                                                                </div>
                                                                                                                <div class="mt-6 flex flex-row items-center justify-center space-x-4">
                                                                                                                    <a href={if lang == Language::Ja { "/about_ja" } else { "/about" }} target="_blank" class="text-gray-500 hover:text-emerald-400 text-xs underline transition-colors">
                                                                                                                        { i18n::t("about", lang) }
                                                                                                                    </a>
                                                                                                                    <a href="/guide" target="_blank" class="text-gray-500 hover:text-emerald-400 text-xs underline transition-colors">
                                                                                                                        { i18n::t("tutorial", lang) }
                                                                                                                    </a>
                                                                                                                    <a href="/terms" target="_blank" class="text-gray-500 hover:text-emerald-400 text-xs underline transition-colors">
                                                                                                                        { "Terms / 利用規約" }
                                                                                                                    </a>
                                                                                                                    <a href="/privacy" target="_blank" class="text-gray-500 hover:text-emerald-400 text-xs underline transition-colors">
                                                                                                                        { "Privacy / ポリシー" }
                                                                                                                    </a>
                                                                                                                    <a href="/licenses" target="_blank" class="text-gray-500 hover:text-emerald-400 text-xs underline transition-colors">
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
                                on_close={let iv = is_file_open_dialog_visible.clone(); let sp = is_suppressing_changes.clone(); let aid = active_id_ref.clone(); let rs = sheets_ref.clone(); let s_state = sheets.clone(); let atref_fo = active_terminal_ref.clone(); move |_| { iv.set(false); sp.set(false); if let Some(ref tid) = *atref_fo.borrow() { crate::js_interop::terminal_focus(tid); } else { focus_editor(); } let aid_val = (*aid.borrow()).clone(); let rs_c = rs.clone(); let s_state_c = s_state.clone(); if let Some(id) = aid_val { let sheets_list = (*rs_c.borrow()).clone(); if let Some(sheet) = sheets_list.iter().find(|s| s.id == id) { if !sheet.category.is_empty() && sheet.category != "__LOCAL__" { if let Some(did) = sheet.drive_id.clone() { let sheet_id = id.clone(); spawn_local(async move { if let Err(_) = crate::drive_interop::get_file_metadata(&did).await { let mut us = (*rs_c.borrow()).clone(); if let Some(s) = us.iter_mut().find(|x| x.id == sheet_id) { s.drive_id = None; s.category = "OTHERS".to_string(); s.is_modified = true; set_gutter_status("unsaved"); let js = s.to_js(); let ser = serde_wasm_bindgen::Serializer::json_compatible(); if let Ok(v) = js.serialize(&ser) { let _ = save_sheet(v).await; } } *rs_c.borrow_mut() = us.clone(); s_state_c.set(us); } }); } } } } } } 
                                on_select={on_file_sel_cb} leaf_data_id={ldid} categories={(*categories).clone()} on_refresh={on_refresh_cats_cb} on_delete_category={on_delete_category_cb} on_rename_category={on_rename_category_cb} on_delete_file={on_delete_file_cb} on_move_file={on_move_file_cb} on_start_processing={let lmk = loading_message_key.clone(); move |_| { lmk.set("synchronizing"); }} on_preview_toggle={let ifds = is_file_dialog_sub_active.clone(); Callback::from(move |v| ifds.set(v))} 
                                on_sub_active_change={let ifds = is_file_dialog_sub_active.clone(); Callback::from(move |v| ifds.set(v))}
                                is_sub_dialog_open={is_sub_overlay_active} is_creating_category={*is_creating_category} on_create_category_toggle={let ic = is_creating_category.clone(); Callback::from(move |v| ic.set(v))} 
                                refresh_files_trigger={*file_refresh_trigger} is_loading={*is_file_list_loading} on_loading_change={let l = is_file_list_loading.clone(); Callback::from(move |v| l.set(v))} 
                                on_network_status_change={let nc = network_connected.clone(); Callback::from(move |v| nc.set(v))}
                                font_size={*preview_font_size} on_change_font_size={on_change_preview_font_size.clone()}
                                is_processing={*is_processing_dialog}
                                close_trigger={*file_close_trigger}
                                active_category_id={current_cat.clone()}
                                active_drive_id={active_sheet_id.as_ref().and_then(|id| sheets.iter().find(|s| s.id == *id).and_then(|s| s.drive_id.clone()))}
                            />
                        </div>
                    }
                }
                { help_html }
                { char_code_html }
                { sheet_info_html }
                if *is_install_confirm_visible { <div class="pointer-events-auto"><ConfirmDialog title={i18n::t("install_title", lang)} message={i18n::t("install_confirm", lang)} on_confirm={let ic = is_install_confirm_visible.clone(); move |_| { ic.set(false); spawn_local(async move { crate::js_interop::trigger_pwa_install().await; }); }} on_cancel={let ic = is_install_confirm_visible.clone(); move |_| ic.set(false)} /></div> }
                if *is_install_manual_visible { <div class="pointer-events-auto"><ConfirmDialog title={i18n::t("install_manual_title", lang)} message={i18n::t("install_manual_message", lang)} ok_label={i18n::t("ok", lang)} on_confirm={let im = is_install_manual_visible.clone(); move |_| im.set(false)} on_cancel={let im = is_install_manual_visible.clone(); move |_| im.set(false)} /></div> }
                if let Some(del_diag) = if let Some(_) = *pending_delete_category { let title = i18n::t("delete", lang); let message = i18n::t("confirm_delete_category", lang); let pending = pending_delete_category.clone(); let on_cfm = on_delete_category_cfm.clone(); Some(html! { <ConfirmDialog title={title} message={message} on_confirm={move |_| { on_cfm.emit(1); }} on_cancel={move |_| { pending.set(None); }} /> }) } else { None } { <div class="pointer-events-auto">{ del_diag }</div> }
                if let Some(conf_diag) = if !conflict_queue.is_empty() { let conflict = conflict_queue.first().unwrap(); let title = if conflict.is_missing_on_drive { i18n::t("file_not_found", lang) } else { i18n::t("conflict_detected", lang) }; let message = if conflict.is_missing_on_drive { i18n::t("missing_file_message", lang).replace("{}", &conflict.title) } else { i18n::t("conflict_message", lang).replace("{}", &conflict.title) }; let options = if conflict.is_missing_on_drive { vec![DialogOption { id: 1, label: i18n::t("opt_reupload", lang) }, DialogOption { id: 3, label: i18n::t("opt_delete_local", lang) }] } else { vec![DialogOption { id: 0, label: i18n::t("opt_load_drive", lang) }, DialogOption { id: 1, label: i18n::t("opt_overwrite_drive", lang) }, DialogOption { id: 2, label: i18n::t("opt_save_new", lang) }] }; let on_cfm = on_conf_cfm.clone(); Some(html! { <CustomDialog title={title} message={message} options={options} on_confirm={on_cfm} /> }) } else { None } { <div class="pointer-events-auto">{ conf_diag }</div> }
                if let Some(fb_alert) = if let Some(_) = fallback_queue.first() { let on_cfm = on_fallback_cfm.clone(); Some(html! { <CustomDialog title={i18n::t("category_not_found_title", lang)} message={i18n::t("category_not_found_fallback", lang)} options={vec![DialogOption { id: 0, label: i18n::t("ok", lang) }]} on_confirm={on_cfm} on_cancel={let fq = fallback_queue.clone(); Some(Callback::from(move |_| { fq.set(Vec::new()); }))} /> }) } else { None } { <div class="pointer-events-auto">{ fb_alert }</div> }
                if let Some(nc_diag) = if !name_conflict_queue.is_empty() { let conflict = name_conflict_queue.first().unwrap(); let title = i18n::t("filename_conflict", lang); let message = i18n::t("filename_conflict_message", lang).replace("{}", &conflict.filename); let on_cfm = on_name_conflict_cfm.clone(); let ncq = name_conflict_queue.clone(); let labels = vec![i18n::t("opt_nc_overwrite", lang), i18n::t("opt_nc_new_guid", lang), i18n::t("opt_nc_rename", lang)]; Some(html! { <NameConflictDialog title={title} message={message} current_name={conflict.filename.clone()} labels={labels} on_confirm={on_cfm} on_cancel={move |_| { ncq.set(Vec::new()); }} /> }) } else { None } { <div class="pointer-events-auto">{ nc_diag }</div> }
                <LoadingOverlay is_visible={*is_import_lock} message={i18n::t("synchronizing", lang)} is_fading_out={*is_import_fading_out} z_index="z-[90]" />
                if *is_loading { <div class={classes!("fixed", "inset-0", "z-[200]", "flex", "items-center", "justify-center", "bg-gray-900", "transition-opacity", "duration-300", "pointer-events-auto", if *is_fading_out { "opacity-0" } else { "opacity-100" } )}><div class="flex flex-col items-center">if *is_initial_load { <img src="assets/image/icon.svg" class="mb-8 shadow-2xl animate-in fade-in zoom-in duration-500" style="width: 20vmin; height: 20vmin;" alt="Leaf Icon" /> }<div class="w-12 h-12 border-4 border-emerald-500 border-t-transparent rounded-full animate-spin"></div>if *is_authenticated { <p class="mt-4 text-white font-bold text-lg animate-pulse">{ i18n::t(*loading_message_key, lang) }</p> }</div></div> }
                if *is_logout_confirm_visible { <div class="pointer-events-auto"><ConfirmDialog title={i18n::t("logout", lang)} message={i18n::t("confirm_logout", lang)} on_confirm={let ic = is_logout_confirm_visible.clone(); let il = is_loading.clone(); let lmk = loading_message_key.clone(); let ifo = is_fading_out.clone(); move |_| { ic.set(false); lmk.set("logging_out"); il.set(true); ifo.set(false); if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) { let _ = storage.remove_item(GUEST_MODE_KEY); } spawn_local(async move { crate::auth_interop::sign_out().await; Timeout::new(800, move || { web_sys::window().unwrap().location().set_href("/login").unwrap(); }).forget(); }); } } on_cancel={let ic = is_logout_confirm_visible.clone(); move |_| ic.set(false)} /></div> }
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
                if (*pending_empty_delete).is_some() {
                    <div class="pointer-events-auto"><EmptySheetDialog
                        lang={lang}
                        on_cancel={let ped = pending_empty_delete.clone(); Callback::from(move |_| ped.set(None))}
                        on_save={{
                            let ped = pending_empty_delete.clone();
                            let rs = sheets_ref.clone();
                            let s_state = sheets.clone();
                            Callback::from(move |_| {
                                if let Some(ref sheet_id) = (*ped).clone() {
                                    ped.set(None);
                                    // 空ファイルとして直接保存
                                    let mut cur_s = (*rs.borrow()).clone();
                                    if let Some(sheet) = cur_s.iter_mut().find(|s| s.id == *sheet_id) {
                                        sheet.content = "".to_string();
                                        sheet.is_modified = false;
                                        sheet.temp_content = Some("".to_string());
                                        sheet.temp_timestamp = Some(js_sys::Date::now() as u64);
                                        let js = sheet.to_js();
                                        let ser = serde_wasm_bindgen::Serializer::json_compatible();
                                        if let Ok(v) = js.serialize(&ser) {
                                            wasm_bindgen_futures::spawn_local(async move { let _ = save_sheet(v).await; });
                                        }
                                    }
                                    *rs.borrow_mut() = cur_s.clone();
                                    s_state.set(cur_s);
                                    set_gutter_status("none");
                                }
                            })
                        }}
                        on_delete={let ped = pending_empty_delete.clone(); let rs = sheets_ref.clone(); let s_state = sheets.clone(); let aid = active_sheet_id.clone(); let aid_ref = active_id_ref.clone(); let sp = is_suppressing_changes.clone(); let ncid = no_category_folder_id.clone(); let to = tab_order_ref.clone(); let atid = active_terminal_id.clone(); let atref = active_terminal_ref.clone(); let tsh_del = TerminalSplitHandles { ts_state: terminal_split_enabled.clone(), ts_ref: terminal_split_ref.clone(), tse_state: terminal_split_edit_mode.clone(), tse_ref: terminal_split_edit_ref.clone(), sps_state: split_pane_sheet_id.clone(), sps_ref: split_pane_sheet_id_ref.clone(), skip_fade: skip_split_fade.clone(), map: terminal_split_map.clone() }; Callback::from(move |_| {
                            if let Some(sheet_id) = (*ped).clone() {
                                ped.set(None);
                                close_tab_direct(sheet_id, rs.clone(), s_state.clone(), aid.clone(), sp.clone(), ncid.clone(), Some(aid_ref.clone()), to.borrow().clone(), Some(atid.clone()), Some(atref.clone()), Some(tsh_del.clone()));
                            }
                        })}
                    /></div>
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
                            current_theme={(*editor_theme).clone()}
                            on_change_theme={let et = editor_theme.clone(); Callback::from(move |theme: String| {
                                crate::js_interop::set_editor_theme(&theme);
                                set_account_storage(EDITOR_THEME_KEY, &theme);
                                et.set(theme);
                            })}
                            empty_save_behavior={*empty_save_behavior}
                            on_change_empty_save={let esb = empty_save_behavior.clone(); Callback::from(move |v: EmptySaveBehavior| {
                                set_account_storage(EMPTY_SAVE_KEY, v.to_str());
                                esb.set(v);
                            })}
                            window_opacity={*window_opacity}
                            on_change_opacity={if crate::js_interop::is_macos_tauri() || crate::js_interop::is_windows_tauri() { Some({
                                let wo = window_opacity.clone();
                                Callback::from(move |v: i32| {
                                    let opacity = v.clamp(50, 100);
                                    crate::js_interop::set_window_opacity(opacity as f64 / 100.0);
                                    set_account_storage(WINDOW_OPACITY_KEY, &opacity.to_string());
                                    wo.set(opacity);
                                })
                            }) } else { None }}
                            window_blur={*window_blur}
                            on_change_blur={if crate::js_interop::is_windows_tauri() { Some({
                                let wb = window_blur.clone();
                                Callback::from(move |v: i32| {
                                    let blur = v.clamp(0, 100);
                                    crate::js_interop::set_window_blur(blur);
                                    set_account_storage(WINDOW_BLUR_KEY, &blur.to_string());
                                    wb.set(blur);
                                })
                            }) } else { None }}
                            terminal_font_size={*terminal_font_size}
                            on_change_terminal_font_size={if crate::js_interop::is_tauri() { Some({
                                let tfs = terminal_font_size.clone();
                                let tfs_ref = terminal_font_size_ref.clone();
                                Callback::from(move |v: i32| {
                                    let new_size = crate::js_interop::terminal_set_font_size(v);
                                    set_account_storage(TERMINAL_FONT_SIZE_KEY, &new_size.to_string());
                                    *tfs_ref.borrow_mut() = new_size;
                                    tfs.set(new_size);
                                })
                            }) } else { None }}
                            is_guest_mode={*is_guest_mode}
                            local_auto_save={*local_auto_save}
                            on_toggle_local_auto_save={Some({
                                let las = local_auto_save.clone();
                                let las_ref = local_auto_save_ref.clone();
                                Callback::from(move |_| {
                                    let next = !*las;
                                    *las_ref.borrow_mut() = next;
                                    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
                                        let _ = storage.set_item(LOCAL_AUTO_SAVE_KEY, if next { "true" } else { "false" });
                                    }
                                    las.set(next);
                                })
                            })}
                            on_google_login={Some({
                                let sv = is_settings_visible.clone();
                                Callback::from(move |_| {
                                    sv.set(false);
                                    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
                                        let _ = storage.remove_item(GUEST_MODE_KEY);
                                    }
                                    if let Some(win) = web_sys::window() {
                                        let _ = win.location().set_href("/login");
                                    }
                                })
                            })}
                            on_close={{
                                let sv = is_settings_visible.clone();
                                let atid_settings = active_terminal_id.clone();
                                Callback::from(move |_| {
                                    sv.set(false);
                                    if let Some(ref tid) = *atid_settings {
                                        crate::js_interop::terminal_focus(tid);
                                    } else {
                                        focus_editor();
                                    }
                                })
                            }}
                        />
                    </div>
                }
                // スプリットクローズ選択ダイアログ
                if *is_split_close_dialog_visible {
                    <div class="pointer-events-auto fixed inset-0 z-[250] flex items-center justify-center">
                        // Backdrop
                        <div
                            class="absolute inset-0 bg-black/60"
                            onclick={{let d = is_split_close_dialog_visible.clone(); let atref = active_terminal_ref.clone(); move |_| {
                                d.set(false);
                                if let Some(tid) = atref.borrow().as_ref().cloned() {
                                    gloo::timers::callback::Timeout::new(50, move || { crate::js_interop::terminal_focus(&tid); }).forget();
                                }
                            }}}
                        ></div>
                        // Dialog
                        <div class="relative z-10 bg-[#1d2021] rounded-xl border border-[#3c3836] shadow-2xl w-full max-w-sm mx-4 p-6">
                            <p class="text-[#ebdbb2] text-base font-bold mb-5 text-center">{ i18n::t("split_close_which", lang) }</p>
                            <div class="flex gap-3 mb-4">
                                // ターミナルを閉じる
                                <button
                                    onclick={{
                                        let d = is_split_close_dialog_visible.clone();
                                        let atref = active_terminal_ref.clone();
                                        let tab_close = on_tab_close_cb.clone();
                                        let ts_enabled = terminal_split_enabled.clone();
                                        let ts_ref = terminal_split_ref.clone();
                                        let sp_sheet = split_pane_sheet_id.clone();
                                        let sp_sheet_ref = split_pane_sheet_id_ref.clone();
                                        let ts_map_btn = terminal_split_map.clone();
                                        move |_| {
                                            d.set(false);
                                            if let Some(tid) = atref.borrow().as_ref().cloned() {
                                                *ts_ref.borrow_mut() = false;
                                                ts_enabled.set(false);
                                                sp_sheet.set(None);
                                                *sp_sheet_ref.borrow_mut() = None;
                                                ts_map_btn.borrow_mut().insert(tid.clone(), (false, false, None));
                                                tab_close.emit(tid);
                                            }
                                        }
                                    }}
                                    class={classes!(
                                        "flex-1", "py-2", "px-3", "rounded-lg", "text-sm", "font-bold", "text-white", "transition-colors",
                                        "ring-2", "ring-offset-2", "ring-offset-[#1d2021]",
                                        if *split_close_selected == 0 { "bg-red-500 ring-red-400" } else { "bg-red-800 ring-transparent hover:bg-red-700" }
                                    )}
                                >
                                    { i18n::t("split_close_terminal", lang) }
                                </button>
                                // プレビューを閉じる
                                <button
                                    onclick={{
                                        let d = is_split_close_dialog_visible.clone();
                                        let ts_enabled = terminal_split_enabled.clone();
                                        let ts_ref = terminal_split_ref.clone();
                                        let sp_sheet = split_pane_sheet_id.clone();
                                        let sp_sheet_ref = split_pane_sheet_id_ref.clone();
                                        let atref = active_terminal_ref.clone();
                                        let ts_map_btn = terminal_split_map.clone();
                                        move |_| {
                                            d.set(false);
                                            *ts_ref.borrow_mut() = false;
                                            ts_enabled.set(false);
                                            sp_sheet.set(None);
                                            *sp_sheet_ref.borrow_mut() = None;
                                            if let Some(tid) = atref.borrow().as_ref().cloned() {
                                                ts_map_btn.borrow_mut().insert(tid.clone(), (false, false, None));
                                                gloo::timers::callback::Timeout::new(50, move || { crate::js_interop::terminal_focus(&tid); }).forget();
                                            }
                                        }
                                    }}
                                    class={classes!(
                                        "flex-1", "py-2", "px-3", "rounded-lg", "text-sm", "font-bold", "text-[#ebdbb2]", "transition-colors",
                                        "ring-2", "ring-offset-2", "ring-offset-[#1d2021]",
                                        if *split_close_selected == 1 { "bg-[#665c54] ring-[#928374]" } else { "bg-[#504945] ring-transparent hover:bg-[#665c54]" }
                                    )}
                                >
                                    { i18n::t("split_close_preview", lang) }
                                </button>
                            </div>
                            // キャンセルボタン（右下）
                            <div class="flex justify-end">
                                <button
                                    onclick={{
                                        let d = is_split_close_dialog_visible.clone();
                                        let atref = active_terminal_ref.clone();
                                        move |_| {
                                            d.set(false);
                                            if let Some(tid) = atref.borrow().as_ref().cloned() {
                                                gloo::timers::callback::Timeout::new(50, move || { crate::js_interop::terminal_focus(&tid); }).forget();
                                            }
                                        }
                                    }}
                                    class="px-4 py-1.5 rounded text-sm text-gray-400 hover:bg-[#3c3836] hover:text-white transition-colors"
                                >
                                    { i18n::t("cancel", lang) }
                                </button>
                            </div>
                        </div>
                    </div>
                }
                // タブ選択ダイアログ（デスクトップ版）
                if *is_tab_select_dialog_visible {
                    <div class="pointer-events-auto">
                        <TabSelectDialog
                            tabs={sheets.iter().map(|s| TabSelectItem { id: s.id.clone(), title: s.title.clone(), tab_color: s.tab_color.clone(), content: s.content.clone() }).collect::<Vec<_>>()}
                            on_select={{
                                let sp_sheet = split_pane_sheet_id.clone();
                                let sp_sheet_ref = split_pane_sheet_id_ref.clone();
                                let ts_enabled = terminal_split_enabled.clone();
                                let ts_ref = terminal_split_ref.clone();
                                let itsd = is_tab_select_dialog_visible.clone();
                                let atref_sel = active_terminal_ref.clone();
                                let ts_map_sel = terminal_split_map.clone();
                                let atref_sel_save = active_terminal_ref.clone();
                                Callback::from(move |id: String| {
                                    sp_sheet.set(Some(id.clone()));
                                    *sp_sheet_ref.borrow_mut() = Some(id.clone());
                                    *ts_ref.borrow_mut() = true;
                                    ts_enabled.set(true);
                                    // アクティブターミナルの ts_map にも即座に保存
                                    if let Some(tid) = atref_sel_save.borrow().as_ref().cloned() {
                                        ts_map_sel.borrow_mut().insert(tid, (true, false, Some(id)));
                                    }
                                    itsd.set(false);
                                    // スプリット表示後にターミナルへフォーカスを戻す
                                    let atref_sel2 = atref_sel.clone();
                                    gloo::timers::callback::Timeout::new(50, move || {
                                        if let Some(tid) = atref_sel2.borrow().as_ref().cloned() {
                                            crate::js_interop::terminal_focus(&tid);
                                        }
                                    }).forget();
                                })
                            }}
                            on_close={{
                                let itsd = is_tab_select_dialog_visible.clone();
                                let atref_close = active_terminal_ref.clone();
                                Callback::from(move |_| {
                                    itsd.set(false);
                                    if let Some(tid) = atref_close.borrow().as_ref().cloned() {
                                        gloo::timers::callback::Timeout::new(50, move || {
                                            crate::js_interop::terminal_focus(&tid);
                                        }).forget();
                                    }
                                })
                            }}
                        />
                    </div>
                }
            </div>
        </div>
    }
}
