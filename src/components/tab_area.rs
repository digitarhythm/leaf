use yew::prelude::*;

#[derive(Clone, PartialEq)]
pub struct Sheet {
    pub id: String,
    pub guid: Option<String>,
    pub category: String,
    pub title: String,
    pub content: String,
    pub is_modified: bool,
    pub drive_id: Option<String>,
    pub temp_content: Option<String>,
    pub temp_timestamp: Option<u64>,
    pub last_sync_timestamp: Option<u64>,
}

#[derive(Properties, PartialEq)]
pub struct TabAreaProps {
    pub sheets: Vec<Sheet>,
    pub active_sheet_id: Option<String>,
    pub on_select_sheet: Callback<String>,
    pub on_close_sheet: Callback<String>,
    pub on_new_sheet: Callback<()>,
}

#[function_component(TabArea)]
pub fn tab_area(props: &TabAreaProps) -> Html {
    let on_double_click = {
        let on_new = props.on_new_sheet.clone();
        Callback::from(move |_| {
            on_new.emit(());
        })
    };

    html! {
        <div class="flex overflow-x-auto bg-gray-900 border-b border-gray-700 min-h-[40px]" ondblclick={on_double_click}>
            { for props.sheets.clone().into_iter().map(|sheet| {
                let is_active = props.active_sheet_id.as_ref() == Some(&sheet.id);
                let id = sheet.id.clone();
                let on_select = props.on_select_sheet.clone();
                let on_close = props.on_close_sheet.clone();

                html! {
                    <div
                        class={classes!(
                            "flex", "items-center", "group", "px-3", "py-2", "cursor-pointer", "border-r", "border-gray-700", "min-w-[120px]", "max-w-[200px]", "select-none",
                            if is_active { vec!["bg-gray-800", "text-white"] } else { vec!["bg-gray-900", "text-gray-400", "hover:bg-gray-800"] }
                        )}
                        onclick={move |_| on_select.emit(id.clone())}
                    >
                        <span class="truncate flex-1 text-sm">{ &sheet.title }{ if sheet.is_modified { " *" } else { "" } }</span>
                        <button
                            class="ml-2 p-0.5 rounded-full opacity-0 group-hover:opacity-100 hover:bg-gray-600 text-gray-400 hover:text-white transition-all"
                            onclick={move |e: MouseEvent| {
                                e.stop_propagation();
                                on_close.emit(sheet.id.clone());
                            }}
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3 h-3">
                                <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
                            </svg>
                        </button>
                    </div>
                }
            }) }
        </div>
    }
}
