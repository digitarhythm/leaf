use yew::prelude::*;
use crate::i18n::{self, Language};
use gloo::timers::callback::Timeout;
use wasm_bindgen::JsCast;

#[derive(Clone, PartialEq)]
pub struct TabSelectItem {
    pub id: String,
    pub title: String,
    pub tab_color: String,
}

#[derive(Properties, PartialEq)]
pub struct TabSelectDialogProps {
    pub tabs: Vec<TabSelectItem>,
    pub on_select: Callback<String>,
    pub on_close: Callback<()>,
}

#[function_component(TabSelectDialog)]
pub fn tab_select_dialog(props: &TabSelectDialogProps) -> Html {
    let lang = Language::detect();
    let is_closing = use_state(|| false);

    let handle_close = {
        let on_close = props.on_close.clone();
        let is_closing = is_closing.clone();
        Callback::from(move |_: ()| {
            is_closing.set(true);
            let cb = on_close.clone();
            Timeout::new(300, move || { cb.emit(()); }).forget();
        })
    };

    // ESCキーで閉じる
    {
        let hc = handle_close.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let mut opts = gloo::events::EventListenerOptions::run_in_capture_phase();
            opts.passive = false;
            let listener = gloo::events::EventListener::new_with_options(&window, "keydown", opts, move |e| {
                let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                if ke.key() == "Escape" {
                    e.stop_immediate_propagation();
                    hc.emit(());
                }
            });
            Box::new(move || drop(listener)) as Box<dyn FnOnce()>
        });
    }

    let anim_class = if *is_closing { "opacity-0 scale-95" } else { "opacity-100 scale-100" };
    let title = i18n::t("select_tab_to_preview", lang);

    html! {
        <div class="fixed inset-0 z-[250] flex items-center justify-center">
            // Backdrop
            <div
                class={classes!(
                    "absolute", "inset-0", "bg-black/60", "transition-opacity", "duration-300",
                    if *is_closing { "opacity-0" } else { "opacity-100" }
                )}
                onclick={{let hc = handle_close.clone(); move |_| hc.emit(())}}
            ></div>

            // Dialog
            <div class={classes!(
                "relative", "z-10", "w-full", "max-w-lg", "mx-4",
                "bg-[#1d2021]", "rounded-xl", "border", "border-[#3c3836]", "shadow-2xl",
                "transition-all", "duration-300", anim_class
            )}>
                // Header
                <div class="flex items-center justify-between px-6 py-4 border-b border-[#3c3836]">
                    <div class="flex items-center gap-3">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5 text-emerald-500">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M2.25 7.125C2.25 6.504 2.754 6 3.375 6h6c.621 0 1.125.504 1.125 1.125v3.75c0 .621-.504 1.125-1.125 1.125h-6a1.125 1.125 0 01-1.125-1.125v-3.75zM14.25 8.625c0-.621.504-1.125 1.125-1.125h5.25c.621 0 1.125.504 1.125 1.125v8.25c0 .621-.504 1.125-1.125 1.125h-5.25a1.125 1.125 0 01-1.125-1.125v-8.25zM3.75 16.125c0-.621.504-1.125 1.125-1.125h5.25c.621 0 1.125.504 1.125 1.125v2.25c0 .621-.504 1.125-1.125 1.125h-5.25a1.125 1.125 0 01-1.125-1.125v-2.25z" />
                        </svg>
                        <h2 class="text-base font-bold text-[#ebdbb2]">{ title }</h2>
                    </div>
                    <button
                        onclick={{let hc = handle_close.clone(); move |_| hc.emit(())}}
                        class="p-1 rounded hover:bg-[#3c3836] text-gray-400 hover:text-white transition-colors"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-5 h-5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                // Tab list
                <div class="overflow-y-auto max-h-[60vh] py-2">
                    { for props.tabs.iter().map(|tab| {
                        let id = tab.id.clone();
                        let on_select = props.on_select.clone();
                        let hc = handle_close.clone();
                        let color = tab.tab_color.clone();
                        html! {
                            <button
                                class="w-full flex items-center gap-3 px-5 py-3 hover:bg-[#282828] transition-colors text-left"
                                onclick={Callback::from(move |_| {
                                    on_select.emit(id.clone());
                                    hc.emit(());
                                })}
                            >
                                <div class="w-2 h-2 rounded-full flex-shrink-0" style={format!("background-color: {};", color)}></div>
                                <span class="text-sm text-[#d4be98] truncate">{ &tab.title }</span>
                            </button>
                        }
                    }) }
                    if props.tabs.is_empty() {
                        <div class="px-5 py-8 text-center text-xs text-gray-500">
                            { i18n::t("no_tabs_open", lang) }
                        </div>
                    }
                </div>
            </div>
        </div>
    }
}
