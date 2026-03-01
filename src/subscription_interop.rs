use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/subscription.js")]
extern "C" {
    pub async fn check_subscription_status() -> JsValue;
    pub fn get_cached_subscription_status() -> JsValue;
    pub fn clear_subscription_cache();
}
