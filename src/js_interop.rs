use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;

#[wasm_bindgen(module = "/editor_interop.js")]
extern "C" {
    pub fn set_window_title(title: &str);
    pub fn init_editor(element_id: &str, callback: &Closure<dyn FnMut(String)>);
    pub fn set_vim_mode(enabled: bool);
    pub fn set_editor_content(content: &str);
    pub fn get_editor_content() -> String;
    pub fn resize_editor();
    pub fn generate_uuid() -> String;
}
