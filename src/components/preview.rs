use yew::prelude::*;
use crate::js_interop::{render_markdown, init_mermaid};

#[derive(Properties, PartialEq)]
pub struct PreviewProps {
    pub content: String,
    pub on_close: Callback<()>,
}

#[function_component(Preview)]
pub fn preview(props: &PreviewProps) -> Html {
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
        <div class="fixed inset-0 z-[300] bg-gray-950/90 overflow-y-auto p-4 sm:p-8 animate-in fade-in duration-200" onclick={props.on_close.reform(|_| ())}>
            <div class="max-w-4xl mx-auto bg-gray-900 p-6 sm:p-10 rounded-xl shadow-2xl border border-gray-800 mb-12" onclick={|e: MouseEvent| e.stop_propagation()}>
                <div 
                    ref={node_ref}
                    class="markdown-body prose prose-invert prose-lime max-w-none text-gray-100"
                >
                    { Html::from_html_unchecked(AttrValue::from(rendered_html)) }
                </div>
            </div>
            <div class="fixed top-4 right-4 text-gray-400 text-[10px] bg-black/60 px-3 py-1.5 rounded-full border border-white/10 backdrop-blur-md">
                { "ESC or Click outside to close" }
            </div>
        </div>
    }
}
