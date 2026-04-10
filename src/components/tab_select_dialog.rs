use yew::prelude::*;
use crate::i18n::{self, Language};
use gloo::timers::callback::Timeout;
use wasm_bindgen::JsCast;

#[derive(Clone, PartialEq)]
pub struct TabSelectItem {
    pub id: String,
    pub title: String,
    pub tab_color: String,
    pub content: String,
}

#[derive(Properties, PartialEq)]
pub struct TabSelectDialogProps {
    pub tabs: Vec<TabSelectItem>,
    pub on_select: Callback<String>,
    pub on_close: Callback<()>,
}

fn scroll_item_into_view(index: usize) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id(&format!("tab-item-{}", index)) {
            let el_js: &wasm_bindgen::JsValue = el.as_ref();
            let has_if_needed = js_sys::Reflect::has(el_js, &"scrollIntoViewIfNeeded".into()).unwrap_or(false);
            if has_if_needed {
                if let Ok(f) = js_sys::Reflect::get(el_js, &"scrollIntoViewIfNeeded".into()) {
                    if let Some(func) = f.dyn_ref::<js_sys::Function>() {
                        let _ = func.call1(el_js, &true.into());
                    }
                }
            } else {
                el.scroll_into_view_with_bool(false);
            }
        }
    }
}

