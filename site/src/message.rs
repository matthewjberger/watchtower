use leptos::prelude::*;

use crate::state::{ChatMessage, MessageRole};
use crate::tool_use::ToolUseDisplay;

#[component]
pub fn MessageBubble(message: ChatMessage) -> impl IntoView {
    let is_user = matches!(message.role, MessageRole::User);
    let content = message.content.clone();
    let tool_uses = message.tool_uses.clone();

    let container_class = if is_user {
        "flex justify-end mb-3"
    } else {
        "flex justify-start mb-3"
    };

    let bubble_class = if is_user {
        "max-w-[80%] px-4 py-2.5 rounded-lg bg-[#1f6feb] text-white"
    } else {
        "max-w-[80%] px-4 py-2.5 rounded-lg bg-[#161b22] text-[#c9d1d9] border border-[#30363d]"
    };

    view! {
        <div class={container_class}>
            <div class={bubble_class}>
                <pre class="whitespace-pre-wrap break-words font-mono text-sm leading-relaxed m-0">{content}</pre>
                {if !tool_uses.is_empty() {
                    Some(view! {
                        <div class="mt-2">
                            {tool_uses.into_iter().map(|tool| {
                                view! { <ToolUseDisplay tool=tool /> }
                            }).collect_view()}
                        </div>
                    })
                } else {
                    None
                }}
            </div>
        </div>
    }
}
