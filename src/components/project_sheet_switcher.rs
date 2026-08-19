//! プロジェクトを開いている時の Alt+T シート切り替えダイアログ。
//!
//! タブで開いているシートを、拡張子に応じた「書類」風のカードで
//! 横 4 列・縦スクロールのグリッドに並べて選択させる。
//!
//! 選択位置の計算はグリッド上のインデックス演算だけで完結するため、
//! 純粋関数として切り出して単体テストの対象にしている。

use crate::i18n::{self, Language};
use yew::prelude::*;

/// グリッド 1 行あたりのカード数
pub const GRID_COLUMNS: usize = 4;

/// カーソルキーの方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// グリッド上で選択を 1 つ動かす。範囲外へは出ず、端では止まる。
///
/// * 左右は行をまたいで連続移動する（行末の次は次行の先頭）
/// * 上下は 1 行（`columns` 個）分移動する
/// * 下に行があるのに移動先が欠けている（最終行が埋まっていない）場合は末尾へ寄せる
pub fn move_selection(current: usize, total: usize, columns: usize, dir: Dir) -> usize {
    if total == 0 {
        return 0;
    }
    let columns = columns.max(1);
    let last = total - 1;
    let current = current.min(last);

    match dir {
        Dir::Left => current.saturating_sub(1),
        Dir::Right => (current + 1).min(last),
        Dir::Up => {
            if current >= columns {
                current - columns
            } else {
                current
            }
        }
        Dir::Down => {
            // 最終行にいる場合は動かさない
            let last_row_start = (last / columns) * columns;
            if current >= last_row_start {
                current
            } else {
                // 真下が存在しない（最終行が途中まで）場合は末尾のカードへ
                (current + columns).min(last)
            }
        }
    }
}

/// 拡張子から書類カードの見た目の系統を決める。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocStyle {
    /// Markdown: 見出し行が並ぶ書類
    Markdown,
    /// ソースコード: 行番号付きの書類
    Code,
    /// プレーンテキスト
    Text,
}

impl DocStyle {
    /// シート名（`{guid}.{拡張子}`）や拡張子文字列から判定する
    pub fn from_extension(ext: &str) -> Self {
        let ext = ext.trim().trim_start_matches('.').to_lowercase();
        match ext.as_str() {
            "md" | "markdown" => DocStyle::Markdown,
            "txt" | "text" | "log" | "csv" | "" => DocStyle::Text,
            _ => DocStyle::Code,
        }
    }
}

/// ファイル名から拡張子を取り出す（大文字化した表示用の文字列）。
/// 拡張子が無い場合は空文字。
pub fn display_extension(file_name: &str) -> String {
    match file_name.rfind('.') {
        Some(pos) if pos + 1 < file_name.len() && pos > 0 => {
            file_name[pos + 1..].to_uppercase()
        }
        _ => String::new(),
    }
}

/// グリッドに 1 枚並べるカードの情報
#[derive(Clone, PartialEq)]
pub struct SwitcherSheet {
    pub id: String,
    /// ファイル名（拡張子の判定と表示に使う）。ターミナルでは表示名が入る
    pub title: String,
    /// 本文（書類の中身として冒頭を表示する）。ターミナルでは空
    pub content: String,
    pub tab_color: String,
    /// ターミナルのカードかどうか（先頭にまとめて並べる）
    #[allow(dead_code)]
    pub is_terminal: bool,
}

#[derive(Properties, PartialEq)]
pub struct ProjectSheetSwitcherProps {
    pub sheets: Vec<SwitcherSheet>,
    pub active_sheet_id: Option<String>,
    /// 選択確定。シート ID を渡す。
    pub on_select: Callback<String>,
    pub on_close: Callback<()>,
    /// 外部（Alt+R の再押下）から閉じるためのトリガー。
    /// 値が変わるとダイアログ自身の閉じる処理を実行し、
    /// Esc と同じスライドアップのアニメーションで閉じる。
    #[prop_or(0)]
    pub close_trigger: u32,
}

