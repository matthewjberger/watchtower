use leptos::prelude::*;
use summoner_protocol::{FrontendCommand, PlayState};

use crate::state::{ActiveTab, AppState};

#[component]
pub fn Toolbar(state: AppState) -> impl IntoView {
    let status = state.status;
    let session_id = state.current_session_id;
    let active_tab = state.active_tab;
    let has_game = state.has_game;
    let play_state = state.play_state;
    let editor_window_open = state.editor_window_open;

    view! {
        <div class="flex items-center justify-between px-4 py-2 bg-[#161b22] border-b border-[#30363d]">
            <div class="flex items-center gap-4">
                <span class="text-sm font-bold text-[#c9d1d9] tracking-wide">"SUMMONER"</span>
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
                {move || {
                    if has_game.get() {
                        Some(view! {
                            <div class="flex items-center gap-1 border-r border-[#30363d] pr-3 mr-1">
                                {move || {
                                    let state = play_state.get();
                                    if state == PlayState::Stopped || state == PlayState::Paused {
                                        Some(view! {
                                            <button
                                                class="w-7 h-7 flex items-center justify-center text-[#3fb950] hover:bg-[#1c2129] rounded cursor-pointer bg-transparent border-none"
                                                title="Play"
                                                on:click=move |_| {
                                                    nightshade::webview::send(&FrontendCommand::PlayGame);
                                                }
                                            >
                                                <span class="text-sm">"\u{25B6}"</span>
                                            </button>
                                        })
                                    } else {
                                        None
                                    }
                                }}
                                {move || {
                                    if play_state.get() == PlayState::Playing {
                                        Some(view! {
                                            <button
                                                class="w-7 h-7 flex items-center justify-center text-[#d29922] hover:bg-[#1c2129] rounded cursor-pointer bg-transparent border-none"
                                                title="Pause"
                                                on:click=move |_| {
                                                    nightshade::webview::send(&FrontendCommand::PauseGame);
                                                }
                                            >
                                                <span class="text-sm">"\u{23F8}"</span>
                                            </button>
                                        })
                                    } else {
                                        None
                                    }
                                }}
                                {move || {
                                    let state = play_state.get();
                                    if state == PlayState::Playing || state == PlayState::Paused {
                                        Some(view! {
                                            <button
                                                class="w-7 h-7 flex items-center justify-center text-[#f85149] hover:bg-[#1c2129] rounded cursor-pointer bg-transparent border-none"
                                                title="Stop"
                                                on:click=move |_| {
                                                    nightshade::webview::send(&FrontendCommand::StopGame);
                                                }
                                            >
                                                <span class="text-sm">"\u{25A0}"</span>
                                            </button>
                                        })
                                    } else {
                                        None
                                    }
                                }}
                                {move || {
                                    if has_game.get() && !editor_window_open.get() {
                                        Some(view! {
                                            <button
                                                class="w-7 h-7 flex items-center justify-center text-[#8b949e] hover:text-[#c9d1d9] hover:bg-[#1c2129] rounded cursor-pointer bg-transparent border-none"
                                                title="Open Editor Window"
                                                on:click=move |_| {
                                                    nightshade::webview::send(&FrontendCommand::OpenEditorWindow);
                                                }
                                            >
                                                <span class="text-sm">"\u{25A1}"</span>
                                            </button>
                                        })
                                    } else {
                                        None
                                    }
                                }}
                            </div>
                        })
                    } else {
                        None
                    }
                }}
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
