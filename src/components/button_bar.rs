use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct ButtonBarProps {
    pub on_new_sheet: Callback<()>,
    pub on_save: Callback<()>,
    pub on_toggle_vim: Callback<()>,
    pub vim_mode: bool,
}

#[function_component(ButtonBar)]
pub fn button_bar(props: &ButtonBarProps) -> Html {
    html! {
        <div class="flex items-center space-x-2 bg-gray-800 p-2 border-b border-gray-700">
            <button
                onclick={props.on_new_sheet.reform(|_| ())}
                class="p-2 rounded hover:bg-gray-700 text-white"
                title="New Sheet"
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
                </svg>
            </button>
            <button
                onclick={props.on_save.reform(|_| ())}
                class="p-2 rounded hover:bg-gray-700 text-white"
                title="Save"
            >
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 6A2.25 2.25 0 016 3.75h2.25A2.25 2.25 0 0110.5 3.75v9a2.25 2.25 0 002.25 2.25h8.5a2.25 2.25 0 002.25-2.25v-9a2.25 2.25 0 012.25-2.25H21a2.25 2.25 0 012.25 2.25v15.75a2.25 2.25 0 01-2.25 2.25h-15a2.25 2.25 0 01-2.25-2.25V6z" />
                    <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
            </button>
            <button
                onclick={props.on_toggle_vim.reform(|_| ())}
                class={classes!(
                    "px-3", "py-1", "rounded", "text-sm", "font-medium", "transition-colors",
                    if props.vim_mode { vec!["bg-green-600", "text-white", "hover:bg-green-700"] } else { vec!["bg-gray-600", "text-gray-300", "hover:bg-gray-500"] }
                )}
                title="Toggle vim Mode"
            >
                { if props.vim_mode { "vim: ON" } else { "vim: OFF" } }
            </button>
            <div class="flex-1"></div>
            <span 
                class="text-green-500 opacity-30 font-bold px-4 text-2xl select-none"
                style="font-family: 'Petit Formal Script', cursive;"
            >
                {"Leaf"}
            </span>
        </div>
    }
}
