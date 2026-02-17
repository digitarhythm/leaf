use yew::prelude::*;
use crate::js_interop::{render_markdown, init_mermaid};
use crate::i18n::{self, Language};
use wasm_bindgen::JsCast;

#[derive(Properties, PartialEq)]
pub struct PreviewProps {
    pub content: String,
    pub on_close: Callback<()>,
    #[prop_or_default]
    pub on_install: Option<Callback<()>>,
    #[prop_or_default]
    pub on_load_more: Option<Callback<()>>,
    #[prop_or_default]
    pub has_more: bool,
    #[prop_or_default]
    pub is_loading: bool,
    #[prop_or_default]
    pub disable_space_scroll: bool,
    #[prop_or_default]
    pub is_help: bool,
}

#[function_component(Preview)]
pub fn preview(props: &PreviewProps) -> Html {
    let lang = Language::detect();
    let node_ref = use_node_ref();

    {
        let content = props.content.clone();
        let node_ref = node_ref.clone();
        use_effect_with(content, move |_| {
            if let Some(div) = node_ref.cast::<web_sys::Element>() {
                // DOM の更新を待つために少し遅延させる
                gloo::timers::callback::Timeout::new(50, move || {
                    let _ = init_mermaid(&div);
                }).forget();
            }
            || ()
        });
    }

    // スクロール監視
    {
        let node_ref = node_ref.clone();
        let on_load_more = props.on_load_more.clone();
        let has_more = props.has_more;
        use_effect_with((node_ref, has_more), move |(node, more)| {
            let mut _listener = None;
            if *more {
                if let Some(el) = node.get() {
                    let on_load = on_load_more.clone();
                    _listener = Some(gloo::events::EventListener::new(&el, "scroll", move |e| {
                        if let Some(target_el) = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                            let scroll_top = target_el.scroll_top();
                            let scroll_height = target_el.scroll_height();
                            let client_height = target_el.client_height();
                            
                            // 下端から 200px 以内になったら追加読み込み
                            if scroll_top + client_height >= scroll_height - 200 {
                                if let Some(cb) = &on_load {
                                    cb.emit(());
                                }
                            }
                        }
                    }));
                }
            }
            move || { drop(_listener); }
        });
    }

    // キーボード操作
    {
        let node_ref = node_ref.clone();
        let disable_space = props.disable_space_scroll;
        let on_close = props.on_close.clone();
        let is_help_mode = props.is_help;
        use_effect_with((disable_space, is_help_mode), move |deps| {
            let (disable_space, is_help_mode) = *deps;
            let on_close = on_close.clone();
            let window = web_sys::window().unwrap();
            let mut opts = gloo::events::EventListenerOptions::run_in_capture_phase();
            opts.passive = false;
            let listener = gloo::events::EventListener::new_with_options(&window, "keydown", opts, move |e| {
                let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                let key = ke.key();
                
                // ナビゲーションキーの判定
                let is_up = key == "PageUp" || key == "RollUp";
                let is_down = key == "PageDown" || key == "RollDown";
                let is_arrow_up = key == "ArrowUp";
                let is_arrow_down = key == "ArrowDown";
                let is_space = key == " " && !disable_space;
                let is_home = key == "Home";
                let is_end = key == "End";

                // ショートカットトグル (適切なキーのみに反応)
                let is_target_key = if is_help_mode {
                    key.to_lowercase() == "h" || key == "˙"
                } else {
                    key.to_lowercase() == "l" || key == "¬"
                };
                let is_alt_toggle = ke.alt_key() && is_target_key;

                if is_alt_toggle {
                    e.prevent_default();
                    e.stop_immediate_propagation();
                    on_close.emit(());
                    return;
                }

                if is_up || is_down || is_arrow_up || is_arrow_down || is_space || is_home || is_end {
                    if let Some(el) = node_ref.cast::<web_sys::Element>() {
                        e.prevent_default();
                        e.stop_immediate_propagation();

                        let client_height = el.client_height();
                        let current_scroll = el.scroll_top();
                        
                        if is_up {
                            el.set_scroll_top(current_scroll - client_height / 2);
                        } else if is_down || is_space {
                            el.set_scroll_top(current_scroll + client_height / 2);
                        } else if is_arrow_up {
                            el.set_scroll_top(current_scroll - 40);
                        } else if is_arrow_down {
                            el.set_scroll_top(current_scroll + 40);
                        } else if is_home {
                            el.set_scroll_top(0);
                        } else if is_end {
                            el.set_scroll_top(el.scroll_height());
                        }
                    }
                }
            });
            move || { drop(listener); }
        });
    }

    let rendered_html = render_markdown(&props.content);

    html! {
        <div class="fixed inset-0 z-[300] bg-black/80 flex items-center justify-center p-4 sm:p-8 animate-in fade-in duration-200" onclick={props.on_close.reform(|_| ())}>
            <div class="w-full max-w-5xl max-h-full bg-[#0d1117] rounded-xl shadow-2xl border border-gray-800 flex flex-col overflow-hidden relative" onclick={|e: MouseEvent| e.stop_propagation()}>
                <div 
                    ref={node_ref}
                    class="markdown-body max-w-none overflow-y-auto p-6 sm:p-12"
                >
                    { Html::from_html_unchecked(AttrValue::from(rendered_html)) }
                </div>
                
                if props.is_loading {
                    <div class="absolute inset-0 z-50 flex items-center justify-center bg-black/30">
                        <div class="w-12 h-12 border-4 border-lime-500 border-t-transparent rounded-full animate-spin"></div>
                    </div>
                }
                
                if let Some(on_install) = &props.on_install {
                    <div class="px-6 py-4 bg-gray-900/50 border-t border-gray-800 flex justify-center">
                        <button 
                            onclick={on_install.reform(|_| ())}
                            class="px-8 py-3 bg-lime-600 hover:bg-lime-700 text-white font-bold rounded-lg shadow-lg transition-all flex items-center space-x-2"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3" />
                            </svg>
                            <span>{ i18n::t("install_app_button", lang) }</span>
                        </button>
                    </div>
                }
            </div>
            <div class="fixed top-4 right-4 text-gray-400 text-[10px] bg-black/60 px-3 py-1.5 rounded-full border border-white/10 backdrop-blur-md flex items-center space-x-4">
                if props.on_install.is_some() {
                    <span class="text-lime-500 font-bold">{ i18n::t("scroll_for_install", lang) }</span>
                    <span class="text-gray-600">{ "|" }</span>
                }
                <span>{ i18n::t("close_guide", lang) }</span>
            </div>
        </div>
    }
}
