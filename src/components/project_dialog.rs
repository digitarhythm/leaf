//! プロジェクトダイアログ（Alt+P）。
//!
//! 全画面で開き、メニューバーの下を左右 50:50 に分割して
//! 左にプロジェクト一覧、右に選択中プロジェクトのシート一覧を表示する。

use crate::i18n::{self, Language};
use crate::project::{NameError, Project, ProjectStore};
use wasm_bindgen::JsCast;
use yew::prelude::*;

/// 右ペインに表示するシートの情報（親から解決済みのものを受け取る）
#[derive(Clone, PartialEq)]
pub struct ProjectSheetInfo {
    pub guid: String,
    /// 本文の先頭 5 行。guid を見ても内容が分からないため本文で識別する。
    pub preview: String,
    /// 拡張子（TXT / MD など）。空なら表示しない。
    pub lang: String,
}

#[derive(Properties, PartialEq)]
pub struct ProjectDialogProps {
    pub store: ProjectStore,
    /// guid → 本文プレビューの解決結果
    pub sheet_previews: Vec<ProjectSheetInfo>,
    /// プロジェクト追加。(名前, メモ) を渡す。検証はダイアログ側で済ませてある。
    pub on_create: Callback<(String, String)>,
    /// プロジェクト更新。(id, 名前, メモ)
    pub on_update: Callback<(String, String, String)>,
    pub on_delete: Callback<String>,
    /// (project_id, sheet_guid) — プロジェクトからシートを外す
    pub on_remove_sheet: Callback<(String, String)>,
    /// プロジェクトを開く（シートが 0 件のプロジェクトでは呼ばれない）
    pub on_open: Callback<String>,
    pub on_close: Callback<()>,
}

/// 入力ダイアログの用途
#[derive(Clone, PartialEq)]
enum NameInput {
    Create,
    Edit { id: String, name: String, memo: String },
}

#[derive(Properties, PartialEq)]
struct ProjectFormDialogProps {
    title: String,
    initial_name: String,
    initial_memo: String,
    /// 検証エラー文言。空なら表示しない。
    error: String,
    /// (プロジェクト名, メモ)
    on_confirm: Callback<(String, String)>,
    on_cancel: Callback<()>,
}

