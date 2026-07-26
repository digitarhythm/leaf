use yew::prelude::*;
use gloo::timers::callback::Timeout;
use crate::i18n::{self, Language};
use crate::js_interop::{
    preview_search, preview_search_goto, preview_search_clear,
    editor_search, editor_search_goto, editor_search_clear,
};

/// 検索対象。UI・操作感は共通で、検索処理だけを切り替える。
#[derive(Clone, Copy, PartialEq)]
pub enum SearchTarget {
    /// プレビュー（レンダリング済みHTML）
    Preview,
    /// Ace エディタの本文
    Editor,
}

/// プレビュー／エディタ共通の検索バー。
/// Ace 標準の検索ボックス（ext-searchbox）は使わず、どちらも同じ UI で検索する。
#[derive(Properties, PartialEq)]
pub struct SearchBarProps {
    /// 検索対象
    pub target: SearchTarget,
    /// バーを閉じる（ハイライトはアンマウント時に自動解除）
    pub on_close: Callback<()>,
    /// 大文字小文字を区別するか
    pub match_case: bool,
    /// 大文字小文字トグルの変更通知（永続化は呼び出し側）
    pub on_toggle_match_case: Callback<bool>,
}

#[function_component(SearchBar)]
pub fn search_bar(props: &SearchBarProps) -> Html {
    let lang = Language::detect();
    let query = use_state(String::new);
    let total = use_state(|| 0i32);
    let current = use_state(|| -1i32);
    let input_ref = use_node_ref();
    let target = props.target;
    let is_editor = matches!(target, SearchTarget::Editor);

    // マウント時に入力欄へフォーカス、アンマウント時にハイライトを解除
    {
        let input_ref = input_ref.clone();
        use_effect_with(is_editor, move |_| {
            if let Some(el) = input_ref.cast::<web_sys::HtmlInputElement>() {
                let _ = el.focus();
            }
            move || {
                if is_editor {
                    // 閉じる時はカーソルをヒット位置に残したままエディタへフォーカスを戻す
                    editor_search_clear(true);
                } else {
                    preview_search_clear();
                }
            }
        });
    }

    // インクリメンタルサーチ（150ms デバウンス）
    {
        let total = total.clone();
        let current = current.clone();
        let q = (*query).clone();
        use_effect_with((q, props.match_case), move |(q, match_case)| {
            let q = q.clone();
            let match_case = *match_case;
            let timeout = Timeout::new(150, move || {
                let n = if q.is_empty() {
                    if is_editor { editor_search("", match_case) } else { preview_search_clear(); 0 }
                } else if is_editor {
                    editor_search(&q, match_case)
                } else {
                    preview_search(&q, match_case)
                };
                total.set(n);
                current.set(if n > 0 { 0 } else { -1 });
            });
            move || drop(timeout)
        });
    }

    let goto = {
        let total = total.clone();
        let current = current.clone();
        Callback::from(move |delta: i32| {
            if *total > 0 {
                let i = if is_editor {
                    editor_search_goto(*current + delta)
                } else {
                    preview_search_goto(*current + delta)
                };
                current.set(i);
            }
        })
    };

    let go_next = { let g = goto.clone(); Callback::from(move |_: ()| g.emit(1)) };
    let go_prev = { let g = goto.clone(); Callback::from(move |_: ()| g.emit(-1)) };

    let oninput = {
        let query = query.clone();
        Callback::from(move |e: InputEvent| {
            let value = e.target_unchecked_into::<web_sys::HtmlInputElement>().value();
            query.set(value);
        })
    };

    let onkeydown = {
        let go_next = go_next.clone();
        let go_prev = go_prev.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |e: KeyboardEvent| {
            let key = e.key();
            if key == "Enter" {
                e.prevent_default();
                e.stop_propagation();
                if e.shift_key() { go_prev.emit(()); } else { go_next.emit(()); }
            } else if key == "Escape" {
                e.prevent_default();
                e.stop_propagation();
                on_close.emit(());
            }
        })
    };

    let on_toggle_case = {
        let on_toggle = props.on_toggle_match_case.clone();
        let match_case = props.match_case;
        let input_ref = input_ref.clone();
        Callback::from(move |_: MouseEvent| {
            on_toggle.emit(!match_case);
            if let Some(el) = input_ref.cast::<web_sys::HtmlInputElement>() { let _ = el.focus(); }
        })
    };

    let has_query = !(*query).is_empty();
    let no_match = has_query && *total == 0;
    let counter = if no_match {
        i18n::t("search_no_match", lang)
    } else if *total > 0 {
        format!("{} / {}", *current + 1, *total)
    } else {
        "0 / 0".to_string()
    };
    let placeholder = if is_editor {
        i18n::t("search_placeholder_editor", lang)
    } else {
        i18n::t("search_placeholder_preview", lang)
    };

    let btn_class = "p-1 rounded text-gray-400 hover:text-emerald-400 hover:bg-gray-800 transition-colors disabled:opacity-30 disabled:hover:text-gray-400 disabled:hover:bg-transparent";

    html! {
        <div class="absolute top-3 right-4 z-40 flex items-center gap-2 bg-gray-900/95 border border-emerald-500/70 rounded-lg shadow-2xl px-3 py-2 backdrop-blur-sm"
             onmousedown={|e: MouseEvent| e.stop_propagation()}>
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-4 h-4 text-emerald-500 flex-shrink-0">
                <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z" />
            </svg>
            <input
                ref={input_ref}
                type="text"
                value={(*query).clone()}
                oninput={oninput}
                onkeydown={onkeydown}
                placeholder={placeholder}
                spellcheck="false"
                autocomplete="off"
                class={classes!(
                    "w-40", "sm:w-56", "bg-gray-800", "text-gray-100", "text-xs", "rounded", "px-2", "py-1", "outline-none", "border",
                    if no_match { "border-red-500" } else { "border-gray-700 focus:border-emerald-500" }
                )}
            />
            <span class={classes!(
                "text-[10px]", "font-mono", "whitespace-nowrap", "text-center", "min-w-[4rem]",
                if no_match { "text-red-400" } else { "text-gray-400" }
            )}>{ counter }</span>
            <button
                onclick={on_toggle_case}
                title={i18n::t("search_match_case", lang)}
                aria-label={i18n::t("search_match_case", lang)}
                class={classes!(
                    "px-1.5", "py-0.5", "rounded", "text-[10px]", "font-bold", "font-mono", "border", "transition-colors",
                    if props.match_case { "border-emerald-500 text-emerald-400 bg-emerald-500/10" } else { "border-gray-700 text-gray-500 hover:text-gray-300" }
                )}
            >{ "Aa" }</button>
            <button
                onclick={let cb = go_prev.clone(); move |_: MouseEvent| cb.emit(())}
                disabled={*total == 0}
                title={i18n::t("search_prev", lang)}
                aria-label={i18n::t("search_prev", lang)}
                class={btn_class}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-4 h-4">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M4.5 15.75l7.5-7.5 7.5 7.5" />
                </svg>
            </button>
            <button
                onclick={let cb = go_next.clone(); move |_: MouseEvent| cb.emit(())}
                disabled={*total == 0}
                title={i18n::t("search_next", lang)}
                aria-label={i18n::t("search_next", lang)}
                class={btn_class}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-4 h-4">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 8.25l-7.5 7.5-7.5-7.5" />
                </svg>
            </button>
            <button
                onclick={let cb = props.on_close.clone(); move |_: MouseEvent| cb.emit(())}
                title={i18n::t("close", lang)}
                aria-label={i18n::t("close", lang)}
                class={btn_class}
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-4 h-4">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
            </button>
        </div>
    }
}
