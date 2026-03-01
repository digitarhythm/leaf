use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/adsense.js")]
extern "C" {
    pub fn load_adsense_script();
    pub fn render_ad(container_id: &str);
    pub fn remove_ad(container_id: &str);
}