/// プロジェクト名とメモを入力するダイアログ。ボタンは左が Cancel、右が OK。
/// 検証は呼び出し側が行うため、OK を押しても自動では閉じない。
#[function_component(ProjectFormDialog)]
fn project_form_dialog(props: &ProjectFormDialogProps) -> Html {
    let lang = Language::detect();
    let name = use_state({
        let v = props.initial_name.clone();
        move || v
    });
    let memo = use_state({
        let v = props.initial_memo.clone();
        move || v
    });
    let root_ref = use_node_ref();
    let name_ref = use_node_ref();

    {
        let name_r = name_ref.clone();
        use_effect_with((), move |_| {
            let r = name_r.clone();
            gloo::timers::callback::Timeout::new(10, move || {
                if let Some(el) = r.cast::<web_sys::HtmlElement>() {
                    let _ = el.focus();
                }
            })
            .forget();
            || ()
        });
    }

    let confirm = {
        let on_confirm = props.on_confirm.clone();
        let name = name.clone();
        let memo = memo.clone();
        Callback::from(move |_: ()| {
            on_confirm.emit(((*name).clone(), (*memo).clone()));
        })
    };

    let on_keydown = {
        let confirm = confirm.clone();
        let on_cancel = props.on_cancel.clone();
        Callback::from(move |e: KeyboardEvent| {
            e.stop_propagation();
            if e.is_composing() {
                return;
            }
            match e.key().as_str() {
                "Escape" => {
                    e.prevent_default();
                    on_cancel.emit(());
                }
                // メモは複数行入力のため、Enter 単体では確定しない
                "Enter" if !e.shift_key() => {
                    let is_textarea = e
                        .target()
                        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                        .map(|el| el.tag_name().eq_ignore_ascii_case("textarea"))
                        .unwrap_or(false);
                    if !is_textarea {
                        e.prevent_default();
                        confirm.emit(());
                    }
                }
                _ => {}
            }
        })
    };

    let is_empty = name.trim().is_empty();

    html! {
        <div
            ref={root_ref} tabindex="0" onkeydown={on_keydown}
            onclick={|e: MouseEvent| e.stop_propagation()}
            class="fixed inset-0 z-[210] flex items-center justify-center p-4 outline-none pointer-events-auto"
        >
            <div class="absolute inset-0 bg-black/50 backdrop-blur-md animate-backdrop-in"></div>
            <div class="relative bg-gray-800 border border-gray-700 rounded-lg shadow-2xl w-full max-w-sm overflow-hidden animate-dialog-in">
                <div class="px-6 py-4 border-b border-gray-700 bg-gray-800/50">
                    <h3 class="text-lg font-bold text-white">{ props.title.clone() }</h3>
                </div>
                <div class="px-6 py-5 space-y-4">
                    <div class="space-y-1">
                        <label class="block text-xs font-bold text-gray-400">{ i18n::t("project_name", lang) }</label>
                        <input
                            ref={name_ref} type="text" value={(*name).clone()}
                            oninput={let n = name.clone(); move |ev: InputEvent| { let input: web_sys::HtmlInputElement = ev.target_unchecked_into(); n.set(input.value()); }}
                            class={classes!(
                                "w-full", "bg-gray-900", "border", "rounded-md", "px-4", "py-2", "text-white", "focus:outline-none", "transition-all",
                                if props.error.is_empty() { "border-gray-700 focus:border-lime-400" } else { "border-red-500 ring-2 ring-red-500" }
                            )}
                        />
                        if !props.error.is_empty() {
                            <p class="text-xs text-red-400 font-bold">{ props.error.clone() }</p>
                        }
                    </div>
                    <div class="space-y-1">
                        <label class="block text-xs font-bold text-gray-400">{ i18n::t("project_memo_optional", lang) }</label>
                        <textarea
                            rows="3" value={(*memo).clone()}
                            oninput={let m = memo.clone(); move |ev: InputEvent| { let input: web_sys::HtmlTextAreaElement = ev.target_unchecked_into(); m.set(input.value()); }}
                            class="w-full bg-gray-900 border border-gray-700 rounded-md px-4 py-2 text-white text-sm focus:outline-none focus:border-lime-400 transition-all resize-none custom-scrollbar"
                        ></textarea>
                    </div>
                </div>
                <div class="px-6 py-3 bg-gray-900/50 flex justify-end space-x-3">
                    <button
                        onclick={let c = props.on_cancel.clone(); move |e: MouseEvent| { e.stop_propagation(); c.emit(()); }}
                        class="px-6 py-2 rounded-md bg-gray-700 text-gray-300 hover:bg-gray-600 transition-colors"
                    >
                        { i18n::t("cancel", lang) }
                    </button>
                    <button
                        disabled={is_empty}
                        onclick={let c = confirm.clone(); move |e: MouseEvent| { e.stop_propagation(); c.emit(()); }}
                        class={classes!(
                            "px-6", "py-2", "rounded-md", "bg-emerald-600", "text-white", "shadow-lg", "transition-colors",
                            if is_empty { "opacity-50 cursor-not-allowed" } else { "hover:bg-emerald-500" }
                        )}
                    >
                        { i18n::t("ok", lang) }
                    </button>
                </div>
            </div>
        </div>
    }
}

