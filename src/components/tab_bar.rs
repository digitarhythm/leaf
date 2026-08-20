use yew::prelude::*;
use wasm_bindgen::JsCast;
use crate::i18n::{self, Language};

/// プロジェクトタブのドロップダウンに並べる 1 件
#[derive(Clone, PartialEq)]
pub struct ProjectTabInfo {
    pub id: String,
    /// 表示名（デフォルトプロジェクトは i18n で解決済みの文字列が入る）
    pub name: String,
    /// デフォルトプロジェクト（閉じられない）かどうか
    pub is_default: bool,
}

#[derive(Clone, PartialEq)]
pub struct TabInfo {
    pub id: String,
    pub title: String,
    pub is_modified: bool,
    pub tab_color: String,
    /// ローカルファイルのシート（プロジェクトに属さない）かどうか
    #[allow(dead_code)]
    pub is_local: bool,
}

// --- Desktop Tab Bar ---

/// (from_id, to_id) - fromをtoの位置へ移動
pub type ReorderEvent = (String, String);

#[derive(Properties, PartialEq)]
pub struct TabBarProps {
    pub sheets: Vec<TabInfo>,
    pub active_sheet_id: Option<String>,
    pub on_select_tab: Callback<String>,
    pub on_close_tab: Callback<String>,
    #[prop_or_default]
    pub on_reorder: Option<Callback<ReorderEvent>>,
    #[prop_or_default]
    pub on_drag_end: Option<Callback<()>>,
    #[prop_or_default]
    pub on_new_tab: Option<Callback<()>>,
    /// 左端に固定するターミナルタブの中身（デスクトップ版のみ）
    #[prop_or_default]
    pub terminals: Vec<TabInfo>,
    /// 新規ターミナルの起動。None ならターミナルタブ自体を表示しない
    #[prop_or_default]
    pub on_new_terminal: Option<Callback<()>>,
    /// 開いているプロジェクトの一覧（ドロップダウンの中身）
    #[prop_or_default]
    pub open_projects: Vec<ProjectTabInfo>,
    /// 現在アクティブなプロジェクトの ID
    #[prop_or_default]
    pub active_project_id: String,
    /// プロジェクトの切り替え
    #[prop_or_default]
    pub on_switch_project: Callback<String>,
    /// プロジェクトを閉じる（デフォルトプロジェクトでは呼ばれない）
    #[prop_or_default]
    pub on_close_project: Callback<String>,
    /// ドロップダウンの開閉を親へ通知する。
    /// Esc を最優先で受け取るために、親側で開閉状態を持つ必要がある。
    #[prop_or_default]
    pub on_menu_open_change: Callback<bool>,
    /// Esc が押されたことを伝えるトリガー。
    /// グローバルのキーハンドラが Esc を消費するため、値の変化で受け取って
    /// ドロップダウンを閉じる。
    #[prop_or(0)]
    pub close_menu_trigger: u32,
}

