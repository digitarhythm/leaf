use yew::prelude::*;
use gloo::timers::callback::Timeout;

#[derive(Clone, PartialEq)]
pub struct DialogOption {
    pub id: usize,
    pub label: String,
}

#[derive(Properties, PartialEq)]
pub struct CustomDialogProps {
    pub title: String,
    pub message: String,
    pub options: Vec<DialogOption>,
    pub on_confirm: Callback<usize>,
    #[prop_or_default]
    pub on_cancel: Option<Callback<()>>,
    #[prop_or_default]
    pub on_start_processing: Callback<()>,
}

#[derive(PartialEq, Clone, Copy)]
enum CustomDialogFocus {
    Options(usize),
    Ok,
    Cancel,
}

#[function_component(CustomDialog)]
pub fn custom_dialog(props: &CustomDialogProps) -> Html {
    let focused = use_state(|| CustomDialogFocus::Options(0));
    let is_fading_out = use_state(|| false);
    let root_ref = use_node_ref();

    {
        let root = root_ref.clone();
        use_effect_with((), move |_| {
            let r = root.clone();
            Timeout::new(10, move || {
                if let Some(el) = r.cast::<web_sys::HtmlElement>() { let _ = el.focus(); }
            }).forget();
            || ()
        });
    }

    let on_confirm = {
        let on_confirm = props.on_confirm.clone();
        let on_start = props.on_start_processing.clone();
        let focused = focused.clone();
        let is_fading_out = is_fading_out.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            let choice = match *focused {
                CustomDialogFocus::Options(idx) => Some(idx),
                CustomDialogFocus::Ok => Some(0), // デフォルト
                _ => None,
            };
            if let Some(idx) = choice {
                is_fading_out.set(true);
                on_start.emit(());
                let on_confirm = on_confirm.clone();
                Timeout::new(200, move || { on_confirm.emit(idx); }).forget();
            }
        })
    };

    let on_keydown = {
        let focused = focused.clone();
        let options_len = props.options.len();
        let on_cfm = on_confirm.clone();
        let on_cxl = props.on_cancel.clone();
        Callback::from(move |e: web_sys::KeyboardEvent| {
            e.stop_propagation();
            match e.key().as_str() {
                "ArrowUp" => {
                    e.prevent_default();
                    match *focused {
                        CustomDialogFocus::Options(idx) if idx > 0 => focused.set(CustomDialogFocus::Options(idx - 1)),
                        CustomDialogFocus::Ok | CustomDialogFocus::Cancel => focused.set(CustomDialogFocus::Options(options_len - 1)),
                        _ => {}
                    }
                }
                "ArrowDown" => {
                    e.prevent_default();
                    match *focused {
                        CustomDialogFocus::Options(idx) if idx + 1 < options_len => focused.set(CustomDialogFocus::Options(idx + 1)),
                        CustomDialogFocus::Options(_) => focused.set(CustomDialogFocus::Ok),
                        _ => {}
                    }
                }
                "ArrowLeft" | "ArrowRight" | "Tab" => {
                    e.prevent_default();
                    match *focused {
                        CustomDialogFocus::Ok => focused.set(CustomDialogFocus::Cancel),
                        CustomDialogFocus::Cancel => focused.set(CustomDialogFocus::Ok),
                        CustomDialogFocus::Options(_) => focused.set(CustomDialogFocus::Ok),
                    }
                }
                "Enter" => {
                    e.prevent_default();
                    match *focused {
                        CustomDialogFocus::Cancel => { if let Some(cb) = &on_cxl { cb.emit(()); } }
                        _ => on_cfm.emit(MouseEvent::new("click").unwrap()),
                    }
                }
                "Escape" => {
                    e.prevent_default();
                    if let Some(cb) = &on_cxl { cb.emit(()); }
                }
                _ => {}
            }
        })
    };

    html! {
        <div 
            ref={root_ref}
            class={classes!(
                "fixed", "inset-0", "z-[100]", "flex", "items-center", "justify-center", "bg-black/50", "backdrop-blur-md", "p-4", "outline-none", "pointer-events-auto",
                if *is_fading_out { "opacity-0 transition-opacity duration-200" } else { "" }
            )}
            tabindex="0"
            onkeydown={on_keydown}
            onclick={|e: MouseEvent| e.stop_propagation()}
        >
            <div class={classes!(
                "bg-gray-800", "border", "border-gray-700", "rounded-lg", "shadow-2xl", "w-full", "max-w-md", "overflow-hidden",
                if *is_fading_out { "animate-dialog-out" } else { "animate-dialog-in" }
            )} onclick={|e: MouseEvent| e.stop_propagation()}>
                <div class="px-6 py-4 border-b border-gray-700 bg-gray-800/50">
                    <h3 class="text-xl font-bold text-white">{ &props.title }</h3>
                </div>
                
                <div class="px-6 py-6">
                    <p class="text-gray-300 mb-6 whitespace-pre-wrap">{ &props.message }</p>
                    
                    <div class="space-y-3">
                        { for props.options.iter().enumerate().map(|(idx, opt)| {
                            let is_selected = matches!(*focused, CustomDialogFocus::Options(i) if i == idx);
                            html! {
                                <label class={classes!(
                                    "flex", "items-center", "p-3", "rounded-md", "border", "cursor-pointer", "transition-colors",
                                    if is_selected { vec!["bg-blue-600/20", "border-blue-500", "text-white"] } else { vec!["bg-gray-700/30", "border-gray-600", "text-gray-400", "hover:bg-gray-700/50"] }
                                )} onclick={|e: MouseEvent| e.stop_propagation()}>
                                    <input 
                                        type="radio" 
                                        tabindex="-1"
                                        class="w-4 h-4 text-blue-600 bg-gray-700 border-gray-600 focus:ring-0"
                                        name="dialog-option"
                                        checked={is_selected}
                                        onclick={let f = focused.clone(); move |e: MouseEvent| { e.stop_propagation(); f.set(CustomDialogFocus::Options(idx)) }}
                                    />
                                    <span class="ml-3 font-medium">{ &opt.label }</span>
                                </label>
                            }
                        }) }
                    </div>
                </div>

                <div class="px-6 py-2 bg-gray-900/50 flex justify-end space-x-3">
                    if let Some(cancel_cb) = &props.on_cancel {
                        <button 
                            tabindex="-1"
                            onclick={let cb = cancel_cb.clone(); move |e: MouseEvent| { e.stop_propagation(); cb.emit(()); }}
                            class={classes!(
                                "px-6", "py-2", "rounded-md", "transition-colors", "border-[3px]",
                                if *focused == CustomDialogFocus::Cancel { vec!["bg-gray-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] }
                                else { vec!["bg-gray-700", "text-gray-300", "border-transparent"] }
                            )}
                        >
                            { "Cancel" }
                        </button>
                    }
                    <button 
                        tabindex="-1"
                        onclick={on_confirm}
                        class={classes!(
                            "px-6", "py-2", "rounded-md", "transition-colors", "shadow-lg", "border-[3px]",
                            if *focused == CustomDialogFocus::Ok { vec!["bg-blue-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] }
                            else { vec!["bg-blue-600", "text-white", "border-transparent"] }
                        )}
                    >
                        { "OK" }
                    </button>
                </div>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct InputDialogProps {
    pub title: String,
    pub message: String,
    pub on_confirm: Callback<String>,
    pub on_cancel: Callback<()>,
}

#[derive(PartialEq, Clone, Copy)]
enum InputDialogFocus {
    Input,
    Ok,
    Cancel,
}

#[function_component(InputDialog)]
pub fn input_dialog(props: &InputDialogProps) -> Html {
    let text = use_state(|| "".to_string());
    let root_ref = use_node_ref();
    let input_ref = use_node_ref();
    let focused = use_state(|| InputDialogFocus::Input);

    {
        let input_r = input_ref.clone();
        use_effect_with((), move |_| {
            let r = input_r.clone();
            Timeout::new(10, move || {
                if let Some(el) = r.cast::<web_sys::HtmlElement>() { 
                    let _ = el.focus(); 
                }
            }).forget();
            || ()
        });
    }

    let on_keydown = {
        let on_confirm = props.on_confirm.clone();
        let on_cancel = props.on_cancel.clone();
        let text = text.clone();
        let focused = focused.clone();
        let input_r = input_ref.clone();
        Callback::from(move |e: KeyboardEvent| {
            e.stop_propagation();
            match e.key().as_str() {
                "Tab" => {
                    e.prevent_default();
                    match *focused {
                        InputDialogFocus::Input => focused.set(InputDialogFocus::Ok),
                        InputDialogFocus::Ok => focused.set(InputDialogFocus::Cancel),
                        InputDialogFocus::Cancel => {
                            focused.set(InputDialogFocus::Input);
                            if let Some(el) = input_r.cast::<web_sys::HtmlElement>() { let _ = el.focus(); }
                        }
                    }
                }
                "ArrowRight" => {
                    if *focused != InputDialogFocus::Input {
                        e.prevent_default();
                        focused.set(InputDialogFocus::Ok);
                    }
                }
                "ArrowLeft" => {
                    if *focused != InputDialogFocus::Input {
                        e.prevent_default();
                        focused.set(InputDialogFocus::Cancel);
                    }
                }
                "Enter" => {
                    e.prevent_default();
                    match *focused {
                        InputDialogFocus::Cancel => on_cancel.emit(()),
                        _ => {
                            // 入力が空でない場合のみ送信を許可
                            if !text.trim().is_empty() {
                                on_confirm.emit((*text).clone());
                            }
                        }
                    }
                }
                "Escape" => {
                    e.prevent_default();
                    on_cancel.emit(());
                }
                _ => {}
            }
        })
    };

    html! {
        <div ref={root_ref} tabindex="0" onkeydown={on_keydown} onclick={|e: MouseEvent| e.stop_propagation()} class="fixed inset-0 z-[200] flex items-center justify-center bg-black/50 backdrop-blur-md p-4 outline-none pointer-events-auto">
            <div class="bg-gray-800 border border-gray-700 rounded-lg shadow-2xl w-full max-w-sm overflow-hidden animate-dialog-in" onclick={|e: MouseEvent| e.stop_propagation()}>
                <div class="px-6 py-4 border-b border-gray-700 bg-gray-800/50">
                    <h3 class="text-lg font-bold text-white">{ &props.title }</h3>
                </div>
                
                <div class="px-6 py-6 space-y-4">
                    <p class="text-sm text-gray-300">{ &props.message }</p>
                    <input 
                        ref={input_ref}
                        type="text" 
                        value={(*text).clone()}
                        oninput={let t = text.clone(); let f = focused.clone(); move |ev: InputEvent| { 
                            let input: web_sys::HtmlInputElement = ev.target_unchecked_into(); 
                            t.set(input.value()); 
                            f.set(InputDialogFocus::Input);
                        }}
                        onclick={|e: MouseEvent| e.stop_propagation()}
                        class={classes!(
                            "w-full", "bg-gray-900", "border", "rounded-md", "px-4", "py-2", "text-white", "focus:outline-none", "transition-all",
                            if *focused == InputDialogFocus::Input { "border-lime-400 ring-2 ring-lime-400" } else { "border-gray-700" }
                        )}
                    />
                </div>

                <div class="px-6 py-2 bg-gray-900/50 flex justify-end space-x-3">
                    <button 
                        tabindex="-1"
                        onclick={let cb = props.on_cancel.clone(); move |e: MouseEvent| { e.stop_propagation(); cb.emit(()); }}
                        class={classes!(
                            "px-6", "py-2", "rounded-md", "transition-colors", "border-[3px]",
                            if *focused == InputDialogFocus::Cancel { vec!["bg-gray-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] }
                            else { vec!["bg-gray-700", "text-gray-300", "border-transparent"] }
                        )}
                    >
                        { "Cancel" }
                    </button>
                    <button 
                        tabindex="-1"
                        disabled={text.trim().is_empty()}
                        onclick={let oc = props.on_confirm.clone(); let t = text.clone(); move |e: MouseEvent| { 
                            if !t.trim().is_empty() {
                                e.stop_propagation(); oc.emit((*t).clone()); 
                            }
                        }}
                        class={classes!(
                            "px-6", "py-2", "rounded-md", "transition-colors", "shadow-lg", "border-[3px]",
                            if text.trim().is_empty() { "opacity-50 cursor-not-allowed" } else { "" },
                            if *focused == InputDialogFocus::Ok { vec!["bg-blue-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] }
                            else { vec!["bg-blue-600", "text-white", "border-transparent"] }
                        )}
                    >
                        { "OK" }
                    </button>
                </div>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct ConfirmDialogProps {
    pub title: String,
    pub message: String,
    pub on_confirm: Callback<()>,
    pub on_cancel: Callback<()>,
    #[prop_or_else(|| "OK".to_string())]
    pub ok_label: String,
    #[prop_or_else(|| "Cancel".to_string())]
    pub cancel_label: String,
}

#[derive(PartialEq, Clone, Copy)]
enum ConfirmFocus {
    Ok,
    Cancel,
}

#[function_component(ConfirmDialog)]
pub fn confirm_dialog(props: &ConfirmDialogProps) -> Html {
    let focused = use_state(|| ConfirmFocus::Ok);
    let root_ref = use_node_ref();

    {
        let root = root_ref.clone();
        use_effect_with((), move |_| {
            let r = root.clone();
            Timeout::new(10, move || {
                if let Some(el) = r.cast::<web_sys::HtmlElement>() { let _ = el.focus(); }
            }).forget();
            || ()
        });
    }

    let on_keydown = {
        let focused = focused.clone();
        let on_cfm = props.on_confirm.clone();
        let on_cxl = props.on_cancel.clone();
        Callback::from(move |e: web_sys::KeyboardEvent| {
            e.stop_propagation();
            match e.key().as_str() {
                "Escape" => {
                    e.prevent_default();
                    on_cxl.emit(());
                }
                "Tab" | "ArrowLeft" | "ArrowRight" => {
                    e.prevent_default();
                    if *focused == ConfirmFocus::Ok { focused.set(ConfirmFocus::Cancel); }
                    else { focused.set(ConfirmFocus::Ok); }
                }
                "Enter" => {
                    e.prevent_default();
                    if *focused == ConfirmFocus::Ok { on_cfm.emit(()); }
                    else { on_cxl.emit(()); }
                }
                _ => {}
            }
        })
    };

    html! {
        <div 
            ref={root_ref}
            class="fixed inset-0 z-[200] flex items-center justify-center bg-black/50 backdrop-blur-md p-4 animate-in fade-in duration-200 outline-none pointer-events-auto" 
            tabindex="0"
            onkeydown={on_keydown}
            onclick={|e: MouseEvent| e.stop_propagation()}
        >
            <div class="bg-gray-800 border border-gray-700 rounded-lg shadow-2xl w-full max-w-sm overflow-hidden animate-dialog-in" onclick={|e: MouseEvent| e.stop_propagation()}>
                <div class="px-6 py-4 border-b border-gray-700 bg-gray-800/50">
                    <h3 class="text-lg font-bold text-white">{ &props.title }</h3>
                </div>
                
                <div class="px-6 py-8">
                    <p class="text-sm text-gray-300 whitespace-pre-wrap">{ &props.message }</p>
                </div>

                <div class="px-6 py-3 bg-gray-900/50 flex justify-end space-x-3">
                    <button 
                        tabindex="-1"
                        onclick={let cb = props.on_cancel.clone(); move |e: MouseEvent| { e.stop_propagation(); cb.emit(()); }}
                        class={classes!(
                            "px-6", "py-2", "rounded-md", "transition-colors", "outline-none", "border-[3px]",
                            if *focused == ConfirmFocus::Cancel { vec!["bg-gray-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] } 
                            else { vec!["bg-gray-700", "text-gray-300", "border-transparent"] }
                        )}
                    >
                        { &props.cancel_label }
                    </button>
                    <button 
                        tabindex="-1"
                        onclick={let cb = props.on_confirm.clone(); move |e: MouseEvent| { e.stop_propagation(); cb.emit(()); }}
                        class={classes!(
                            "px-8", "py-2", "rounded-md", "transition-colors", "shadow-lg", "outline-none", "border-[3px]",
                            if *focused == ConfirmFocus::Ok { vec!["bg-red-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] } 
                            else { vec!["bg-red-600", "text-white", "border-transparent"] }
                        )}
                    >
                        { &props.ok_label }
                    </button>
                </div>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct NameConflictDialogProps {
    pub title: String,
    pub message: String,
    pub current_name: String,
    pub on_confirm: Callback<(usize, String)>, // (choice_id, new_name)
    pub on_cancel: Callback<()>,
    pub labels: Vec<String>, // [overwrite, guid, specify]
}

#[derive(PartialEq, Clone, Copy)]
enum NameConflictFocus {
    Overwrite,
    Guid,
    Specify,
    Input,
    Ok,
    Cancel,
}

#[function_component(NameConflictDialog)]
pub fn name_conflict_dialog(props: &NameConflictDialogProps) -> Html {
    let focused = use_state(|| NameConflictFocus::Overwrite);
    let selected_opt = use_state(|| 0usize);
    let input_val = use_state(|| props.current_name.clone());
    let root_ref = use_node_ref();
    let input_ref = use_node_ref();

    {
        let root = root_ref.clone();
        use_effect_with((), move |_| {
            let r = root.clone();
            Timeout::new(10, move || {
                if let Some(el) = r.cast::<web_sys::HtmlElement>() { let _ = el.focus(); }
            }).forget();
            || ()
        });
    }

    let on_keydown = {
        let focused = focused.clone();
        let selected_opt = selected_opt.clone();
        let on_cfm = props.on_confirm.clone();
        let on_cxl = props.on_cancel.clone();
        let input_val = input_val.clone();
        let input_r = input_ref.clone();
        Callback::from(move |e: KeyboardEvent| {
            e.stop_propagation();
            match e.key().as_str() {
                "ArrowUp" => {
                    e.prevent_default();
                    match *focused {
                        NameConflictFocus::Guid => { focused.set(NameConflictFocus::Overwrite); selected_opt.set(0); }
                        NameConflictFocus::Specify | NameConflictFocus::Input => { focused.set(NameConflictFocus::Guid); selected_opt.set(1); }
                        NameConflictFocus::Ok | NameConflictFocus::Cancel => { focused.set(NameConflictFocus::Specify); selected_opt.set(2); }
                        _ => {}
                    }
                }
                "ArrowDown" => {
                    e.prevent_default();
                    match *focused {
                        NameConflictFocus::Overwrite => { focused.set(NameConflictFocus::Guid); selected_opt.set(1); }
                        NameConflictFocus::Guid => { focused.set(NameConflictFocus::Specify); selected_opt.set(2); }
                        NameConflictFocus::Specify | NameConflictFocus::Input => focused.set(NameConflictFocus::Ok),
                        _ => {}
                    }
                }
                "ArrowLeft" | "ArrowRight" | "Tab" => {
                    e.prevent_default();
                    match *focused {
                        NameConflictFocus::Ok => focused.set(NameConflictFocus::Cancel),
                        NameConflictFocus::Cancel => focused.set(NameConflictFocus::Ok),
                        NameConflictFocus::Specify => {
                            focused.set(NameConflictFocus::Input);
                            let r = input_r.clone();
                            Timeout::new(10, move || { if let Some(el) = r.cast::<web_sys::HtmlElement>() { let _ = el.focus(); } }).forget();
                        }
                        NameConflictFocus::Input => {
                            focused.set(NameConflictFocus::Specify);
                        }
                        _ => focused.set(NameConflictFocus::Ok),
                    }
                }
                "Enter" => {
                    e.prevent_default();
                    match *focused {
                        NameConflictFocus::Cancel => on_cxl.emit(()),
                        _ => on_cfm.emit((*selected_opt, (*input_val).clone())),
                    }
                }
                "Escape" => {
                    e.prevent_default();
                    on_cxl.emit(());
                }
                _ => {}
            }
        })
    };

    html! {
        <div ref={root_ref} tabindex="0" onkeydown={on_keydown} onclick={|e: MouseEvent| e.stop_propagation()} class="fixed inset-0 z-[200] flex items-center justify-center bg-black/50 backdrop-blur-md p-4 animate-in fade-in duration-200 outline-none pointer-events-auto">
            <div class="bg-gray-800 border border-gray-700 rounded-lg shadow-2xl w-full max-w-md overflow-hidden animate-dialog-in" onclick={|e: MouseEvent| e.stop_propagation()}>
                <div class="px-6 py-4 border-b border-gray-700 bg-gray-800/50">
                    <h3 class="text-lg font-bold text-white">{ &props.title }</h3>
                </div>
                
                <div class="px-6 py-6 space-y-4">
                    <p class="text-sm text-gray-300 mb-4">{ &props.message }</p>
                    
                    <div class="space-y-3">
                        <label class={classes!(
                            "flex", "items-center", "p-3", "rounded-md", "border", "cursor-pointer", "transition-colors",
                            if *focused == NameConflictFocus::Overwrite { vec!["bg-blue-600/20", "border-lime-400", "text-white"] } 
                            else if *selected_opt == 0 { vec!["bg-slate-700/50", "border-blue-500/50", "text-gray-200"] }
                            else { vec!["bg-gray-700/30", "border-gray-600", "text-gray-400"] }
                        )} onclick={|e: MouseEvent| e.stop_propagation()}>
                            <input type="radio" tabindex="-1" name="nc-opt" checked={*selected_opt == 0} onclick={let s = selected_opt.clone(); let f = focused.clone(); move |e: MouseEvent| { e.stop_propagation(); s.set(0); f.set(NameConflictFocus::Overwrite); }} class="w-4 h-4 text-blue-600 bg-gray-700 border-gray-600" />
                            <span class="ml-3 font-medium">{ &props.labels[0] }</span>
                        </label>

                        <label class={classes!(
                            "flex", "items-center", "p-3", "rounded-md", "border", "cursor-pointer", "transition-colors",
                            if *focused == NameConflictFocus::Guid { vec!["bg-blue-600/20", "border-lime-400", "text-white"] } 
                            else if *selected_opt == 1 { vec!["bg-slate-700/50", "border-blue-500/50", "text-gray-200"] }
                            else { vec!["bg-gray-700/30", "border-gray-600", "text-gray-400"] }
                        )} onclick={|e: MouseEvent| e.stop_propagation()}>
                            <input type="radio" tabindex="-1" name="nc-opt" checked={*selected_opt == 1} onclick={let s = selected_opt.clone(); let f = focused.clone(); move |e: MouseEvent| { e.stop_propagation(); s.set(1); f.set(NameConflictFocus::Guid); }} class="w-4 h-4 text-blue-600 bg-gray-700 border-gray-600" />
                            <span class="ml-3 font-medium">{ &props.labels[1] }</span>
                        </label>

                        <label class={classes!(
                            "flex", "flex-col", "p-3", "rounded-md", "border", "cursor-pointer", "transition-colors",
                            if *focused == NameConflictFocus::Specify || *focused == NameConflictFocus::Input { vec!["bg-blue-600/20", "border-lime-400", "text-white"] } 
                            else if *selected_opt == 2 { vec!["bg-slate-700/50", "border-blue-500/50", "text-gray-200"] }
                            else { vec!["bg-gray-700/30", "border-gray-600", "text-gray-400"] }
                        )} onclick={|e: MouseEvent| e.stop_propagation()}>
                            <div class="flex items-center w-full">
                                <input type="radio" tabindex="-1" name="nc-opt" checked={*selected_opt == 2} onclick={let s = selected_opt.clone(); let f = focused.clone(); move |e: MouseEvent| { e.stop_propagation(); s.set(2); f.set(NameConflictFocus::Specify); }} class="w-4 h-4 text-blue-600 bg-gray-700 border-gray-600" />
                                <span class="ml-3 font-medium">{ &props.labels[2] }</span>
                            </div>
                            <div class="mt-2 ml-7">
                                <input 
                                    ref={input_ref}
                                    type="text" 
                                    tabindex="-1"
                                    value={(*input_val).clone()}
                                    oninput={let v = input_val.clone(); let s = selected_opt.clone(); let f = focused.clone(); move |ev: InputEvent| { let input: web_sys::HtmlInputElement = ev.target_unchecked_into(); v.set(input.value()); s.set(2); f.set(NameConflictFocus::Input); }}
                                    onclick={|e: MouseEvent| e.stop_propagation()}
                                    class={classes!(
                                        "w-full", "bg-gray-900", "border", "rounded-md", "px-3", "py-1.5", "text-white", "focus:outline-none",
                                        if *focused == NameConflictFocus::Input { "border-lime-400 ring-1 ring-lime-400" } else { "border-gray-700" }
                                    )}
                                    placeholder="Enter filename..."
                                />
                            </div>
                        </label>
                    </div>
                </div>

                <div class="px-6 py-3 bg-gray-900/50 flex justify-end space-x-3">
                    <button 
                        tabindex="-1"
                        onclick={let cb = props.on_cancel.clone(); move |e: MouseEvent| { e.stop_propagation(); cb.emit(()); }} 
                        class={classes!(
                            "px-6", "py-2", "rounded-md", "transition-colors", "border-[3px]",
                            if *focused == NameConflictFocus::Cancel { vec!["bg-gray-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] }
                            else { vec!["bg-gray-700", "text-gray-300", "border-transparent"] }
                        )}
                    >
                        { "Cancel" }
                    </button>
                    <button 
                        tabindex="-1"
                        onclick={let oc = props.on_confirm.clone(); let s = selected_opt.clone(); let v = input_val.clone(); move |e: MouseEvent| { e.stop_propagation(); oc.emit((*s, (*v).clone())); }} 
                        class={classes!(
                            "px-8", "py-2", "rounded-md", "transition-colors", "shadow-lg", "border-[3px]",
                            if *focused == NameConflictFocus::Ok { vec!["bg-blue-600", "text-white", "border-lime-400", "ring-1", "ring-lime-400"] }
                            else { vec!["bg-blue-600", "text-white", "border-transparent"] }
                        )}
                    >
                        { "OK" }
                    </button>
                </div>
            </div>
        </div>
    }
}