/// 書類カードの中身に描画する行数。
/// 縮小表示のため通常のプレビューより多くの行が入る。全文を描画すると
/// カード枚数ぶんの描画コストが嵩むため上限を設ける。
/// 選択中のカードはスクロールできるので、カードの高さより多めに描画する。
const DOC_PREVIEW_LINES: usize = 150;

#[function_component(ProjectSheetSwitcher)]
pub fn project_sheet_switcher(props: &ProjectSheetSwitcherProps) -> Html {
    let lang = Language::detect();
    let root_ref = use_node_ref();
    let is_closing = use_state(|| false);
    // Tauri(デスクトップ)版は同じ指定でも文字が小さく見えるため一回り大きくする
    // （シート選択ダイアログのファイル名ラベルと同じ調整）
    let is_desktop = crate::js_interop::is_tauri();
    // ブラウザ版はこのサイズがちょうど良いので変えず、デスクトップ版だけ大きくする
    let head_text_class = if is_desktop { "text-sm" } else { "text-[9px]" };
    let badge_text_class = if is_desktop { "text-[11px]" } else { "text-[8px]" };
    // タイトルバーの高さもデスクトップ版だけ広げる
    let head_pad_class = if is_desktop { "py-1.5" } else { "py-1" };
    // カード内のプレビューの描画は重い（marked / highlight.js）。
    // 最初の描画で実行するとロールダウンのアニメーションが表示されないまま
    // 終わってしまう（特にデスクトップ版の WebView で顕著）ため、
    // アニメーションが終わってから内容を作る。
    let content_ready = use_state(|| false);
    {
        let cr = content_ready.clone();
        use_effect_with((), move |_| {
            gloo::timers::callback::Timeout::new(120, move || cr.set(true)).forget();
            || ()
        });
    }
    // 選択が変わるたびに再計算しないよう、シートの内容が変わった時だけ作り直す
    let rendered_docs = use_memo((props.sheets.clone(), *content_ready), |(sheets, ready)| {
        if !*ready {
            return Vec::new();
        }
        sheets
            .iter()
            .map(|sheet| {
                if sheet.is_terminal {
                    return None;
                }
                let ext = display_extension(&sheet.title);
                let style = DocStyle::from_extension(&ext);
                doc_html(sheet, style, &ext)
            })
            .collect::<Vec<Option<String>>>()
    });

    // 初期選択は現在アクティブなシート
    let selected = use_state({
        let sheets = props.sheets.clone();
        let active = props.active_sheet_id.clone();
        move || {
            active
                .and_then(|id| sheets.iter().position(|s| s.id == id))
                .unwrap_or(0)
        }
    });

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

    let handle_close = {
        let on_close = props.on_close.clone();
        let is_closing = is_closing.clone();
        Callback::from(move |_: ()| {
            is_closing.set(true);
            let cb = on_close.clone();
            gloo::timers::callback::Timeout::new(100, move || cb.emit(())).forget();
        })
    };

    // Alt+R の再押下による外部クローズ（初回マウント時はスキップ）
    {
        let handle_close_trigger = handle_close.clone();
        let close_trigger = props.close_trigger;
        let is_first_render = use_mut_ref(|| true);
        use_effect_with(close_trigger, move |_| {
            if *is_first_render.borrow() {
                *is_first_render.borrow_mut() = false;
            } else {
                handle_close_trigger.emit(());
            }
            || ()
        });
    }

    let confirm = {
        let on_select = props.on_select.clone();
        let sheets = props.sheets.clone();
        let selected = selected.clone();
        let is_closing = is_closing.clone();
        Callback::from(move |_: ()| {
            if let Some(sheet) = sheets.get(*selected) {
                let id = sheet.id.clone();
                is_closing.set(true);
                let cb = on_select.clone();
                gloo::timers::callback::Timeout::new(100, move || cb.emit(id.clone())).forget();
            }
        })
    };

    let on_keydown = {
        let selected = selected.clone();
        let total = props.sheets.len();
        let close = handle_close.clone();
        let confirm = confirm.clone();
        Callback::from(move |e: KeyboardEvent| {
            e.stop_propagation();
            let dir = match e.key().as_str() {
                "ArrowLeft" => Some(Dir::Left),
                "ArrowRight" => Some(Dir::Right),
                "ArrowUp" => Some(Dir::Up),
                "ArrowDown" => Some(Dir::Down),
                "Enter" => {
                    e.prevent_default();
                    confirm.emit(());
                    return;
                }
                "Escape" => {
                    e.prevent_default();
                    close.emit(());
                    return;
                }
                _ => None,
            };
            if let Some(dir) = dir {
                e.prevent_default();
                selected.set(move_selection(*selected, total, GRID_COLUMNS, dir));
            }
        })
    };

    // 選択カードを常に見える位置へスクロールする
    {
        let sel = *selected;
        use_effect_with(sel, move |sel| {
            let id = format!("leaf-switcher-card-{}", sel);
            if let Some(el) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id(&id))
            {
                let opts = web_sys::ScrollIntoViewOptions::new();
                opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
                el.scroll_into_view_with_scroll_into_view_options(&opts);
            }
            || ()
        });
    }

    html! {
        <div
            ref={root_ref}
            tabindex="0"
            onkeydown={on_keydown}
            class="fixed inset-0 z-[150] flex items-start justify-center outline-none pointer-events-auto"
            onclick={|e: MouseEvent| e.stop_propagation()}
        >
            <div class={classes!(
                "absolute", "inset-0", "bg-black",
                if *is_closing { "animate-backdrop-out" } else { "animate-backdrop-in" }
            )} onclick={handle_close.reform(|_| ())}></div>

            // 上端から幅80% / 高さ90% で 0.1 秒でロールダウン
            <div
                class={classes!(
                    "relative", "flex", "flex-col", "bg-gray-900", "border-2", "border-emerald-500",
                    "rounded-b-lg", "shadow-2xl", "overflow-hidden",
                    if *is_closing { "animate-roll-down-out" } else { "animate-roll-down-in" }
                )}
                style="width: 80%; height: 90%;"
                onclick={|e: MouseEvent| e.stop_propagation()}
            >
                <div class="flex items-center justify-between px-4 py-2 border-b border-white/10 flex-shrink-0">
                    <span class="text-[10px] font-black uppercase tracking-widest text-gray-500">
                        { i18n::t("project_sheets", lang) }
                    </span>
                    <div class="flex items-center gap-3 text-[10px] text-gray-500">
                        <span class="flex items-center gap-1"><kbd class="px-1 py-0.5 bg-gray-800 rounded text-gray-400 font-mono">{"←↑↓→"}</kbd>{ i18n::t("key_navigate", lang) }</span>
                        <span class="flex items-center gap-1"><kbd class="px-1 py-0.5 bg-gray-800 rounded text-gray-400 font-mono">{"Enter"}</kbd>{ i18n::t("key_confirm", lang) }</span>
                    </div>
                </div>

                <div class="flex-1 overflow-y-auto custom-scrollbar p-4">
                    if props.sheets.is_empty() {
                        <div class="h-full flex items-center justify-center text-gray-600 text-xs">
                            { i18n::t("no_project_sheets", lang) }
                        </div>
                    } else {
                        // 横 4 列固定・縦スクロール
                        <div class="grid grid-cols-4 gap-4">
                            { for props.sheets.iter().enumerate().map(|(i, sheet)| {
                                let is_sel = i == *selected;
                                let is_active = props.active_sheet_id.as_deref() == Some(sheet.id.as_str());
                                let ext = display_extension(&sheet.title);
                                let style = DocStyle::from_extension(&ext);
                                let sel_click = selected.clone();
                                let confirm_dbl = confirm.clone();
                                let sel_dbl = selected.clone();
                                html! {
                                    <div
                                        id={format!("leaf-switcher-card-{}", i)}
                                        onclick={move |_| sel_click.set(i)}
                                        ondblclick={move |_| { sel_dbl.set(i); confirm_dbl.emit(()); }}
                                        class={classes!(
                                            // 書類を模したカード（横:縦 = 1:1）
                                            "group", "relative", "aspect-square", "rounded-md", "overflow-hidden",
                                            "cursor-pointer", "transition-all", "duration-150", "flex", "flex-col",
                                            // 選択中は枠線を太くし、それ以外は暗くして見分けやすくする
                                            if is_sel {
                                                vec!["border-4", "border-emerald-400", "ring-4", "ring-emerald-500/30", "scale-[1.02]", "shadow-xl", "opacity-100"]
                                            } else {
                                                vec!["border-2", "border-white/20", "hover:border-emerald-500/60", "opacity-50", "hover:opacity-90"]
                                            }
                                        )}
                                    >
                                        // 書類の上端（タブ色 + 1行目の見出し + 拡張子）
                                        // ※ ターミナルはアイコン表示に切り替える
                                        <div class={classes!("flex", "items-center", "gap-1", "px-2", head_pad_class, "bg-gray-800", "border-b", "border-white/10", "flex-shrink-0")}>
                                            <span class="w-2 h-2 rounded-full flex-shrink-0" style={format!("background-color: {};", sheet.tab_color)}></span>
                                            // 1行目を可能な限り表示する（入り切らない分は省略）
                                            <span class={classes!("flex-1", "min-w-0", "truncate", head_text_class, "font-bold",
                                                if is_sel { "text-white" } else { "text-gray-300" })}
                                                title={first_line(sheet)}
                                            >
                                                { first_line(sheet) }
                                            </span>
                                            <span class={classes!("px-1", "rounded", badge_text_class, "font-black", "uppercase", "tracking-tighter", "flex-shrink-0",
                                                if sheet.is_terminal { "bg-gray-500/20 text-gray-300" } else { "" },
                                                match style {
                                                    DocStyle::Markdown => "bg-sky-500/20 text-sky-300",
                                                    DocStyle::Code => "bg-amber-500/20 text-amber-300",
                                                    DocStyle::Text => "bg-emerald-500/20 text-emerald-300",
                                                })}>
                                                { if ext.is_empty() { "—".to_string() } else { ext.clone() } }
                                            </span>
                                        </div>
                                        // 書類の中身（プレビュー画面と同じレンダリング。結果はキャッシュ済み）
                                        // 選択中のカードだけスクロールできるようにする。
                                        // overscroll-contain で、端まで来てもグリッド側へスクロールが
                                        // 波及しないようにしている。
                                        <div class={classes!("flex-1", "min-h-0",
                                            if sheet.is_terminal { "bg-[#1d2021]" } else { "bg-[#fdf6e3]" },
                                            if is_sel { vec!["overflow-y-auto", "overscroll-contain", "custom-scrollbar"] }
                                            else { vec!["overflow-hidden"] })}>
                                            {
                                                if sheet.is_terminal {
                                                    // ターミナルは中身を描画せず、ターミナルらしい見た目にする
                                                    html! {
                                                        <div class="w-full h-full flex items-center justify-center text-emerald-500/70">
                                                            <svg xmlns="http://www.w3.org/2000/svg" class="h-10 w-10" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M4 5h16a1 1 0 011 1v12a1 1 0 01-1 1H4a1 1 0 01-1-1V6a1 1 0 011-1z" /></svg>
                                                        </div>
                                                    }
                                                } else if !*content_ready {
                                                    // 描画準備中は白紙のまま（レイアウトは確定しているのでガタつかない）
                                                    html! {}
                                                } else {
                                                match rendered_docs.get(i).and_then(|d| d.clone()) {
                                                    Some(html_str) => html! {
                                                        <div class="markdown-body leaf-switcher-doc max-w-none">
                                                            { Html::from_html_unchecked(AttrValue::from(html_str)) }
                                                        </div>
                                                    },
                                                    None => html! {
                                                        <div class="text-[8px] text-gray-600 italic p-2">{ "(empty)" }</div>
                                                    },
                                                }
                                                }
                                            }
                                        </div>
                                        // 現在開いているシートの印
                                        if is_active {
                                            <div class={classes!("absolute", "bottom-1", "right-1", "px-1", "py-0.5", "rounded", "bg-emerald-600", "text-white", badge_text_class, "font-black", "uppercase", "tracking-tighter")}>
                                                { i18n::t("active_sheet", lang) }
                                            </div>
                                        }
                                    </div>
                                }
                            })}
                        </div>
                    }
                </div>
            </div>
        </div>
    }
}

