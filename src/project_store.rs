//! `.leaf_projects.json` の Google ドライブとの読み書き。
//!
//! ロジックは [`crate::project`] 側に置き、このモジュールは I/O のみを担当する。
//! 設定ファイルは Leaf アプリケーションフォルダ直下に置くため、
//! カテゴリーにもシート一覧にも現れない。

use crate::drive_interop::{download_file, find_file_by_name, upload_file};
use crate::project::{ProjectStore, PROJECTS_FILE_NAME};
use wasm_bindgen::JsValue;

/// 設定ファイルの Drive 上のファイル ID を探す。未作成なら None。
async fn find_projects_file_id(app_folder_id: &str) -> Option<String> {
    let found = find_file_by_name(PROJECTS_FILE_NAME, app_folder_id).await.ok()?;
    if found.is_null() || found.is_undefined() {
        return None;
    }
    js_sys::Reflect::get(&found, &JsValue::from_str("id"))
        .ok()
        .and_then(|v| v.as_string())
}

/// 設定ファイルを読み込む。
///
/// 未作成・取得失敗・JSON 破損のいずれでも空のストアを返す。
/// ここでエラーを表面化させると起動やダイアログ表示ごと失敗してしまい、
/// かえって復旧しにくくなるため。
pub async fn load(app_folder_id: &str) -> ProjectStore {
    let file_id = match find_projects_file_id(app_folder_id).await {
        Some(id) => id,
        None => return ProjectStore::new(),
    };
    let bytes_js = match download_file(&file_id, None, None).await {
        Ok(v) => v,
        Err(_) => return ProjectStore::new(),
    };
    let bytes = js_sys::Uint8Array::new(&bytes_js).to_vec();
    match String::from_utf8(bytes) {
        Ok(json) => ProjectStore::from_json(&json),
        Err(_) => ProjectStore::new(),
    }
}

/// 設定ファイルを保存する（既存があれば上書き、無ければ新規作成）。
/// 保存できたかどうかを返す。
pub async fn save(app_folder_id: &str, store: &ProjectStore) -> bool {
    let json = store.to_json();
    let existing = find_projects_file_id(app_folder_id).await;
    upload_file(
        PROJECTS_FILE_NAME,
        &JsValue::from_str(&json),
        app_folder_id,
        existing.as_deref(),
    )
    .await
    .is_ok()
}
