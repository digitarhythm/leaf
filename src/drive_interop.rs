use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/drive.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    pub async fn find_or_create_folder(folder_name: &str, parent_id: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn ensure_directory_structure() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn list_folders(parent_id: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn upload_file(filename: &str, content: &str, folder_id: &str, file_id: Option<&str>) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn list_files(folder_id: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn download_file(file_id: &str, range: Option<&str>) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn get_file_metadata(file_id: &str) -> Result<JsValue, JsValue>;

    pub fn parse_date(date_str: &str) -> f64;
}