#[function_component(TabBar)]
pub fn tab_bar(props: &TabBarProps) -> Html {
    let lang = Language::detect();
    let dragging_id = use_state(|| None::<String>);
    // 開いている固定タブのドロップダウン（"terminal" / "project"）
    let open_menu = use_state(|| None::<&'static str>);
    // タブバーは overflow-x-auto のため、内部に絶対配置するとドロップダウンが
    // 切り取られてしまう。タブの位置を測って画面に対する固定配置で描画する。
    let menu_pos = use_state(|| (0.0_f64, 0.0_f64));

    // ドロップダウンの開閉を親へ通知する
    {
        let notify = props.on_menu_open_change.clone();
        let is_open = open_menu.is_some();
        use_effect_with(is_open, move |is_open| {
            notify.emit(*is_open);
            || ()
        });
    }

    // Esc でドロップダウンを閉じる（初回マウント時はスキップ）
    {
        let om = open_menu.clone();
        let trigger = props.close_menu_trigger;
        let is_first_render = use_mut_ref(|| true);
        use_effect_with(trigger, move |_| {
            if *is_first_render.borrow() {
                *is_first_render.borrow_mut() = false;
            } else {
                om.set(None);
            }
            || ()
        });
    }

    // 固定タブ（ターミナル／プロジェクト）を含めて何も無い時のみ非表示
    if props.sheets.is_empty() && props.on_new_terminal.is_none() && props.open_projects.is_empty() {
        return html! {};
    }

    let on_dblclick_area = {
        let on_new_tab = props.on_new_tab.clone();
        Callback::from(move |e: MouseEvent| {
            // タブ自体（.tab-item クラス持つ要素かその子）でのダブルクリックは無視
            let target = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok());
            let on_tab = target.as_ref().map(|el| {
                el.closest(".tab-item").unwrap_or(None).is_some()
            }).unwrap_or(false);
            if !on_tab {
                if let Some(ref cb) = on_new_tab { cb.emit(()); }
            }
        })
    };

    // クリックされたタブの左下座標を求める（ドロップダウンの表示位置）
    let measure_pos = |e: &MouseEvent| -> (f64, f64) {
        e.target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|el| el.closest("[data-fixed-tab]").ok().flatten())
            .map(|el| {
                let rect = el.get_bounding_client_rect();
                (rect.left(), rect.bottom())
            })
            .unwrap_or((0.0, 32.0))
    };

    // --- 左端に固定するタブ（ターミナル／プロジェクト）---
    // tab_order の管理外で data-tab-id を持たないため、ドラッグの対象にならない
    let fixed_tab_html = {
        let active_id = props.active_sheet_id.clone();
        let is_terminal_active = active_id.as_ref().map(|id| id.starts_with("__TERM__")).unwrap_or(false);

        let terminal_tab = match props.on_new_terminal.clone() {
            None => html! {},
            Some(on_new_terminal) => {
                let is_open = *open_menu == Some("terminal");
                let label = match props.terminals.iter().find(|t| Some(&t.id) == active_id.as_ref()) {
                    Some(active) if is_terminal_active => active.title.clone(),
                    _ => i18n::t("terminal", lang),
                };
                let toggle = {
                    let om = open_menu.clone();
                    let mp = menu_pos.clone();
                    let on_new = on_new_terminal.clone();
                    let has_terminal = !props.terminals.is_empty();
                    Callback::from(move |e: MouseEvent| {
                        // 1つも起動していない時は、押すだけで新規ターミナルを起動する
                        if !has_terminal {
                            on_new.emit(());
                            return;
                        }
                        mp.set(measure_pos(&e));
                        om.set(if *om == Some("terminal") { None } else { Some("terminal") });
                    })
                };
                html! {
                    <div class="shrink-0">
                        <div
                            data-fixed-tab="terminal"
                            onclick={toggle}
                            class={classes!(
                                "flex", "items-center", "gap-1", "px-3", "py-1", "cursor-pointer",
                                "text-xs", "whitespace-nowrap", "select-none", "transition-all", "duration-100",
                                "border-r", "border-[#3c3836]",
                                // シートのタブと区別できるよう淡いオレンジ系にする
                                if is_terminal_active { "bg-[#4a3a2a] text-[#fbbf87] border-b-2 border-b-amber-400" }
                                else { "bg-[#3a2e22] text-[#d5a373] hover:bg-[#4a3a2a] hover:text-[#fbbf87] border-b-2 border-b-transparent" }
                            )}
                            title={i18n::t("terminal", lang)}
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M4 5h16a1 1 0 011 1v12a1 1 0 01-1 1H4a1 1 0 01-1-1V6a1 1 0 011-1z" /></svg>
                            <span class="max-w-[140px] truncate">{ label }</span>
                            <span class="text-[8px] opacity-60">{ "\u{25BC}" }</span>
                        </div>
                        if is_open {
                            <>
                                <div class="fixed inset-0 z-[80]" onclick={let om = open_menu.clone(); move |_| om.set(None)}></div>
                                <div class="fixed z-[90] min-w-[200px] bg-gray-800 border border-white/10 rounded-md shadow-2xl py-1"
                                    style={format!("left: {}px; top: {}px;", menu_pos.0, menu_pos.1 + 2.0)}>
                                    { for props.terminals.iter().map(|t| {
                                        let is_sel = Some(&t.id) == active_id.as_ref();
                                        let sel_id = t.id.clone();
                                        let close_id = t.id.clone();
                                        let on_select = props.on_select_tab.clone();
                                        let on_close = props.on_close_tab.clone();
                                        let om_sel = open_menu.clone();
                                        let om_close = open_menu.clone();
                                        html! {
                                            <div class={classes!("group", "flex", "items-center", "gap-2", "px-3", "py-1.5", "text-xs", "cursor-pointer", "transition-colors",
                                                if is_sel { "bg-emerald-600/30 text-emerald-300 font-bold" } else { "text-gray-300 hover:bg-emerald-600 hover:text-white" })}
                                                onclick={move |_| { on_select.emit(sel_id.clone()); om_sel.set(None); }}
                                            >
                                                <span class="flex-1 truncate">{ t.title.clone() }</span>
                                                <button
                                                    class="text-gray-500 hover:text-red-400 px-0.5 opacity-0 group-hover:opacity-100 transition-opacity"
                                                    title={i18n::t("close_tab", lang)}
                                                    onclick={move |e: MouseEvent| { e.stop_propagation(); on_close.emit(close_id.clone()); om_close.set(None); }}
                                                >{ "\u{2715}" }</button>
                                            </div>
                                        }
                                    })}
                                    <div class="border-t border-white/10 my-1"></div>
                                    <div
                                        class="flex items-center gap-2 px-3 py-1.5 text-xs text-emerald-400 hover:bg-emerald-600 hover:text-white cursor-pointer transition-colors"
                                        onclick={let om = open_menu.clone(); let on_new = on_new_terminal.clone(); move |_| { on_new.emit(()); om.set(None); }}
                                    >
                                        <span class="text-sm leading-none">{ "+" }</span>
                                        <span>{ i18n::t("new_terminal", lang) }</span>
                                    </div>
                                </div>
                            </>
                        }
                    </div>
                }
            }
        };

        // 開いているプロジェクトを 1 つずつタブとして並べる。
        // 現在のプロジェクトのシートタブは、この右側に続けて表示される。
        let project_tabs = html! {
            <>
                { for props.open_projects.iter().map(|p| {
                    let is_active = p.id == props.active_project_id;
                    let switch_id = p.id.clone();
                    let close_id = p.id.clone();
                    let on_switch = props.on_switch_project.clone();
                    let on_close = props.on_close_project.clone();
                    let can_close = !p.is_default;
                    html! {
                        <div
                            onclick={move |_: MouseEvent| on_switch.emit(switch_id.clone())}
                            class={classes!(
                                "group", "flex", "items-center", "gap-1", "px-3", "py-1", "cursor-pointer",
                                "text-xs", "whitespace-nowrap", "select-none", "transition-all", "duration-100",
                                "border-r", "border-[#3c3836]", "shrink-0",
                                // シートのタブと区別できるよう淡いブルー系にする
                                if is_active { "bg-[#31424f] text-[#a9d3ea] border-b-2 border-b-sky-400" }
                                else { "bg-[#26333d] text-[#8fb8d0] hover:bg-[#31424f] hover:text-[#a9d3ea] border-b-2 border-b-transparent" }
                            )}
                            title={p.name.clone()}
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M10.5 9.9h3v2h-3zM12 11.9v1.6M8.5 13.5h7M8.5 13.5v1.3M15.5 13.5v1.3M7 14.8h3v2H7zM14 14.8h3v2h-3z" /></svg>
                            <span class="max-w-[160px] truncate font-bold">{ p.name.clone() }</span>
                            if can_close {
                                // デフォルトプロジェクトは閉じられないので ✕ を出さない
                                <button
                                    class="ml-1 text-gray-500 hover:text-red-400 hover:bg-[#504945] rounded px-1 py-0.5 text-[5px] leading-none transition-colors"
                                    title={i18n::t("close_project", lang)}
                                    onclick={move |e: MouseEvent| { e.stop_propagation(); on_close.emit(close_id.clone()); }}
                                >{ "\u{2715}" }</button>
                            }
                        </div>
                    }
                }) }
            </>
        };

        html! { <>{ terminal_tab }{ project_tabs }</> }
    };

    html! {
        <div
            class="desktop:flex mobile:hidden items-center bg-[#1d2021] border-b border-[#3c3836] overflow-x-auto scrollbar-none"
            style="min-height: 32px;"
            ondblclick={on_dblclick_area}
        >
            { fixed_tab_html }
            { for props.sheets.iter().map(|tab| {
                let is_active = props.active_sheet_id.as_ref() == Some(&tab.id);
                let tab_id_select = tab.id.clone();
                let tab_id_close = tab.id.clone();
                let on_select = props.on_select_tab.clone();
                let on_close = props.on_close_tab.clone();
                let on_reorder = props.on_reorder.clone();
                let on_drag_end = props.on_drag_end.clone();
                let title = tab.title.clone();
                let is_modified = tab.is_modified;
                let is_dragging = (*dragging_id).as_ref() == Some(&tab.id);

                let dragging = dragging_id.clone();
                let tab_id_drag = tab.id.clone();

                // mousedownでドラッグ開始
                let onmousedown = {
                    let dragging = dragging.clone();
                    let tab_id = tab_id_drag.clone();
                    let on_reorder = on_reorder.clone();
                    let on_drag_end = on_drag_end.clone();
                    Callback::from(move |e: MouseEvent| {
                        // 閉じるボタンのクリックは除外
                        if let Some(target) = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                            if target.closest("button").ok().flatten().is_some() { return; }
                        }
                        e.prevent_default();
                        dragging.set(Some(tab_id.clone()));

                        let drag_id = tab_id.clone();
                        let dragging_end = dragging.clone();
                        let on_reorder = on_reorder.clone();
                        let on_drag_end = on_drag_end.clone();

                        // window mousemove/mouseupリスナー
                        let mousemove = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |me: web_sys::MouseEvent| {
                            let mx = me.client_x() as f64;
                            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                if let Ok(tabs) = doc.query_selector_all("[data-tab-id]") {
                                    // ドラッグ元タブの位置を取得
                                    let mut drag_idx = None;
                                    let mut tab_data: Vec<(String, f64, f64)> = Vec::new(); // (id, left, width)
                                    for i in 0..tabs.length() {
                                        if let Some(el) = tabs.item(i).and_then(|n| n.dyn_into::<web_sys::Element>().ok()) {
                                            if let Some(tid) = el.get_attribute("data-tab-id") {
                                                let rect = el.get_bounding_client_rect();
                                                if tid == drag_id { drag_idx = Some(tab_data.len()); }
                                                tab_data.push((tid, rect.left(), rect.width()));
                                            }
                                        }
                                    }
                                    if let Some(di) = drag_idx {
                                        // 右隣: マウスが右隣タブの中央を超えたら入れ替え
                                        if di + 1 < tab_data.len() {
                                            let (ref tid, left, width) = tab_data[di + 1];
                                            let center = left + width / 2.0;
                                            if mx > center {
                                                if let Some(ref cb) = on_reorder {
                                                    cb.emit((drag_id.clone(), tid.clone()));
                                                }
                                                return;
                                            }
                                        }
                                        // 左隣: マウスが左隣タブの中央を超えたら入れ替え
                                        if di > 0 {
                                            let (ref tid, left, width) = tab_data[di - 1];
                                            let center = left + width / 2.0;
                                            if mx < center {
                                                if let Some(ref cb) = on_reorder {
                                                    cb.emit((drag_id.clone(), tid.clone()));
                                                }
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        });

                        let mousemove_ref = std::rc::Rc::new(std::cell::RefCell::new(Some(mousemove)));
                        let mouseup_ref: std::rc::Rc<std::cell::RefCell<Option<wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MouseEvent)>>>> = std::rc::Rc::new(std::cell::RefCell::new(None));

                        let mouseup_ref2 = mouseup_ref.clone();
                        let mousemove_ref2 = mousemove_ref.clone();

                        let mouseup = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |_: web_sys::MouseEvent| {
                            dragging_end.set(None);
                            if let Some(ref cb) = on_drag_end { cb.emit(()); }
                            // リスナー解除
                            if let Some(win) = web_sys::window() {
                                if let Some(cb) = mousemove_ref2.borrow_mut().take() {
                                    let _ = win.remove_event_listener_with_callback("mousemove", cb.as_ref().unchecked_ref());
                                }
                                if let Some(cb) = mouseup_ref2.borrow_mut().take() {
                                    let _ = win.remove_event_listener_with_callback("mouseup", cb.as_ref().unchecked_ref());
                                }
                            }
                        });

                        if let Some(win) = web_sys::window() {
                            let _ = win.add_event_listener_with_callback("mousemove", mousemove_ref.borrow().as_ref().unwrap().as_ref().unchecked_ref());
                            *mouseup_ref.borrow_mut() = Some(mouseup);
                            let _ = win.add_event_listener_with_callback("mouseup", mouseup_ref.borrow().as_ref().unwrap().as_ref().unchecked_ref());
                        }
                    })
                };

                html! {
                    <div
                        data-tab-id={tab.id.clone()}
                        onmousedown={onmousedown}
                        class={classes!(
                            "tab-item",
                            "flex", "items-center", "gap-1", "px-3", "py-1", "cursor-grab",
                            "text-xs", "whitespace-nowrap", "select-none", "transition-all", "duration-100",
                            "border-r", "border-[#3c3836]", "shrink-0",
                                // ローカルファイルのシートはプロジェクト外なので淡い赤で区別する
                            if tab.is_local {
                                if is_active { "bg-[#4a2e2e] text-[#f5b0b0] border-b-2 border-b-red-400" }
                                else { "bg-[#3a2424] text-[#d59090] hover:bg-[#4a2e2e] hover:text-[#f5b0b0] border-b-2 border-b-transparent" }
                            } else if is_active { "bg-[#3c3836] text-[#ebdbb2] border-b-2 border-b-emerald-500" }
                            else { "bg-[#282828] text-gray-400 hover:bg-[#3c3836] hover:text-gray-300 border-b-2 border-b-transparent" },
                            if is_dragging { "opacity-40" } else { "" }
                        )}
                        onclick={Callback::from(move |_| on_select.emit(tab_id_select.clone()))}
                    >
                        <span class="max-w-[120px] truncate" title={title.clone()}>{title}</span>
                        if is_modified {
                            <span
                                class="text-[10px] text-amber-400 ml-0.5"
                                title={i18n::t("modified_indicator", lang)}
                            >
                                { "\u{25CF}" }
                            </span>
                        }
                        <button
                            class="ml-1 text-gray-500 hover:text-red-400 hover:bg-[#504945] rounded px-1 py-0.5 text-[5px] leading-none transition-colors"
                            title={i18n::t("close_tab", lang)}
                            onclick={Callback::from(move |e: MouseEvent| {
                                e.stop_propagation();
                                on_close.emit(tab_id_close.clone());
                            })}
                        >
                            { "\u{2715}" }
                        </button>
                    </div>
                }
            })}
        </div>
    }
}

