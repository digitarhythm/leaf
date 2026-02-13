use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;

#[wasm_bindgen(module = "/editor_interop.js")]
extern "C" {
    pub fn set_window_title(title: &str);
    pub fn init_editor(element_id: &str, callback: &Closure<dyn FnMut(String)>);
    pub fn set_vim_mode(enabled: bool);
    pub fn set_editor_content(content: &str);
    pub fn get_editor_content() -> JsValue;
    pub fn resize_editor();
    pub fn focus_editor();
    pub fn set_gutter_status(unsaved: bool);
    pub fn generate_uuid() -> String;
    pub fn change_font_size(delta: i32);
    pub fn render_markdown(text: &str) -> String;
    pub fn init_mermaid(element: &web_sys::Element);
    pub fn set_preview_active(active: bool);
    pub fn set_editor_mode(filename: &str);
}