#[function_component(TabSelectDialog)]
pub fn tab_select_dialog(props: &TabSelectDialogProps) -> Html {
    let lang = Language::detect();
    let is_closing = use_state(|| false);
    let is_visible = use_state(|| false); // フェードイン用
    // use_state: 再レンダー用。use_mut_ref: クロージャ内での最新値アクセス用（stale closure対策）
    let selected_index = use_state(|| 0usize);
    let selected_index_ref = use_mut_ref(|| 0usize);

    let handle_close = {
        let on_close = props.on_close.clone();
        let is_closing = is_closing.clone();
        Callback::from(move |_: ()| {
            is_closing.set(true);
            let cb = on_close.clone();
            Timeout::new(300, move || { cb.emit(()); }).forget();
        })
    };

    // ダイアログにフォーカスを移してターミナルへのキーイベントをブロック＋フェードイン
    {
        let iv = is_visible.clone();
        use_effect_with((), move |_| {
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                if let Some(el) = doc.get_element_by_id("tab-select-dialog") {
                    let _ = el.unchecked_ref::<web_sys::HtmlElement>().focus();
                }
            }
            // 次フレームでopacity-100に切り替えてフェードイン
            Timeout::new(10, move || { iv.set(true); }).forget();
            || ()
        });
    }

    // キーボードナビゲーション（use_mut_refで最新インデックスを参照）
    {
        let hc = handle_close.clone();
        let idx_state = selected_index.clone();
        let idx_ref = selected_index_ref.clone();
        let on_select = props.on_select.clone();
        let hc_confirm = handle_close.clone();
        let tabs = props.tabs.clone();
        let tab_count = props.tabs.len();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let mut opts = gloo::events::EventListenerOptions::run_in_capture_phase();
            opts.passive = false;
            let listener = gloo::events::EventListener::new_with_options(&window, "keydown", opts, move |e| {
                let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                match ke.key().as_str() {
                    "Escape" => {
                        e.stop_immediate_propagation();
                        e.prevent_default();
                        hc.emit(());
                    }
                    "ArrowUp" => {
                        e.stop_immediate_propagation();
                        e.prevent_default();
                        let cur = *idx_ref.borrow();
                        if cur > 0 {
                            let new_idx = cur - 1;
                            *idx_ref.borrow_mut() = new_idx;
                            idx_state.set(new_idx);
                            scroll_item_into_view(new_idx);
                        }
                    }
                    "ArrowDown" => {
                        e.stop_immediate_propagation();
                        e.prevent_default();
                        let cur = *idx_ref.borrow();
                        if tab_count > 0 && cur < tab_count - 1 {
                            let new_idx = cur + 1;
                            *idx_ref.borrow_mut() = new_idx;
                            idx_state.set(new_idx);
                            scroll_item_into_view(new_idx);
                        }
                    }
                    "Enter" => {
                        e.stop_immediate_propagation();
                        e.prevent_default();
                        let cur = *idx_ref.borrow();
                        if let Some(tab) = tabs.get(cur) {
                            on_select.emit(tab.id.clone());
                            hc_confirm.emit(());
                        }
                    }
                    _ => {}
                }
            });
            Box::new(move || drop(listener)) as Box<dyn FnOnce()>
        });
    }

    // 選択中タブのプレビューHTML
    let preview_html = if let Some(tab) = props.tabs.get(*selected_index) {
        crate::js_interop::render_markdown(&tab.content)
    } else {
        String::new()
    };

    let anim_class = if *is_closing || !*is_visible { "opacity-0 scale-95" } else { "opacity-100 scale-100" };
    let title = i18n::t("select_tab_to_preview", lang);

    html! {
        <div class="fixed inset-0 z-[250] flex items-center justify-center">
            // Backdrop
            <div
                class={classes!(
                    "absolute", "inset-0", "bg-black/60", "transition-opacity", "duration-300",
                    if *is_closing || !*is_visible { "opacity-0" } else { "opacity-100" }
                )}
                onclick={{let hc = handle_close.clone(); move |_| hc.emit(())}}
            ></div>

            // Dialog
            <div
                id="tab-select-dialog"
                tabindex="0"
                class={classes!(
                    "relative", "z-10", "w-full", "max-w-3xl", "mx-4",
                    "bg-[#1d2021]", "rounded-xl", "border", "border-[#3c3836]", "shadow-2xl",
                    "transition-all", "duration-300", "flex", "flex-col",
                    "outline-none",
                    anim_class
                )}
                style="height: 80vh;"
            >
                // Header
                <div class="flex items-center justify-between px-6 py-3 border-b border-[#3c3836] flex-shrink-0">
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

                // 上半分: タブ一覧（各タブの1行目）
                <div class="flex-1 overflow-y-auto border-b border-[#3c3836]">
                    if props.tabs.is_empty() {
                        <div class="px-5 py-8 text-center text-xs text-gray-500">
                            { i18n::t("no_tabs_open", lang) }
                        </div>
                    } else {
                        { for props.tabs.iter().enumerate().map(|(i, tab)| {
                            let first_line = tab.content.lines().next()
                                .map(|l| l.trim().to_string())
                                .filter(|l| !l.is_empty())
                                .unwrap_or_else(|| tab.title.clone());
                            let is_selected = i == *selected_index;
                            let idx_s = selected_index.clone();
                            let idx_r = selected_index_ref.clone();
                            let color = tab.tab_color.clone();
                            let tab_id_dbl = tab.id.clone();
                            let on_select_dbl = props.on_select.clone();
                            let hc_dbl = handle_close.clone();
                            html! {
                                <button
                                    id={format!("tab-item-{}", i)}
                                    class={classes!(
                                        "w-full", "flex", "items-center", "gap-3",
                                        "px-5", "py-2", "transition-colors", "text-left",
                                        if is_selected { "bg-[#3c3836]" } else { "hover:bg-[#282828]/60" }
                                    )}
                                    onclick={Callback::from(move |_| {
                                        *idx_r.borrow_mut() = i;
                                        idx_s.set(i);
                                    })}
                                    ondblclick={Callback::from(move |_| {
                                        on_select_dbl.emit(tab_id_dbl.clone());
                                        hc_dbl.emit(());
                                    })}
                                >
                                    <div class="w-2 h-2 rounded-full flex-shrink-0" style={format!("background-color: {};", color)}></div>
                                    <span class={classes!(
                                        "text-xs", "truncate", "font-mono",
                                        if is_selected { "text-emerald-400" } else { "text-[#d4be98]" }
                                    )}>{ first_line }</span>
                                </button>
                            }
                        }) }
                    }
                </div>

                // 下半分: 選択タブのプレビュー
                <div class="flex-1 overflow-y-auto px-5 py-3">
                    <div class="prose prose-invert max-w-none text-sm text-[#d4be98] leading-relaxed">
                        { Html::from_html_unchecked(AttrValue::from(preview_html)) }
                    </div>
                </div>

                // フッター: OKとCancelボタン
                <div class="flex justify-end gap-3 px-6 py-3 border-t border-[#3c3836] flex-shrink-0">
                    <button
                        onclick={{let hc = handle_close.clone(); move |_| hc.emit(())}}
                        class="px-4 py-1.5 rounded text-sm text-gray-300 hover:bg-[#3c3836] transition-colors"
                    >
                        { i18n::t("cancel", lang) }
                    </button>
                    <button
                        onclick={{
                            let tabs = props.tabs.clone();
                            let on_select = props.on_select.clone();
                            let hc = handle_close.clone();
                            let idx_r = selected_index_ref.clone();
                            move |_| {
                                let cur = *idx_r.borrow();
                                if let Some(tab) = tabs.get(cur) {
                                    on_select.emit(tab.id.clone());
                                    hc.emit(());
                                }
                            }
                        }}
                        class="px-4 py-1.5 rounded text-sm font-bold bg-emerald-600 hover:bg-emerald-700 text-white transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                        disabled={props.tabs.is_empty()}
                    >
                        { i18n::t("ok", lang) }
                    </button>
                </div>
            </div>
        </div>
    }
}
