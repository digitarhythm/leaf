use yew::prelude::*;
use crate::js_interop::{render_markdown, init_mermaid};
use crate::i18n::{self, Language};

#[derive(Properties, PartialEq)]
pub struct PreviewProps {
    pub content: String,
    pub on_close: Callback<()>,
    #[prop_or_default]
    pub on_install: Option<Callback<()>>,
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

    let rendered_html = render_markdown(&props.content);

    html! {
        <div class="fixed inset-0 z-[300] bg-black/80 flex items-center justify-center p-4 sm:p-8 animate-in fade-in duration-200" onclick={props.on_close.reform(|_| ())}>
            <div class="w-full max-w-5xl max-h-full bg-[#0d1117] rounded-xl shadow-2xl border border-gray-800 flex flex-col overflow-hidden" onclick={|e: MouseEvent| e.stop_propagation()}>
                <div 
                    ref={node_ref}
                    class="markdown-body max-w-none overflow-y-auto p-6 sm:p-12"
                >
                    { Html::from_html_unchecked(AttrValue::from(rendered_html)) }
                </div>
                
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
