use leptos::prelude::*;
use watchtower_protocol::FrontendCommand;

use crate::state::{ActiveTab, AppState};

#[component]
pub fn Toolbar(state: AppState) -> impl IntoView {
    let status = state.status;
    let session_id = state.current_session_id;
    let working_directory = state.working_directory;
    let active_tab = state.active_tab;

    let on_browse = move |_| {
        nightshade::webview::send(&FrontendCommand::BrowseWorkingDirectory);
    };

    view! {
        <div class="flex items-center justify-between px-4 py-2 bg-[#161b22] border-b border-[#30363d]">
            <div class="flex items-center gap-4">
                <span class="text-sm font-bold text-[#c9d1d9] tracking-wide">"WATCHTOWER"</span>
                <div class="flex items-center gap-1">
                    <button
                        class=move || {
                            if active_tab.get() == ActiveTab::Chat {
                                "px-3 py-1 text-xs text-[#c9d1d9] border-b-2 border-[#58a6ff] cursor-pointer bg-transparent"
                            } else {
                                "px-3 py-1 text-xs text-[#484f58] hover:text-[#8b949e] border-b-2 border-transparent cursor-pointer bg-transparent"
                            }
                        }
                        on:click=move |_| active_tab.set(ActiveTab::Chat)
                    >
                        "Chat"
                    </button>
                    <button
                        class=move || {
                            if active_tab.get() == ActiveTab::Test {
                                "px-3 py-1 text-xs text-[#c9d1d9] border-b-2 border-[#58a6ff] cursor-pointer bg-transparent"
                            } else {
                                "px-3 py-1 text-xs text-[#484f58] hover:text-[#8b949e] border-b-2 border-transparent cursor-pointer bg-transparent"
                            }
                        }
                        on:click=move |_| active_tab.set(ActiveTab::Test)
                    >
                        "Test"
                    </button>
                </div>
                <div class="flex items-center gap-2">
                    <div class={move || format!("w-2 h-2 rounded-full {}", status.get().dot_color_class())}></div>
                    <span class="text-xs text-[#8b949e]">{move || status.get().label().to_string()}</span>
                    {move || {
                        let current_status = status.get();
                        if let crate::state::StatusDisplay::UsingTool { tool_name } = current_status {
                            format!(" ({})", tool_name)
                        } else {
                            String::new()
                        }
                    }}
                </div>
            </div>
            <div class="flex items-center gap-3">
                <div class="flex items-center gap-1">
                    <span class="text-xs text-[#484f58]">"cwd:"</span>
                    <span class="text-xs text-[#8b949e] max-w-[200px] truncate">
                        {move || {
                            let dir = working_directory.get();
                            if dir.is_empty() {
                                "(default)".to_string()
                            } else {
                                dir
                            }
                        }}
                    </span>
                    <button
                        class="text-xs text-[#58a6ff] hover:text-[#79c0ff] cursor-pointer px-1"
                        on:click=on_browse
                    >
                        "Browse..."
                    </button>
                </div>
                <div class="text-xs text-[#484f58]">
                    {move || session_id.get().map(|id| {
                        if id.len() > 12 {
                            format!("{}...", &id[..12])
                        } else {
                            id
                        }
                    }).unwrap_or_default()}
                </div>
            </div>
        </div>
    }
}
