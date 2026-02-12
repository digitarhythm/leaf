use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct StatusBarProps {
    pub network_status: bool, // true = connected, false = disconnected
}

#[function_component(StatusBar)]
pub fn status_bar(props: &StatusBarProps) -> Html {
    html! {
        <div class="flex items-center justify-end px-4 py-1 bg-gray-800 border-t border-gray-700 text-xs">
            <span class={classes!(
                "flex", "items-center", "space-x-2", "font-semibold",
                if props.network_status { "text-green-500" } else { "text-red-500" }
            )}>
                <span class={classes!(
                    "w-2", "h-2", "rounded-full",
                    if props.network_status { "bg-green-500" } else { "bg-red-500" }
                )}></span>
                <span>
                    { if props.network_status { "Network connected" } else { "Network unreachable" } }
                </span>
            </span>
        </div>
    }
}
