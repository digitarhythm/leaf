use yew::prelude::*;
use crate::i18n::{self, Language};
use gloo::timers::callback::Timeout;
use wasm_bindgen::JsCast;

#[derive(Properties, PartialEq)]
pub struct SheetInfoDialogProps {
    pub on_close: Callback<()>,
    pub title: String,
    pub char_count: usize,
    pub created_at: Option<u64>,
    pub updated_at: Option<u64>,
    pub needs_bom: bool,
    pub category_name: String,
}

fn format_ts(ts: Option<u64>, lang: Language) -> String {
    match ts {
        None => i18n::t("not_available", lang).to_string(),
        Some(ms) => {
            let secs = (ms / 1000) as f64;
            let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(secs * 1000.0));
            let y = date.get_full_year();
            let mo = date.get_month() + 1;
            let d = date.get_date();
            let h = date.get_hours();
            let mi = date.get_minutes();
            format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, mi)
        }
    }
}

#[function_component(SheetInfoDialog)]
pub fn sheet_info_dialog(props: &SheetInfoDialogProps) -> Html {
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
    let encoding = if props.needs_bom { "UTF-8 (BOM)" } else { "UTF-8" };

    let updated_ts = props.updated_at;
    let created_ts = props.created_at;
    let char_count = props.char_count;
    let category_name = props.category_name.clone();
    let encoding_str = encoding.to_string();

    let rows = vec![
        (i18n::t("info_char_count", lang).to_string(), format!("{}", char_count)),
        (i18n::t("info_created_at", lang).to_string(), format_ts(created_ts, lang)),
        (i18n::t("info_updated_at", lang).to_string(), format_ts(updated_ts, lang)),
        (i18n::t("info_encoding", lang).to_string(), encoding_str),
        (i18n::t("info_directory", lang).to_string(), category_name),
    ];

    html! {
        <div class="fixed inset-0 z-[200] flex items-center justify-center">
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
                "relative", "z-10", "w-full", "max-w-sm", "mx-4",
                "bg-[#1d2021]", "rounded-xl", "border", "border-[#3c3836]", "shadow-2xl",
                "transition-all", "duration-300", anim_class
            )}>
                // Header
                <div class="flex items-center justify-between px-6 py-4 border-b border-[#3c3836]">
                    <div class="flex items-center gap-3">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24"
                             stroke-width="1.5" stroke="currentColor" class="w-5 h-5 text-emerald-500">
                            <path stroke-linecap="round" stroke-linejoin="round"
                                  d="M11.25 11.25l.041-.02a.75.75 0 011.063.852l-.708 2.836a.75.75 0 001.063.853l.041-.021M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9-3.75h.008v.008H12V8.25z" />
                        </svg>
                        <h2 class="text-base font-bold text-[#ebdbb2]">{ i18n::t("sheet_info_title", lang) }</h2>
                    </div>
                    <button
                        onclick={{let hc = handle_close.clone(); move |_| hc.emit(())}}
                        class="p-1 rounded hover:bg-[#3c3836] text-gray-400 hover:text-white transition-colors"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24"
                             stroke-width="2" stroke="currentColor" class="w-5 h-5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                // Content
                <div class="px-6 py-4 space-y-3">
                    <div class="flex flex-col gap-0.5 mb-2">
                        <span class="text-[10px] font-bold text-emerald-500/80 uppercase tracking-widest">{ i18n::t("info_filename", lang) }</span>
                        <div class="text-sm text-[#d4be98] font-mono truncate">{ props.title.clone() }</div>
                    </div>
                    { for rows.iter().map(|(label, value)| html! {
                        <div class="flex flex-col gap-0.5">
                            <span class="text-[10px] font-bold text-emerald-500/80 uppercase tracking-widest">{ label.clone() }</span>
                            <span class="text-[13px] text-gray-300 font-mono break-all">{ value.clone() }</span>
                        </div>
                    }) }
                </div>

                // Footer
                <div class="px-6 py-4 border-t border-[#3c3836] flex justify-end">
                    <button
                        onclick={{let hc = handle_close.clone(); move |_| hc.emit(())}}
                        class="px-6 py-2 rounded-lg text-sm font-bold bg-emerald-600 hover:bg-emerald-700 text-white transition-colors"
                    >
                        { "OK" }
                    </button>
                </div>
            </div>
        </div>
    }
}
