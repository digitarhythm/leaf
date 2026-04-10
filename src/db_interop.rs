use wasm_bindgen::prelude::*;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct JSSheet {
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
    #[serde(default)]
    pub total_size: u64,
    #[serde(default)]
    pub loaded_bytes: u64,
    #[serde(default = "default_true")]
    pub needs_bom: bool,
    #[serde(default)]
    pub is_preview: bool,
    #[serde(default)]
    pub created_at: Option<u64>,
}

fn default_true() -> bool { true }

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct JSCategory {
    pub id: String,
    pub name: String,
}

#[wasm_bindgen(module = "/db.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    pub async fn init_db(db_name: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn save_sheet(sheet: JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn load_sheets() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn delete_sheet(id: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn save_categories(categories: JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn load_categories() -> Result<JsValue, JsValue>;

    pub fn close_db();
}
