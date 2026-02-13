use yew::prelude::*;

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

#[function_component(CustomDialog)]
pub fn custom_dialog(props: &CustomDialogProps) -> Html {
    let selected_option = use_state(|| 0usize);
    let is_fading_out = use_state(|| false);

    let on_confirm = {
        let on_confirm = props.on_confirm.clone();
        let on_start = props.on_start_processing.clone();
        let selected = selected_option.clone();
        let is_fading_out = is_fading_out.clone();
        Callback::from(move |_| {
            is_fading_out.set(true);
            on_start.emit(());
            let on_confirm = on_confirm.clone();
            let selected_val = *selected;
            gloo::timers::callback::Timeout::new(200, move || {
                on_confirm.emit(selected_val);
            }).forget();
        })
    };

    html! {
        <div class={classes!(
            "fixed", "inset-0", "z-[100]", "flex", "items-center", "justify-center", "bg-black/60", "backdrop-blur-sm", "p-4",
            if *is_fading_out { "opacity-0 transition-opacity duration-200" } else { "" }
        )}>
            <div class={classes!(
                "bg-gray-800", "border", "border-gray-700", "rounded-lg", "shadow-2xl", "w-full", "max-w-md", "overflow-hidden",
                if *is_fading_out { "animate-dialog-out" } else { "animate-dialog-in" }
            )}>
                <div class="px-6 py-4 border-b border-gray-700 bg-gray-800/50">
                    <h3 class="text-xl font-bold text-white">{ &props.title }</h3>
                </div>
                
                <div class="px-6 py-6">
                    <p class="text-gray-300 mb-6 whitespace-pre-wrap">{ &props.message }</p>
                    
                    <div class="space-y-3">
                        { for props.options.iter().map(|opt| {
                            let opt_id = opt.id;
                            let is_selected = *selected_option == opt_id;
                            let selected_option = selected_option.clone();
                            html! {
                                <label class={classes!(
                                    "flex", "items-center", "p-3", "rounded-md", "border", "cursor-pointer", "transition-colors",
                                    if is_selected { vec!["bg-blue-600/20", "border-blue-500", "text-white"] } else { vec!["bg-gray-700/30", "border-gray-600", "text-gray-400", "hover:bg-gray-700/50"] }
                                )}>
                                    <input 
                                        type="radio" 
                                        class="w-4 h-4 text-blue-600 bg-gray-700 border-gray-600 focus:ring-blue-500 focus:ring-offset-gray-800"
                                        name="dialog-option"
                                        checked={is_selected}
                                        onclick={Callback::from(move |_| selected_option.set(opt_id))}
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
                            onclick={cancel_cb.reform(|_| ())}
                            class="px-6 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded-md transition-colors"
                        >
                            { "Cancel" }
                        </button>
                    }
                    <button 
                        onclick={on_confirm}
                        class="px-6 py-2 bg-blue-600 hover:bg-blue-700 text-white font-bold rounded-md transition-colors shadow-lg shadow-blue-900/20"
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

#[function_component(InputDialog)]
pub fn input_dialog(props: &InputDialogProps) -> Html {
    let text = use_state(|| "".to_string());
    let input_ref = use_node_ref();

    {
        let input_ref = input_ref.clone();
        use_effect_with((), move |_| {
            if let Some(input) = input_ref.cast::<web_sys::HtmlInputElement>() {
                let _ = input.focus();
            }
            || ()
        });
    }

    let on_input = {
        let text = text.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            text.set(input.value());
        })
    };

    let on_confirm = {
        let on_confirm = props.on_confirm.clone();
        let text = text.clone();
        Callback::from(move |_| {
            on_confirm.emit((*text).clone());
        })
    };

    let on_keydown = {
        let on_confirm = props.on_confirm.clone();
        let on_cancel = props.on_cancel.clone();
        let text = text.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Enter" {
                e.prevent_default();
                on_confirm.emit((*text).clone());
            } else if e.key() == "Escape" {
                e.prevent_default();
                on_cancel.emit(());
            }
        })
    };

    html! {
        <div class="fixed inset-0 z-[200] flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
            <div class="bg-gray-800 border border-gray-700 rounded-lg shadow-2xl w-full max-w-sm overflow-hidden animate-dialog-in">
                <div class="px-6 py-4 border-b border-gray-700 bg-gray-800/50">
                    <h3 class="text-lg font-bold text-white">{ &props.title }</h3>
                </div>
                
                <div class="px-6 py-6 space-y-4">
                    <p class="text-sm text-gray-300">{ &props.message }</p>
                    <input 
                        ref={input_ref}
                        type="text" 
                        value={(*text).clone()}
                        oninput={on_input}
                        onkeydown={on_keydown}
                        class="w-full bg-gray-900 border border-gray-700 rounded-md px-4 py-2 text-white focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    />
                </div>

                <div class="px-6 py-2 bg-gray-900/50 flex justify-end space-x-3">
                    <button 
                        onclick={props.on_cancel.reform(|_| ())}
                        class="px-6 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded-md transition-colors"
                    >
                        { "Cancel" }
                    </button>
                    <button 
                        onclick={on_confirm}
                        class="px-6 py-2 bg-blue-600 hover:bg-blue-700 text-white font-bold rounded-md transition-colors shadow-lg"
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
}

#[function_component(ConfirmDialog)]
pub fn confirm_dialog(props: &ConfirmDialogProps) -> Html {
    html! {
        <div class="fixed inset-0 z-[200] flex items-center justify-center bg-black/60 backdrop-blur-sm p-4 animate-in fade-in duration-200">
            <div class="bg-gray-800 border border-gray-700 rounded-lg shadow-2xl w-full max-w-sm overflow-hidden animate-dialog-in">
                <div class="px-6 py-4 border-b border-gray-700 bg-gray-800/50">
                    <h3 class="text-lg font-bold text-white">{ &props.title }</h3>
                </div>
                
                <div class="px-6 py-8">
                    <p class="text-sm text-gray-300 whitespace-pre-wrap">{ &props.message }</p>
                </div>

                <div class="px-6 py-3 bg-gray-900/50 flex justify-end space-x-3">
                    <button 
                        onclick={props.on_cancel.reform(|_| ())}
                        class="px-6 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded-md transition-colors"
                    >
                        { "Cancel" }
                    </button>
                    <button 
                        onclick={props.on_confirm.reform(|_| ())}
                        class="px-8 py-2 bg-red-600 hover:bg-red-700 text-white font-bold rounded-md transition-colors shadow-lg"
                    >
                        { "OK" }
                    </button>
                </div>
            </div>
        </div>
    }
}
