use leptos::prelude::*;

use crate::state::{ChatMessage, MessageRole};
use crate::tool_use::ToolUseDisplay;

#[component]
pub fn MessageBubble(message: ChatMessage) -> impl IntoView {
    let is_user = matches!(message.role, MessageRole::User);
    let content = message.content.clone();
    let thinking = message.thinking.clone();
    let thinking_duration_ms = message.thinking_duration_ms;
    let has_thinking = !thinking.is_empty() || thinking_duration_ms > 0;
    let tool_uses = message.tool_uses.clone();
    let (thinking_expanded, set_thinking_expanded) = signal(false);

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

    let duration_label = if thinking_duration_ms > 0 {
        let seconds = thinking_duration_ms as f64 / 1000.0;
        format!("Thinking ({seconds:.1}s)")
    } else {
        "Thinking".to_string()
    };

    view! {
        <div class={container_class}>
            <div class={bubble_class}>
                {if has_thinking && !is_user {
                    let thinking_clone = thinking.clone();
                    let has_thinking_text = !thinking.is_empty();
                    Some(view! {
                        <div class="mb-2">
                            <button
                                class="flex items-center gap-1.5 text-xs text-yellow-500 hover:text-yellow-400 cursor-pointer bg-transparent"
                                on:click=move |_| set_thinking_expanded.update(|value| *value = !*value)
                            >
                                <span class=move || if thinking_expanded.get() { "transform rotate-90 transition-transform" } else { "transition-transform" }>
                                    "▶"
                                </span>
                                {duration_label.clone()}
                            </button>
                            {move || {
                                if thinking_expanded.get() {
                                    if has_thinking_text {
                                        view! {
                                            <pre class="whitespace-pre-wrap break-words font-mono text-xs leading-relaxed mt-1 text-[#8b949e] pl-3 border-l-2 border-[#30363d]">
                                                {thinking_clone.clone()}
                                            </pre>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <p class="text-xs text-[#484f58] mt-1 pl-3 border-l-2 border-[#30363d] italic">
                                                "Thinking content not available from CLI"
                                            </p>
                                        }.into_any()
                                    }
                                } else {
                                    view! { <div></div> }.into_any()
                                }
                            }}
                        </div>
                    })
                } else {
                    None
                }}
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
