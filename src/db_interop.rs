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
}
