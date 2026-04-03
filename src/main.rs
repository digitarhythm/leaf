mod app;
mod components;
mod js_interop;
mod auth_interop;
mod db_interop;
mod drive_interop;
mod i18n;

use app::App;

fn main() {
    // Set up panic hook to log errors to the browser console
    console_error_panic_hook::set_once();
    
    yew::Renderer::<App>::new().render();
}
