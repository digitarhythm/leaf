use yew::prelude::*;
use crate::i18n::{self, Language};

#[derive(Properties, PartialEq)]
pub struct StatusBarProps {
    pub network_status: bool, // true = connected, false = disconnected
    pub version: String,
}

#[function_component(StatusBar)]
pub fn status_bar(props: &StatusBarProps) -> Html {
    let lang = Language::detect();

    html! {
        <div class="flex items-center justify-between px-4 py-1 bg-gray-800 border-t border-gray-700 text-xs text-gray-400 select-none">
            <span class="font-mono">{ format!("ver{}", props.version) }</span>
            
            <span class={classes!(
                "flex", "items-center", "space-x-2", "font-semibold",
                if props.network_status { "text-green-500" } else { "text-red-500" }
            )}>
                <span class={classes!(
                    "w-2", "h-2", "rounded-full",
                    if props.network_status { "bg-green-500" } else { "bg-red-500" }
                )}></span>
                <span>
                    { if props.network_status { 
                        i18n::t("network_connected", lang)
                    } else { 
                        i18n::t("offline", lang)
                    } }
                </span>
            </span>
        </div>
    }
}
