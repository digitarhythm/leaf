use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;

#[wasm_bindgen(module = "/editor_interop.js")]
extern "C" {
    pub fn set_window_title(title: &str);
    pub fn init_editor(element_id: &str, callback: &Closure<dyn FnMut(String)>);
    pub fn set_vim_mode(enabled: bool);
    pub fn set_editor_content(content: &str);
    pub fn load_editor_content(content: &str);
    pub fn append_editor_content(content: &str);
    pub fn get_editor_content() -> JsValue;
    pub fn resize_editor();
    pub fn focus_editor();
    pub fn set_gutter_status(mode: &str);
    pub fn generate_uuid() -> String;
    pub fn get_font_size() -> i32;
    pub fn change_font_size(delta: i32) -> i32;
    pub fn render_markdown(text: &str) -> String;
    pub fn highlight_code(code: &str, lang: &str) -> String;
    pub fn init_mermaid(element: &web_sys::Element);
    pub fn set_preview_active(active: bool);
    pub fn set_editor_mode(filename: &str);
    pub fn set_editor_theme(theme_name: &str);
    pub fn exec_editor_command(command: &str);

    pub fn is_tauri() -> bool;
    pub fn get_safe_chunk(data: &JsValue) -> JsValue;

    pub fn can_install_pwa() -> bool;
    pub async fn trigger_pwa_install() -> JsValue;
    pub fn is_webkit_or_safari() -> bool;

    pub async fn open_local_file() -> JsValue;
    pub async fn save_local_file(content: &str, needs_bom: bool) -> JsValue;
    pub fn clear_local_handle();
    pub fn scroll_into_view_graceful(container: &web_sys::Element, index: u32, duration_ms: f64);
}
