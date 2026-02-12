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
}

#[function_component(CustomDialog)]
pub fn custom_dialog(props: &CustomDialogProps) -> Html {
    let selected_option = use_state(|| 0usize);

    let on_confirm = {
        let on_confirm = props.on_confirm.clone();
        let selected = selected_option.clone();
        Callback::from(move |_| {
            on_confirm.emit(*selected);
        })
    };

    html! {
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
            <div class="bg-gray-800 border border-gray-700 rounded-lg shadow-2xl w-full max-w-md overflow-hidden animate-in fade-in zoom-in duration-200">
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

                <div class="px-6 py-4 bg-gray-900/50 flex justify-end">
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