#[function_component(ProjectDialog)]
pub fn project_dialog(props: &ProjectDialogProps) -> Html {
    let lang = Language::detect();
    let root_ref = use_node_ref();
    let selected_id = use_state(|| None::<String>);
    let name_input = use_state(|| None::<NameInput>);
    let name_error = use_state(String::new);
    let pending_delete = use_state(|| None::<Project>);
    let is_fading_out = use_state(|| false);

    // 初期選択（先頭のプロジェクト）とダイアログへのフォーカス
    {
        let root = root_ref.clone();
        use_effect_with((), move |_| {
            let r = root.clone();
            gloo::timers::callback::Timeout::new(10, move || {
                if let Some(el) = r.cast::<web_sys::HtmlElement>() {
                    let _ = el.focus();
                }
            })
            .forget();
            || ()
        });
    }
    {
        let sel = selected_id.clone();
        let first_id = props.store.projects.first().map(|p| p.id.clone());
        let ids: Vec<String> = props.store.projects.iter().map(|p| p.id.clone()).collect();
        use_effect_with(ids, move |ids| {
            // 選択中のプロジェクトが消えた場合は先頭へ戻す
            let still_exists = sel.as_ref().map(|id| ids.contains(id)).unwrap_or(false);
            if !still_exists {
                sel.set(first_id.clone());
            }
            || ()
        });
    }

    let handle_close = {
        let on_close = props.on_close.clone();
        let is_fading_out = is_fading_out.clone();
        Callback::from(move |_: ()| {
            is_fading_out.set(true);
            let cb = on_close.clone();
            gloo::timers::callback::Timeout::new(300, move || cb.emit(())).forget();
        })
    };

    let selected_project: Option<&Project> =
        selected_id.as_ref().and_then(|id| props.store.find(id));

    // プロジェクトを開く。シートが無いプロジェクトは開けない。
    let open_selected = {
        let on_open = props.on_open.clone();
        let store = props.store.clone();
        let sel = selected_id.clone();
        Callback::from(move |_: ()| {
            if let Some(id) = (*sel).clone() {
                if store.is_openable(&id) {
                    on_open.emit(id);
                }
            }
        })
    };

    let on_keydown = {
        let sel = selected_id.clone();
        let ids: Vec<String> = props.store.projects.iter().map(|p| p.id.clone()).collect();
        let close = handle_close.clone();
        let open = open_selected.clone();
        let has_sub_dialog = name_input.is_some() || pending_delete.is_some();
        Callback::from(move |e: KeyboardEvent| {
            if has_sub_dialog {
                return;
            }
            e.stop_propagation();
            match e.key().as_str() {
                "Escape" => {
                    e.prevent_default();
                    close.emit(());
                }
                "Enter" => {
                    e.prevent_default();
                    open.emit(());
                }
                "ArrowDown" | "ArrowUp" => {
                    e.prevent_default();
                    if ids.is_empty() {
                        return;
                    }
                    let cur = sel
                        .as_ref()
                        .and_then(|id| ids.iter().position(|x| x == id))
                        .unwrap_or(0);
                    let next = if e.key() == "ArrowDown" {
                        (cur + 1).min(ids.len() - 1)
                    } else {
                        cur.saturating_sub(1)
                    };
                    sel.set(Some(ids[next].clone()));
                }
                _ => {}
            }
        })
    };

    // --- メニューバー ---
    let menu_bar = {
        let ni = name_input.clone();
        let ne = name_error.clone();
        html! {
            <div class="flex items-center gap-2 px-3 py-2 border-b border-white/10 bg-gray-900 flex-shrink-0">
                <button
                    onclick={move |_| { ne.set(String::new()); ni.set(Some(NameInput::Create)); }}
                    class="flex items-center gap-1 px-2 py-1 rounded text-[11px] font-bold text-emerald-400 bg-emerald-500/10 hover:bg-emerald-500/20 border border-emerald-500/20 transition-all"
                    title={i18n::t("new_project", lang)}
                >
                    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" /></svg>
                </button>
            </div>
        }
    };

    // --- 左: プロジェクト一覧 ---
    let project_list = {
        let sel = selected_id.clone();
        let ni = name_input.clone();
        let ne = name_error.clone();
        let pd = pending_delete.clone();
        let open = open_selected.clone();
        html! {
            <div class="flex flex-col h-full min-h-0">
                <div class="px-3 py-1.5 text-[10px] font-black uppercase tracking-widest text-gray-500 border-b border-white/5 flex-shrink-0">
                    { i18n::t("projects", lang) }
                </div>
                <div class="flex-1 overflow-y-auto custom-scrollbar p-2 space-y-1">
                    if props.store.projects.is_empty() {
                        <div class="h-full flex items-center justify-center text-gray-600 text-xs">
                            { i18n::t("no_projects", lang) }
                        </div>
                    } else {
                        { for props.store.projects.iter().map(|p| {
                            let is_sel = sel.as_deref() == Some(p.id.as_str());
                            let openable = p.is_openable();
                            let pid = p.id.clone();
                            let pid_dbl = p.id.clone();
                            let sel_click = sel.clone();
                            let sel_dbl = sel.clone();
                            let open_dbl = open.clone();
                            html! {
                                <div
                                    onclick={move |_| sel_click.set(Some(pid.clone()))}
                                    ondblclick={move |_| {
                                        // 選択が未反映でも開けるよう、その場で選択してから開く
                                        sel_dbl.set(Some(pid_dbl.clone()));
                                        open_dbl.emit(());
                                    }}
                                    class={classes!(
                                        "group", "flex", "items-start", "gap-2", "px-3", "py-2", "rounded-md", "cursor-pointer", "transition-colors", "select-none",
                                        if is_sel { "bg-emerald-600 text-white" } else { "text-gray-300 hover:bg-white/5" }
                                    )}
                                    title={if openable { String::new() } else { i18n::t("project_empty_cannot_open", lang) }}
                                >
                                    // プロジェクトを表すアイコン（フォルダーの中にフローチャート）
                                    <svg xmlns="http://www.w3.org/2000/svg" class={classes!("h-4","w-4","flex-shrink-0","mt-0.5", if is_sel { "text-white" } else { "text-emerald-500/70" })} fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M10.5 9.9h3v2h-3zM12 11.9v1.6M8.5 13.5h7M8.5 13.5v1.3M15.5 13.5v1.3M7 14.8h3v2H7zM14 14.8h3v2h-3z" /></svg>
                                    // シートが 0 件のプロジェクトは開けないため淡色表示にする
                                    <div class={classes!("flex-1","min-w-0", if openable { "" } else { "opacity-40" })}>
                                        <div class="truncate text-sm font-medium">{ p.name.clone() }</div>
                                        if !p.memo.is_empty() {
                                            // プロジェクト名の下に一回り小さいフォントでメモを表示
                                            <div class={classes!("text-[10px]","leading-snug","line-clamp-2","whitespace-pre-wrap","break-all","mt-0.5", if is_sel { "text-white/70" } else { "text-gray-500" })}>
                                                { p.memo.clone() }
                                            </div>
                                        }
                                    </div>
                                    <span class={classes!("text-[10px]","font-mono","flex-shrink-0","mt-0.5", if is_sel { "text-white/70" } else { "text-gray-500" })}>
                                        { p.sheets.len() }
                                    </span>
                                    <div class="flex items-center gap-0.5 flex-shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                                        <button
                                            onclick={
                                                let ni = ni.clone(); let ne = ne.clone();
                                                let id = p.id.clone(); let cur = p.name.clone(); let memo = p.memo.clone();
                                                move |e: MouseEvent| {
                                                    e.stop_propagation();
                                                    ne.set(String::new());
                                                    ni.set(Some(NameInput::Edit { id: id.clone(), name: cur.clone(), memo: memo.clone() }));
                                                }
                                            }
                                            class={classes!("p-1","rounded","transition-colors", if is_sel { "hover:bg-black/20 text-white" } else { "hover:bg-white/10 text-gray-500" })}
                                            title={i18n::t("rename_project", lang)}
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" /></svg>
                                        </button>
                                        <button
                                            onclick={
                                                let pd = pd.clone(); let target = p.clone();
                                                move |e: MouseEvent| { e.stop_propagation(); pd.set(Some(target.clone())); }
                                            }
                                            class={classes!("p-1","rounded","transition-colors", if is_sel { "hover:bg-red-500/40 text-white" } else { "hover:bg-red-500/30 text-gray-500" })}
                                            title={i18n::t("delete", lang)}
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
                                        </button>
                                    </div>
                                </div>
                            }
                        })}
                    }
                </div>
            </div>
        }
    };

    // --- 右: プロジェクトのシート一覧 ---
    let sheet_list = {
        let on_remove = props.on_remove_sheet.clone();
        let previews = props.sheet_previews.clone();
        html! {
            <div class="flex flex-col h-full min-h-0">
                <div class="px-3 py-1.5 text-[10px] font-black uppercase tracking-widest text-gray-500 border-b border-white/5 flex-shrink-0">
                    { i18n::t("project_sheets", lang) }
                </div>
                <div class="flex-1 overflow-y-auto custom-scrollbar p-3">
                    {
                        match selected_project {
                            None => html! {
                                <div class="h-full flex items-center justify-center text-gray-600 text-xs">
                                    { i18n::t("no_projects", lang) }
                                </div>
                            },
                            Some(project) if project.sheets.is_empty() => html! {
                                <div class="h-full flex items-center justify-center text-gray-600 text-xs">
                                    { i18n::t("no_project_sheets", lang) }
                                </div>
                            },
                            // カード表示（シート選択ダイアログと同じ見せ方に揃える）
                            Some(project) => html! {
                                // 1シート1カードを縦に並べる
                                <div class="flex flex-col gap-3">
                                    { for project.sheets.iter().map(|guid| {
                                        let info = previews.iter().find(|t| &t.guid == guid);
                                        // 本文が取得できていない場合のみ guid を出す（最後の手段）
                                        let body = info
                                            .map(|t| t.preview.clone())
                                            .filter(|p| !p.trim().is_empty())
                                            .unwrap_or_else(|| guid.clone());
                                        let ext = info
                                            .map(|t| t.lang.clone())
                                            .filter(|e| !e.is_empty())
                                            .unwrap_or_else(|| "—".to_string());
                                        let pid = project.id.clone();
                                        let sheet_guid = guid.clone();
                                        let on_remove = on_remove.clone();
                                        html! {
                                            <div class="group relative flex flex-col rounded-lg border border-white/20 bg-gray-800 hover:border-emerald-500/60 hover:bg-gray-800/80 transition-colors overflow-hidden min-h-[8rem]">
                                                // ヘッダー: 拡張子バッジと削除ボタン
                                                <div class="flex items-center justify-between px-3 pt-2 pb-1 flex-shrink-0">
                                                    <div class="flex items-center gap-1.5 min-w-0">
                                                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 flex-shrink-0 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" /></svg>
                                                        <span class="px-1 py-0.5 rounded text-[8px] font-black uppercase tracking-tighter bg-emerald-500/10 text-emerald-400/80">{ ext }</span>
                                                    </div>
                                                    <button
                                                        onclick={move |e: MouseEvent| { e.stop_propagation(); on_remove.emit((pid.clone(), sheet_guid.clone())); }}
                                                        class="p-1 rounded text-gray-500 hover:bg-red-500/40 hover:text-white transition-colors opacity-0 group-hover:opacity-100 flex-shrink-0"
                                                        title={i18n::t("remove_from_project", lang)}
                                                    >
                                                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
                                                    </button>
                                                </div>
                                                // 本文の先頭5行
                                                <div class="px-3 pb-2 text-xs font-bold leading-snug line-clamp-5 break-all whitespace-pre-wrap text-gray-300">
                                                    { body }
                                                </div>
                                            </div>
                                        }
                                    })}
                                </div>
                            },
                        }
                    }
                </div>
            </div>
        }
    };

    // --- プロジェクト名入力ダイアログ（新規／変更で共用）---
    let name_dialog = match (*name_input).clone() {
        None => html! {},
        Some(kind) => {
            let (title, initial_name, initial_memo) = match &kind {
                NameInput::Create => (i18n::t("new_project", lang), String::new(), String::new()),
                NameInput::Edit { name, memo, .. } => {
                    (i18n::t("rename_project", lang), name.clone(), memo.clone())
                }
            };
            let on_confirm = {
                let store = props.store.clone();
                let on_create = props.on_create.clone();
                let on_update = props.on_update.clone();
                let ni = name_input.clone();
                let ne = name_error.clone();
                let kind = kind.clone();
                Callback::from(move |(value, memo): (String, String)| {
                    let exclude = match &kind {
                        NameInput::Create => None,
                        NameInput::Edit { id, .. } => Some(id.as_str()),
                    };
                    match store.validate_name(&value, exclude) {
                        Err(err) => {
                            // 検証エラー時はダイアログを閉じずにメッセージを出す
                            ne.set(i18n::t(NameError::i18n_key(err), lang));
                        }
                        Ok(name) => {
                            match &kind {
                                NameInput::Create => on_create.emit((name, memo)),
                                NameInput::Edit { id, .. } => {
                                    on_update.emit((id.clone(), name, memo))
                                }
                            }
                            ne.set(String::new());
                            ni.set(None);
                        }
                    }
                })
            };
            html! {
                <ProjectFormDialog
                    title={title}
                    initial_name={initial_name}
                    initial_memo={initial_memo}
                    error={(*name_error).clone()}
                    on_confirm={on_confirm}
                    on_cancel={let ni = name_input.clone(); let ne = name_error.clone(); Callback::from(move |_| { ne.set(String::new()); ni.set(None); })}
                />
            }
        }
    };

    // --- 削除確認ダイアログ ---
    let delete_dialog = match (*pending_delete).clone() {
        None => html! {},
        Some(project) => html! {
            <div class="z-[210]">
                <crate::components::dialog::ConfirmDialog
                    title={i18n::t("delete", lang)}
                    message={format!("{}\n{}", project.name, i18n::t("confirm_delete_project", lang))}
                    on_confirm={
                        let on_delete = props.on_delete.clone();
                        let pd = pending_delete.clone();
                        let id = project.id.clone();
                        Callback::from(move |_| { on_delete.emit(id.clone()); pd.set(None); })
                    }
                    on_cancel={let pd = pending_delete.clone(); Callback::from(move |_| pd.set(None))}
                />
            </div>
        },
    };

    html! {
        <div
            ref={root_ref}
            tabindex="0"
            onkeydown={on_keydown}
            class="fixed inset-0 z-[100] flex items-stretch justify-center outline-none pointer-events-auto"
            onclick={|e: MouseEvent| e.stop_propagation()}
        >
            <div class={classes!(
                "absolute", "inset-0", "bg-black",
                if *is_fading_out { "animate-backdrop-out" } else { "animate-backdrop-in" }
            )} onclick={handle_close.reform(|_| ())}></div>

            <div class={classes!(
                "relative", "flex", "flex-col", "w-full", "h-full", "bg-gray-900", "overflow-hidden",
                "shadow-2xl", "border-2", "border-emerald-500", "rounded-lg",
                if *is_fading_out { "animate-slide-out" } else { "animate-slide-in" }
            )} onclick={|e: MouseEvent| e.stop_propagation()}>
                { menu_bar }
                // メニューバーの下を左右 50:50 に分割
                <div class="flex flex-row flex-1 min-h-0">
                    <div class="w-1/2 min-w-0 overflow-hidden border-r border-white/10 bg-gray-900">
                        { project_list }
                    </div>
                    <div class="w-1/2 min-w-0 overflow-hidden bg-gray-950">
                        { sheet_list }
                    </div>
                </div>
                <div class="bg-gray-950/50 border-t border-white/5 flex items-center justify-between p-3 flex-shrink-0">
                    <div class="flex items-center gap-3 text-[10px] text-gray-500">
                        <span class="flex items-center gap-1"><kbd class="px-1 py-0.5 bg-gray-800 rounded text-gray-400 font-mono">{"↑↓"}</kbd>{ i18n::t("key_navigate", lang) }</span>
                        <span class="flex items-center gap-1"><kbd class="px-1 py-0.5 bg-gray-800 rounded text-gray-400 font-mono">{"Enter"}</kbd>{ i18n::t("open_project", lang) }</span>
                    </div>
                    <button
                        onclick={handle_close.reform(|_| ())}
                        class="px-4 py-1.5 rounded-md text-xs font-bold text-gray-400 hover:bg-white/5 transition-all uppercase tracking-widest border border-white/10"
                    >
                        { i18n::t("cancel", lang) }
                    </button>
                </div>
            </div>

            { name_dialog }
            { delete_dialog }
        </div>
    }
}
