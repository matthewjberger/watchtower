use leptos::prelude::*;

use crate::state::ToolUseBlock;

#[component]
pub fn ToolUseDisplay(tool: ToolUseBlock) -> impl IntoView {
    let (expanded, set_expanded) = signal(false);
    let tool_name = tool.tool_name.clone();
    let input_json = tool.input_json.clone();
    let finished = tool.finished;

    view! {
        <div class="my-2 border border-[#30363d] rounded-md overflow-hidden">
            <button
                class="w-full flex items-center gap-2 px-3 py-1.5 bg-[#161b22] text-xs text-[#c9d1d9] hover:bg-[#1c2129] cursor-pointer"
                on:click=move |_| set_expanded.update(|value| *value = !*value)
            >
                <span class={move || if expanded.get() { "transform rotate-90 transition-transform" } else { "transition-transform" }}>
                    "▶"
                </span>
                <span class="text-purple-400 font-medium">{tool_name.clone()}</span>
                {if finished {
                    view! { <span class="text-green-500 ml-auto">"✓"</span> }.into_any()
                } else {
                    view! { <span class="text-yellow-500 ml-auto animate-pulse">"⟳"</span> }.into_any()
                }}
            </button>
            {move || {
                if expanded.get() && !input_json.is_empty() {
                    Some(view! {
                        <pre class="px-3 py-2 text-xs text-[#8b949e] bg-[#0d1117] overflow-x-auto whitespace-pre-wrap break-all">
                            {input_json.clone()}
                        </pre>
                    })
                } else {
                    None
                }
            }}
        </div>
    }
}