// --- Mobile Sheet List Panel ---

#[derive(Properties, PartialEq)]
pub struct SheetListPanelProps {
    pub sheets: Vec<TabInfo>,
    pub active_sheet_id: Option<String>,
    pub on_select_tab: Callback<String>,
    pub on_close_tab: Callback<String>,
    pub on_close_panel: Callback<()>,
}

#[function_component(SheetListPanel)]
pub fn sheet_list_panel(props: &SheetListPanelProps) -> Html {
    let lang = Language::detect();
    let is_closing = use_state(|| false);

    let on_close = {
        let is_closing = is_closing.clone();
        let on_close_panel = props.on_close_panel.clone();
        Callback::from(move |_: ()| {
            is_closing.set(true);
            let cb = on_close_panel.clone();
            gloo::timers::callback::Timeout::new(250, move || { cb.emit(()); }).forget();
        })
    };

    let on_select = {
        let on_select_tab = props.on_select_tab.clone();
        let close = on_close.clone();
        Callback::from(move |id: String| {
            on_select_tab.emit(id);
            close.emit(());
        })
    };

    html! {
        <div class="fixed inset-0 z-[160] flex flex-col">
            // Backdrop
            <div
                class="absolute inset-0 bg-black/50"
                onclick={let c = on_close.clone(); move |_| c.emit(())}
            ></div>
            // Panel
            <div
                class={classes!(
                    "relative", "z-10", "flex", "flex-col", "bg-[#1d2021]", "border-b-2", "border-emerald-600",
                    "overflow-hidden", "transition-all", "duration-250", "ease-out",
                    if *is_closing { "max-h-0 opacity-0" } else { "max-h-[80vh] opacity-100" }
                )}
                style="animation: slideDown 250ms ease-out;"
            >
                // Header
                <div class="flex items-center justify-between px-4 py-2 bg-[#282828] border-b border-[#3c3836]">
                    <span class="text-sm font-bold text-[#ebdbb2]">
                        { i18n::t("open_sheets", lang) }
                        <span class="ml-2 text-xs text-gray-500">{ format!("({})", props.sheets.len()) }</span>
                    </span>
                    <button
                        onclick={let c = on_close.clone(); move |_| c.emit(())}
                        class="p-1 rounded hover:bg-[#3c3836] text-gray-400 hover:text-white transition-colors"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-5 h-5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>
                // Sheet list
                <div class="overflow-y-auto flex-1">
                    { for props.sheets.iter().map(|tab| {
                        html! {
                            <SwipeableSheetRow
                                tab={tab.clone()}
                                is_active={props.active_sheet_id.as_ref() == Some(&tab.id)}
                                on_select={on_select.clone()}
                                on_close_tab={props.on_close_tab.clone()}
                                on_close_panel={on_close.clone()}
                            />
                        }
                    })}
                </div>
            </div>
        </div>
    }
}

