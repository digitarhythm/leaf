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

                <div class="px-6 py-2 bg-gray-900/50 flex justify-end">
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
