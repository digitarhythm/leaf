use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;

#[wasm_bindgen(module = "/assets/js/auth.js")]
extern "C" {
    pub fn init_google_auth(client_id: &str, callback: &Closure<dyn FnMut(String)>);
    pub fn request_access_token();
    pub fn is_signed_in() -> bool;
    pub async fn sign_out();
    pub async fn fetch_user_email() -> JsValue;
    pub fn get_user_email() -> JsValue;
}