// --- Swipeable Sheet Row ---

#[derive(Properties, PartialEq)]
struct SwipeableSheetRowProps {
    tab: TabInfo,
    is_active: bool,
    on_select: Callback<String>,
    on_close_tab: Callback<String>,
    on_close_panel: Callback<()>,
}

#[function_component(SwipeableSheetRow)]
fn swipeable_sheet_row(props: &SwipeableSheetRowProps) -> Html {
    let lang = Language::detect();
    let row_ref = use_node_ref();
    let offset_x = use_state(|| 0.0_f64);
    let swipe_start_x = use_mut_ref(|| 0.0_f64);
    let swipe_start_y = use_mut_ref(|| 0.0_f64);
    let swipe_is_horizontal = use_mut_ref(|| None::<bool>);
    let is_dragging = use_state(|| false);

    let tab_id = props.tab.id.clone();
    let title = props.tab.title.clone();
    let is_modified = props.tab.is_modified;
    let is_active = props.is_active;

    let on_touch_start = {
        let sx = swipe_start_x.clone();
        let sy = swipe_start_y.clone();
        let sh = swipe_is_horizontal.clone();
        let ox = offset_x.clone();
        Callback::from(move |e: TouchEvent| {
            if let Some(touch) = e.touches().get(0) {
                *sx.borrow_mut() = touch.client_x() as f64;
                *sy.borrow_mut() = touch.client_y() as f64;
                *sh.borrow_mut() = None;
                ox.set(0.0);
            }
        })
    };

    let on_touch_move = {
        let sx = swipe_start_x.clone();
        let sy = swipe_start_y.clone();
        let sh = swipe_is_horizontal.clone();
        let ox = offset_x.clone();
        let dragging = is_dragging.clone();
        Callback::from(move |e: TouchEvent| {
            if let Some(touch) = e.touches().get(0) {
                let dx = touch.client_x() as f64 - *sx.borrow();
                let dy = touch.client_y() as f64 - *sy.borrow();
                let is_h = *sh.borrow();
                if is_h.is_none() {
                    if dx.abs() > 8.0 || dy.abs() > 8.0 {
                        let horizontal = dx.abs() > dy.abs();
                        *sh.borrow_mut() = Some(horizontal);
                        if !horizontal { return; }
                    } else {
                        return;
                    }
                } else if !is_h.unwrap_or(false) {
                    return;
                }
                e.prevent_default();
                dragging.set(true);
                if dx < 0.0 {
                    ox.set(dx);
                } else {
                    ox.set(0.0);
                }
            }
        })
    };

    let on_touch_end = {
        let ox = offset_x.clone();
        let dragging = is_dragging.clone();
        let on_close_tab = props.on_close_tab.clone();
        let on_close_panel = props.on_close_panel.clone();
        let tid = tab_id.clone();
        let rr = row_ref.clone();
        Callback::from(move |_: TouchEvent| {
            dragging.set(false);
            let offset = *ox;
            let item_width = rr.cast::<web_sys::Element>()
                .map(|el| el.client_width() as f64)
                .unwrap_or(300.0);
            let threshold = item_width / 3.0;
            if offset < -threshold {
                ox.set(-item_width);
                let cb_close = on_close_tab.clone();
                let cb_panel = on_close_panel.clone();
                let id = tid.clone();
                gloo::timers::callback::Timeout::new(200, move || {
                    cb_close.emit(id);
                    cb_panel.emit(());
                }).forget();
            } else {
                ox.set(0.0);
            }
        })
    };

    let on_close_click = {
        let on_close_tab = props.on_close_tab.clone();
        let on_close_panel = props.on_close_panel.clone();
        let tid = tab_id.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            on_close_tab.emit(tid.clone());
            on_close_panel.emit(());
        })
    };

    let on_click = {
        let on_sel = props.on_select.clone();
        let tid = tab_id.clone();
        let sh = swipe_is_horizontal.clone();
        Callback::from(move |_: MouseEvent| {
            let was_horizontal = (*sh.borrow()).unwrap_or(false);
            if !was_horizontal {
                on_sel.emit(tid.clone());
            }
        })
    };

    let current_offset = *offset_x;
    let transition = if *is_dragging { "none" } else { "transform 0.2s ease-out" };
    let bg_reveal = current_offset.abs() > 10.0;

    html! {
        <div ref={row_ref} class="relative overflow-hidden border-b border-[#3c3836]/50">
            // 赤い背景＋ゴミ箱アイコン（スワイプ時に露出）
            if bg_reveal {
                <div class="absolute inset-0 bg-red-900/40 flex items-center justify-end pr-6 z-0">
                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-5 h-5 text-red-400">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
                    </svg>
                </div>
            }
            <div
                class={classes!(
                    "relative", "z-[1]", "group", "flex", "items-center", "px-4", "py-3", "cursor-pointer",
                    if is_active { "bg-[#3c3836]" } else { "bg-[#1d2021] hover:bg-[#282828]" }
                )}
                style={format!("transform: translateX({current_offset}px); transition: {transition};")}
                onclick={on_click}
                ontouchstart={on_touch_start}
                ontouchmove={on_touch_move}
                ontouchend={on_touch_end}
            >
                // File icon
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class={classes!("w-5", "h-5", "mr-3", "shrink-0", if is_active { "text-[#ebdbb2]" } else { "text-gray-500" })}>
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                </svg>
                <div class="flex-1 min-w-0">
                    <span class={classes!(
                        "block", "text-sm", "truncate",
                        if is_active { "text-[#ebdbb2] font-bold" } else { "text-gray-400" }
                    )}>
                        { title }
                    </span>
                </div>
                if is_modified {
                    <span class="text-amber-400 text-xs mr-2" title={i18n::t("modified_indicator", lang)}>
                        { "\u{25CF}" }
                    </span>
                }
                <button
                    class="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-red-500/20 text-gray-500 hover:text-red-400 transition-all duration-150 shrink-0"
                    title={i18n::t("close_tab", lang)}
                    onclick={on_close_click}
                >
                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-4 h-4">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                    </svg>
                </button>
            </div>
        </div>
    }
}
