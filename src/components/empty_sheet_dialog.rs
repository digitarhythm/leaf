use yew::prelude::*;
use gloo::timers::callback::Timeout;
use crate::i18n;
use crate::i18n::Language;

#[derive(Properties, PartialEq)]
pub struct EmptySheetDialogProps {
    pub on_cancel: Callback<()>,
    pub on_save: Callback<()>,
    pub on_delete: Callback<()>,
    pub lang: Language,
}

#[derive(PartialEq, Clone, Copy)]
enum Focus {
    Cancel,
    Save,
    Delete,
}

#[function_component(EmptySheetDialog)]
pub fn empty_sheet_dialog(props: &EmptySheetDialogProps) -> Html {
    let focused = use_state(|| Focus::Cancel);
    let is_fading_out = use_state(|| false);
    let root_ref = use_node_ref();

    {
        let root = root_ref.clone();
        use_effect_with((), move |_| {
            let r = root.clone();
            Timeout::new(10, move || { if let Some(el) = r.cast::<web_sys::HtmlElement>() { let _ = el.focus(); } }).forget();
            || ()
        });
    }

    let on_cancel = {
        let cb = props.on_cancel.clone();
        let fo = is_fading_out.clone();
        Callback::from(move |_: ()| { fo.set(true); let c = cb.clone(); Timeout::new(300, move || c.emit(())).forget(); })
    };
    let on_save = {
        let cb = props.on_save.clone();
        let fo = is_fading_out.clone();
        Callback::from(move |_: ()| { fo.set(true); let c = cb.clone(); Timeout::new(300, move || c.emit(())).forget(); })
    };
    let on_delete = {
        let cb = props.on_delete.clone();
        let fo = is_fading_out.clone();
        Callback::from(move |_: ()| { fo.set(true); let c = cb.clone(); Timeout::new(300, move || c.emit(())).forget(); })
    };

    let on_keydown = {
        let focused = focused.clone();
        let on_cxl = on_cancel.clone();
        let on_sav = on_save.clone();
        let on_del = on_delete.clone();
        Callback::from(move |e: web_sys::KeyboardEvent| {
            e.stop_propagation();
            match e.key().as_str() {
                "Escape" => { e.prevent_default(); on_cxl.emit(()); }
                "Tab" | "ArrowLeft" | "ArrowRight" => {
                    e.prevent_default();
                    let next = match *focused {
                        Focus::Cancel => if e.key() == "ArrowLeft" { Focus::Delete } else { Focus::Save },
                        Focus::Save => if e.key() == "ArrowLeft" { Focus::Cancel } else { Focus::Delete },
                        Focus::Delete => if e.key() == "ArrowLeft" { Focus::Save } else { Focus::Cancel },
                    };
                    focused.set(next);
                }
                "Enter" => {
                    e.prevent_default();
                    match *focused {
                        Focus::Cancel => on_cxl.emit(()),
                        Focus::Save => on_sav.emit(()),
                        Focus::Delete => on_del.emit(()),
                    }
                }
                _ => {}
            }
        })
    };

    let lang = props.lang;

    html! {
        <div
            ref={root_ref} tabindex="0" onkeydown={on_keydown} onclick={|e: MouseEvent| e.stop_propagation()}
            class="fixed inset-0 z-[200] flex items-center justify-center p-4 outline-none pointer-events-auto"
        >
            <div class={classes!(
                "absolute", "inset-0", "bg-black/50", "backdrop-blur-md",
                if *is_fading_out { "animate-backdrop-out" } else { "animate-backdrop-in" }
            )}></div>
            <div class={classes!(
                "relative", "bg-gray-800", "border", "border-gray-700", "rounded-lg", "shadow-2xl", "w-full", "max-w-sm", "overflow-hidden",
                if *is_fading_out { "animate-dialog-out" } else { "animate-dialog-in" }
            )} onclick={|e: MouseEvent| e.stop_propagation()}>
                <div class="px-6 py-4 border-b border-gray-700 bg-gray-800/50">
                    <h3 class="text-lg font-bold text-white">{ i18n::t("empty_sheet_title", lang) }</h3>
                </div>
                <div class="px-6 py-8">
                    <p class="text-sm text-gray-300 whitespace-pre-wrap">{ i18n::t("empty_sheet_message", lang) }</p>
                </div>
                <div class="px-6 py-3 bg-gray-900/50 flex justify-end space-x-3">
                    <button
                        tabindex="-1" onclick={let cb = on_cancel.clone(); move |e: MouseEvent| { e.stop_propagation(); cb.emit(()); }}
                        class={classes!(
                            "px-5", "py-2", "rounded-md", "transition-colors", "outline-none", "border-[3px]",
                            if *focused == Focus::Cancel { vec!["bg-gray-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] }
                            else { vec!["bg-gray-700", "text-gray-300", "border-transparent"] }
                        )}
                    >{ i18n::t("cancel", lang) }</button>
                    <button
                        tabindex="-1" onclick={let cb = on_save.clone(); move |e: MouseEvent| { e.stop_propagation(); cb.emit(()); }}
                        class={classes!(
                            "px-5", "py-2", "rounded-md", "transition-colors", "outline-none", "border-[3px]",
                            if *focused == Focus::Save { vec!["bg-emerald-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] }
                            else { vec!["bg-emerald-600", "text-white", "border-transparent"] }
                        )}
                    >{ i18n::t("save", lang) }</button>
                    <button
                        tabindex="-1" onclick={let cb = on_delete.clone(); move |e: MouseEvent| { e.stop_propagation(); cb.emit(()); }}
                        class={classes!(
                            "px-5", "py-2", "rounded-md", "transition-colors", "shadow-lg", "outline-none", "border-[3px]",
                            if *focused == Focus::Delete { vec!["bg-red-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] }
                            else { vec!["bg-red-600", "text-white", "border-transparent"] }
                        )}
                    >{ i18n::t("delete", lang) }</button>
                </div>
            </div>
        </div>
    }
}