/// カードのヘッダーに出す見出し。本文の 1 行目（空行はスキップ）を使い、
/// 本文が空ならファイル名で代替する。
/// Markdown の見出し記号は邪魔になるので落とす。
fn first_line(sheet: &SwitcherSheet) -> String {
    sheet
        .content
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| sheet.title.clone())
}

/// 拡張子に応じてレンダリングした「書類の中身」の HTML を返す。
///
/// プレビュー画面（preview.rs）と同じレンダラを使うことで、
/// Markdown は整形済みの見た目、ソースコードはシンタックスハイライト付きになる。
/// カードが小さいため、冒頭の一定行数だけを対象にする（重い描画を避ける）。
///
/// 本文が空の場合は None。
fn doc_html(sheet: &SwitcherSheet, style: DocStyle, ext: &str) -> Option<String> {
    let head: String = sheet
        .content
        .lines()
        .take(DOC_PREVIEW_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    if head.trim().is_empty() {
        return None;
    }

    // プレビュー画面（preview.rs）と同じ分岐にする。
    // Markdown は marked、それ以外は highlight.js で描画される。
    let rendered = if style == DocStyle::Markdown {
        crate::js_interop::render_markdown(&head)
    } else {
        let lang = ext.to_lowercase();
        let code_html = crate::js_interop::highlight_code(&head, &lang);
        format!(
            r#"<pre><code class="hljs language-{}">{}</code></pre>"#,
            lang, code_html
        )
    };
    Some(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 12 枚（4 列 x 3 行ちょうど）を基本形とする
    const FULL: usize = 12;

    #[test]
    fn left_and_right_move_one_step_across_rows() {
        assert_eq!(move_selection(0, FULL, 4, Dir::Right), 1);
        // 行末（index 3）の右は次の行の先頭（index 4）
        assert_eq!(move_selection(3, FULL, 4, Dir::Right), 4);
        assert_eq!(move_selection(4, FULL, 4, Dir::Left), 3);
    }

    #[test]
    fn left_and_right_stop_at_both_ends() {
        assert_eq!(move_selection(0, FULL, 4, Dir::Left), 0);
        assert_eq!(move_selection(11, FULL, 4, Dir::Right), 11);
    }

    #[test]
    fn up_and_down_move_one_row() {
        assert_eq!(move_selection(0, FULL, 4, Dir::Down), 4);
        assert_eq!(move_selection(4, FULL, 4, Dir::Down), 8);
        assert_eq!(move_selection(8, FULL, 4, Dir::Up), 4);
        assert_eq!(move_selection(4, FULL, 4, Dir::Up), 0);
    }

    #[test]
    fn up_on_first_row_stays() {
        for i in 0..4 {
            assert_eq!(move_selection(i, FULL, 4, Dir::Up), i);
        }
    }

    #[test]
    fn down_on_last_row_stays() {
        for i in 8..12 {
            assert_eq!(move_selection(i, FULL, 4, Dir::Down), i);
        }
    }

    #[test]
    fn down_into_incomplete_last_row_snaps_to_last_card() {
        // 10 枚 = 4 + 4 + 2。index 6（2行目3列目）の真下は存在しない
        assert_eq!(move_selection(6, 10, 4, Dir::Down), 9, "末尾のカードへ寄せる");
        assert_eq!(move_selection(7, 10, 4, Dir::Down), 9);
        // 真下が存在する場合はそのまま真下へ
        assert_eq!(move_selection(5, 10, 4, Dir::Down), 9);
        assert_eq!(move_selection(4, 10, 4, Dir::Down), 8);
        // 既に最終行なら動かない
        assert_eq!(move_selection(8, 10, 4, Dir::Down), 8);
        assert_eq!(move_selection(9, 10, 4, Dir::Down), 9);
    }

    #[test]
    fn single_row_has_no_vertical_movement() {
        assert_eq!(move_selection(1, 3, 4, Dir::Up), 1);
        assert_eq!(move_selection(1, 3, 4, Dir::Down), 1);
        assert_eq!(move_selection(1, 3, 4, Dir::Right), 2);
        assert_eq!(move_selection(2, 3, 4, Dir::Right), 2);
    }

    #[test]
    fn empty_and_out_of_range_inputs_are_safe() {
        // 0 枚: どの方向でも 0（パニックしない）
        for dir in [Dir::Left, Dir::Right, Dir::Up, Dir::Down] {
            assert_eq!(move_selection(0, 0, 4, dir), 0);
            assert_eq!(move_selection(5, 0, 4, dir), 0);
        }
        // 範囲外の current は末尾に丸められる
        assert_eq!(move_selection(99, FULL, 4, Dir::Left), 10);
        assert_eq!(move_selection(99, FULL, 4, Dir::Right), 11);
        // columns=0 は 1 列として扱う
        assert_eq!(move_selection(0, FULL, 0, Dir::Down), 1);
    }

    #[test]
    fn doc_style_is_chosen_by_extension() {
        assert_eq!(DocStyle::from_extension("md"), DocStyle::Markdown);
        assert_eq!(DocStyle::from_extension(".markdown"), DocStyle::Markdown);
        assert_eq!(DocStyle::from_extension("MD"), DocStyle::Markdown);
        assert_eq!(DocStyle::from_extension("txt"), DocStyle::Text);
        assert_eq!(DocStyle::from_extension(""), DocStyle::Text);
        assert_eq!(DocStyle::from_extension("rs"), DocStyle::Code);
        assert_eq!(DocStyle::from_extension("js"), DocStyle::Code);
    }

    fn sheet(title: &str, content: &str) -> SwitcherSheet {
        SwitcherSheet {
            id: "id".to_string(),
            title: title.to_string(),
            content: content.to_string(),
            tab_color: "#fff".to_string(),
            is_terminal: false,
        }
    }

    #[test]
    fn header_shows_first_non_empty_line() {
        assert_eq!(first_line(&sheet("a.txt", "会議メモ\n2行目")), "会議メモ");
        // 先頭の空行はスキップする
        assert_eq!(first_line(&sheet("a.txt", "\n\n  本題  \n次")), "本題");
    }

    #[test]
    fn header_strips_markdown_heading_marks() {
        assert_eq!(first_line(&sheet("a.md", "# 見出し")), "見出し");
        assert_eq!(first_line(&sheet("a.md", "### 小見出し\n本文")), "小見出し");
    }

    #[test]
    fn header_falls_back_to_file_name() {
        // 本文が空、または記号だけで中身が無い場合はファイル名を使う
        assert_eq!(first_line(&sheet("memo.txt", "")), "memo.txt");
        assert_eq!(first_line(&sheet("memo.txt", "\n \n")), "memo.txt");
        assert_eq!(first_line(&sheet("memo.md", "###")), "memo.md");
    }

    #[test]
    fn extension_is_extracted_for_display() {
        assert_eq!(display_extension("memo.txt"), "TXT");
        assert_eq!(display_extension("readme.md"), "MD");
        assert_eq!(display_extension("archive.tar.gz"), "GZ");
        // 拡張子が無い / ドットのみのケース
        assert_eq!(display_extension("Makefile"), "");
        assert_eq!(display_extension("noext."), "");
        assert_eq!(display_extension(".hidden"), "");
    }
}
