use yew::prelude::*;
use crate::js_interop::{render_markdown, highlight_code};
use crate::i18n::{self, Language};
use wasm_bindgen::JsCast;
use gloo::timers::callback::Timeout;

#[derive(Properties, PartialEq)]
pub struct PreviewProps {
    pub content: String,
    pub on_close: Callback<()>,
    #[prop_or_default]
    pub lang: String, // ファイル拡張子
    #[prop_or_default]
    pub has_more: bool,
    #[prop_or_default]
    pub is_loading: bool,
    #[prop_or_default]
    pub on_install: Option<Callback<()>>,
    #[prop_or_default]
    pub is_help: bool,
    #[prop_or_default]
    pub is_sub_dialog_open: bool,
    #[prop_or_default]
    pub is_fading_out: bool,
    #[prop_or(14)]
    pub font_size: i32,
    #[prop_or_default]
    pub on_change_font_size: Callback<i32>,
    #[prop_or_default]
    pub is_embedded: bool, // ダイアログ内埋め込みモード
    #[prop_or_default]
    pub close_on_space: bool, // スペースキーで閉じるか
}

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = initMermaid)]
    fn init_mermaid(el: &web_sys::Element);
}

#[function_component(Preview)]
pub fn preview(props: &PreviewProps) -> Html {
    let lang = Language::detect();
    let node_ref = use_node_ref();
    let is_closing = use_state(|| false);
    let is_fading_out = *is_closing || props.is_fading_out;

    let is_sub_dialog_open = props.is_sub_dialog_open;

    let handle_close = {
        let on_close = props.on_close.clone();
        let is_closing = is_closing.clone();
        Callback::from(move |_: ()| {
            is_closing.set(true);
            let on_close = on_close.clone();
            Timeout::new(100, move || {
                on_close.emit(());
            }).forget();
        })
    };

    // Mermaidの適用とフォーカス制御
    {
        let content = props.content.clone();
        let node_ref = node_ref.clone();
        use_effect_with(content, move |_| {
            if let Some(div) = node_ref.cast::<web_sys::Element>() {
                let div_c = div.clone();
                gloo::timers::callback::Timeout::new(100, move || {
                    let _ = init_mermaid(&div_c);
                    if let Some(html_el) = div_c.dyn_ref::<web_sys::HtmlElement>() {
                        if html_el.get_attribute("tabindex").map(|t| t == "0").unwrap_or(false) {
                            let _ = html_el.focus();
                        }
                    }
                }).forget();
            }
            || ()
        });
    }

    // キーボード操作
    {
        let node_ref = node_ref.clone();
        let on_close_cb = handle_close.clone();
        let is_help_mode = props.is_help;
        let on_change_font_size = props.on_change_font_size.clone();
        let is_embedded = props.is_embedded;
        let close_on_space = props.close_on_space;
        use_effect_with((is_help_mode, is_sub_dialog_open, on_change_font_size, is_embedded, close_on_space), move |deps| {
            let (is_help_mode, is_sub_open, on_change_fs, is_embedded, close_on_space) = deps.clone();
            if is_embedded { return Box::new(|| ()) as Box<dyn FnOnce()>; }

            let on_close = on_close_cb.clone();
            let window = web_sys::window().unwrap();
            let mut opts = gloo::events::EventListenerOptions::run_in_capture_phase();
            opts.passive = false;
            let listener = gloo::events::EventListener::new_with_options(&window, "keydown", opts, move |e| {
                if is_sub_open { return; }
                let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                let key = ke.key();
                let code = ke.code();
                let is_up = key == "PageUp" || key == "RollUp";
                let is_down = key == "PageDown" || key == "RollDown";
                let is_arrow_up = key == "ArrowUp";
                let is_arrow_down = key == "ArrowDown";
                let is_space = key == " ";
                let is_home = key == "Home";
                let is_end = key == "End";
                e.stop_immediate_propagation();
                
                // ESCキー または (設定されている場合の) スペースキーで閉じる
                if (is_space && close_on_space) || key == "Escape" { e.prevent_default(); on_close.emit(()); return; }
                
                let is_l_key = code == "KeyL" || key.to_lowercase() == "l" || key == "¬";
                let is_h_key = code == "KeyH" || key.to_lowercase() == "h" || key == "˙";
                let is_target_key = if is_help_mode { is_h_key } else if close_on_space { false } else { is_l_key };
                if ke.alt_key() && is_target_key { e.prevent_default(); on_close.emit(()); return; }
                if key == "Tab" { e.prevent_default(); return; }
                if ke.alt_key() && !is_help_mode {
                    if code == "Equal" || key == "=" || key == "+" || key == "≠" { e.prevent_default(); on_change_fs.emit(1); return; }
                    if code == "Minus" || key == "-" || key == "–" { e.prevent_default(); on_change_fs.emit(-1); return; }
                }
                if is_up || is_down || is_arrow_up || is_arrow_down || is_home || is_end || is_space {
                    if let Some(el) = node_ref.cast::<web_sys::Element>() {
                        e.prevent_default();
                        let client_height = el.client_height();
                        let current_scroll = el.scroll_top();
                        if is_up { el.set_scroll_top(current_scroll - client_height / 2); } 
                        else if is_down || is_space { el.set_scroll_top(current_scroll + client_height / 2); } 
                        else if is_arrow_up { el.set_scroll_top(current_scroll - 40); } 
                        else if is_arrow_down { el.set_scroll_top(current_scroll + 40); } 
                        else if is_home { el.set_scroll_top(0); } 
                        else if is_end { el.set_scroll_top(el.scroll_height()); }
                    }
                    return;
                }
                let is_printable = key.len() == 1;
                if is_printable || key.starts_with("F") { e.prevent_default(); }
            });
            Box::new(move || { drop(listener); }) as Box<dyn FnOnce()>
        });
    }

    let is_markdown = props.lang == "md" || props.lang == "markdown";
    let rendered_html = if is_markdown {
        render_markdown(&props.content)
    } else {
        // highlight.js を使用してハイライト済みの HTML 文字列を生成
        let code_html = highlight_code(&props.content, &props.lang);
        format!(r#"<pre class="hljs whitespace-pre-wrap break-all"><code class="hljs language-{}">{}</code></pre>"#, props.lang, code_html)
    };

    let content_node = html! {
        <div 
            ref={node_ref.clone()}
            tabindex={if props.is_embedded { "-1" } else { "0" }}
            class={classes!(
                if is_markdown { "markdown-body" } else { "hljs" },
                "max-w-none", "outline-none", "relative",
                if props.is_embedded { "flex-1 overflow-auto custom-scrollbar bg-[#1a1b26] p-6 text-xs" } else { "overflow-y-auto p-6 sm:p-12 bg-[#1a1b26]" }
            )}
            style={if props.is_embedded { "".to_string() } else { format!("font-size: {}pt;", props.font_size) }}
        >
            if props.is_loading {
                <div class={classes!(
                    "flex", "flex-col", "items-center", "justify-center", "space-y-4",
                    if props.is_embedded { vec!["absolute", "inset-0", "z-50", "bg-gray-950"] } else { vec!["min-h-[200px]"] }
                )}>
                    <div class="w-8 h-8 border-2 border-emerald-500/30 border-t-emerald-500 rounded-full animate-spin"></div>
                    <span class="text-[10px] text-emerald-500/50 font-bold uppercase tracking-widest animate-pulse">{ i18n::t("fetching_preview", lang) }</span>
                </div>
            }

            if !props.content.is_empty() {
                { Html::from_html_unchecked(AttrValue::from(rendered_html)) }
                if props.has_more {
                    <>
                        if !props.is_embedded { <div class="h-32 -mt-32 bg-gradient-to-t from-[#0d1117] via-[#0d1117]/80 to-transparent pointer-events-none relative z-10"></div> }
                        <div class="py-8 text-center text-gray-500 font-mono whitespace-pre-wrap leading-relaxed opacity-60 relative z-20">
                            { i18n::t("omitted_below", lang) }
                        </div>
                    </>
                }
            }
        </div>
    };

    if props.is_embedded {
        return content_node;
    }

    html! {
        <div class="fixed inset-0 z-[300] flex items-center justify-center p-4 sm:p-8">
            <div class={classes!(
                "absolute", "inset-0", "bg-black",
                if is_fading_out { "animate-backdrop-out" } else { "animate-backdrop-in" }
            )} onclick={let cb = handle_close.clone(); move |_| cb.emit(())}></div>
            <div
                class={classes!(
                    "w-full", "max-w-5xl", "max-h-full", "bg-[#0d1117]", "rounded-xl", "shadow-2xl", "border-2", "border-emerald-500", "flex", "flex-col", "overflow-hidden", "relative",
                    if props.is_help { if is_fading_out { "animate-help-out" } else { "animate-help-in" } } 
                    else { if is_fading_out { "animate-dialog-out" } else { "animate-dialog-in" } }
                )}
                onclick={|e: MouseEvent| e.stop_propagation()}
            >
                { content_node }
                
                if props.on_install.is_some() || props.is_help {
                    <div class="px-6 py-4 bg-gray-900/50 border-t border-gray-800 flex flex-col items-center space-y-4 relative">
                        if props.is_help {
                            <div class="absolute left-6 bottom-4 md:bottom-auto md:top-1/2 md:-translate-y-1/2 text-xs text-gray-600 font-mono tracking-widest whitespace-nowrap">
                                { format!("ver {}", env!("CARGO_PKG_VERSION")) }
                            </div>
                        }
                        if let Some(on_install) = &props.on_install {
                            <button onclick={on_install.reform(|_| ())} class="px-8 py-3 bg-lime-600 hover:bg-lime-700 text-white font-bold rounded-lg shadow-lg transition-all flex items-center space-x-2">
                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6"><path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3" /></svg>
                                <span>{ i18n::t("install_app_button", lang) }</span>
                            </button>
                        }
                        if props.is_help {
                            <div class="flex items-center space-x-6 text-xs text-gray-500">
                                <a href="terms.html" target="_blank" class="hover:text-emerald-400 transition-colors underline underline-offset-4 decoration-gray-700">{ i18n::t("terms_of_service", lang) }</a>
                                <a href="privacy.html" target="_blank" class="hover:text-emerald-400 transition-colors underline underline-offset-4 decoration-gray-700">{ i18n::t("privacy_policy", lang) }</a>
                                <a href="licenses.html" target="_blank" class="hover:text-emerald-400 transition-colors underline underline-offset-4 decoration-gray-700">{ i18n::t("oss_licenses", lang) }</a>
                                <a href={format!("features_{}.html", match lang { Language::Ja => "ja", Language::Zh => "zh", Language::Ko => "ko", Language::Es => "es", Language::De => "de", Language::Fr => "fr", Language::It => "it", Language::Nl => "nl", _ => "en" })} target="_blank" class="hover:text-emerald-400 transition-colors underline underline-offset-4 decoration-gray-700">{ i18n::t("tutorial", lang) }</a>
                            </div>
                        }
                    </div>
                }
            </div>
            <div class="fixed top-4 right-4 text-gray-400 text-[10px] bg-black/60 px-3 py-1.5 rounded-full border border-white/10 backdrop-blur-md flex items-center space-x-4">
                <div class="flex items-center space-x-2">
                    <span class="bg-gray-700 px-1.5 py-0.5 rounded text-gray-200">{"ESC"}</span>
                    <span>{ i18n::t("close", lang) }</span>
                </div>
            </div>
        </div>
    }
}
