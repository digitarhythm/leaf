use yew::prelude::*;
use wasm_bindgen::JsCast;
use gloo::timers::callback::Timeout;
use encoding_rs::{SHIFT_JIS, EUC_JP, ISO_2022_JP};
use crate::i18n::{self, Language};

#[derive(Properties, PartialEq)]
pub struct CharCodeDialogProps {
    /// カーソル位置の文字（JS文字列。サロゲートペアの場合は2バイト）
    pub char_str: String,
    pub on_close: Callback<()>,
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
}

/// encoding_rs でエンコードし、変換不能なら None を返す
fn encode_safe(encoding: &'static encoding_rs::Encoding, s: &str) -> Option<Vec<u8>> {
    let (bytes, _, had_errors) = encoding.encode(s);
    if had_errors { None } else { Some(bytes.into_owned()) }
}

/// ISO-2022-JP からエスケープシーケンスを除いた JIS X 0208 コードバイトを返す
fn get_jis_bytes(s: &str) -> Option<Vec<u8>> {
    let (bytes, _, had_errors) = ISO_2022_JP.encode(s);
    if had_errors { return None; }
    let b = bytes.as_ref();
    // ASCII は 1 バイトそのまま
    if b.len() == 1 {
        return Some(b.to_vec());
    }
    // JIS X 0208 文字: ESC $ B <j1> <j2> ESC ( B  (合計 8 バイト)
    // b[0]=0x1B b[1]=0x24 b[2]=0x42 b[3]=j1 b[4]=j2 ...
    if b.len() >= 5 && b[0] == 0x1B && b[1] == 0x24 {
        return Some(vec![b[3], b[4]]);
    }
    None
}

#[function_component(CharCodeDialog)]
pub fn char_code_dialog(props: &CharCodeDialogProps) -> Html {
    let lang = Language::detect();
    let is_closing = use_state(|| false);

    let handle_close = {
        let on_close = props.on_close.clone();
        let ic = is_closing.clone();
        Callback::from(move |_: ()| {
            ic.set(true);
            let cb = on_close.clone();
            Timeout::new(200, move || { cb.emit(()); }).forget();
        })
    };

    // ESC で閉じる
    {
        let hc = handle_close.clone();
        use_effect_with((), move |_| {
            let window = web_sys::window().unwrap();
            let mut opts = gloo::events::EventListenerOptions::run_in_capture_phase();
            opts.passive = false;
            let listener = gloo::events::EventListener::new_with_options(
                &window, "keydown", opts,
                move |e| {
                    let ke = e.unchecked_ref::<web_sys::KeyboardEvent>();
                    if ke.key() == "Escape" {
                        e.stop_immediate_propagation();
                        hc.emit(());
                    }
                },
            );
            Box::new(move || drop(listener)) as Box<dyn FnOnce()>
        });
    }

    // ── エンコード計算 ──────────────────────────────
    let ch_str = &props.char_str;
    let ch = ch_str.chars().next().unwrap_or('\0');

    let ucs4 = ch as u32;

    let mut utf8_buf = [0u8; 4];
    let utf8_len = ch.encode_utf8(&mut utf8_buf).len();
    let utf8_bytes = &utf8_buf[..utf8_len];

    let jis_bytes   = get_jis_bytes(ch_str);
    let sjis_bytes  = encode_safe(SHIFT_JIS, ch_str);
    let euc_bytes   = encode_safe(EUC_JP, ch_str);

    let na = i18n::t("not_available", lang);

    // ── JIS 4桁表示（2バイト = 4桁 hex、1バイトは "ASCII" 扱いで表示） ──
    let jis_str = jis_bytes.as_ref().map(|b| {
        if b.len() == 1 {
            format!("{:02X}  (ASCII)", b[0])
        } else {
            bytes_to_hex(b)
        }
    }).unwrap_or_else(|| na.clone());

    let sjis_str = sjis_bytes.as_ref().map(|b| bytes_to_hex(b)).unwrap_or_else(|| na.clone());
    let euc_str  = euc_bytes.as_ref().map(|b| bytes_to_hex(b)).unwrap_or_else(|| na.clone());
    let utf8_str = bytes_to_hex(utf8_bytes);
    let ucs4_str = format!("U+{:04X}  ({})", ucs4, ucs4);

    let anim_class = if *is_closing { "opacity-0 scale-95" } else { "opacity-100 scale-100" };

    // ── 行レンダラ ──────────────────────────────────
    let row = |label: &'static str, value: String| -> Html {
        html! {
            <div class="flex items-center gap-4 py-2 border-b border-[#3c3836]/40 last:border-0">
                <span class="w-20 shrink-0 text-[10px] text-gray-500 font-mono uppercase tracking-wider select-none">
                    { label }
                </span>
                <span class="text-[13px] font-mono text-[#d4be98] tracking-wider select-all">
                    { value }
                </span>
            </div>
        }
    };

    html! {
        <div class="fixed inset-0 z-[200] flex items-center justify-center">
            // Backdrop
            <div
                class={classes!(
                    "absolute", "inset-0", "bg-black/60",
                    "transition-opacity", "duration-200",
                    if *is_closing { "opacity-0" } else { "opacity-100" }
                )}
                onclick={{ let hc = handle_close.clone(); move |_| hc.emit(()) }}
            ></div>

            // Dialog
            <div class={classes!(
                "relative", "z-10", "w-full", "max-w-xs", "mx-4",
                "bg-[#1d2021]", "rounded-xl", "border", "border-[#3c3836]", "shadow-2xl",
                "transition-all", "duration-200", anim_class
            )}>
                // ヘッダー
                <div class="flex items-center justify-between px-5 py-3 border-b border-[#3c3836]">
                    <div class="flex items-center gap-4">
                        // 文字大きく表示
                        <span class="text-3xl leading-none select-all"
                              style="font-family: 'Noto Sans JP', 'Hiragino Sans', sans-serif;">
                            { ch_str.clone() }
                        </span>
                        <span class="text-[11px] text-gray-500 leading-tight">
                            { i18n::t("char_code_title", lang) }
                        </span>
                    </div>
                    <button
                        onclick={{ let hc = handle_close.clone(); move |_| hc.emit(()) }}
                        class="p-1 rounded hover:bg-[#3c3836] text-gray-400 hover:text-white transition-colors"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24"
                             stroke-width="2" stroke="currentColor" class="w-4 h-4">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                // ボディ
                <div class="px-5 py-3">
                    { row("Unicode",   ucs4_str) }
                    { row("UTF-8",     utf8_str) }
                    { row("JIS",       jis_str) }
                    { row("Shift_JIS", sjis_str) }
                    { row("EUC-JP",    euc_str) }
                </div>
            </div>
        </div>
    }
}
