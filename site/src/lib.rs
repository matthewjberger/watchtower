mod chat;
mod message;
mod state;
mod test_tab;
mod toolbar;
mod tool_use;

use leptos::prelude::*;
use summoner_protocol::{BackendEvent, ContentFormat, FrontendCommand};

use crate::chat::ChatView;
use crate::state::{ActiveTab, AppState, ChatMessage, InputRequest, MessageRole, StatusDisplay, TestEntry, TestStatus, ToolUseBlock};
use crate::test_tab::TestTab;
use crate::toolbar::Toolbar;

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();

    let state_for_handler = state.clone();
    Effect::new(move |_| {
        let handler_state = state_for_handler.clone();
        nightshade::webview::connect(FrontendCommand::Ready, move |event| {
            handle_backend_event(&handler_state, event);
        });
    });

    let toolbar_state = state.clone();
    let chat_state = state.clone();
    let test_state = state.clone();
    let active_tab = state.active_tab;
    let notifications_state = state.clone();

    view! {
        <div class="h-screen flex flex-col bg-[#0d1117] text-[#c9d1d9] font-mono">
            <Toolbar state=toolbar_state />

            <div class="flex-1 overflow-hidden">
                {move || match active_tab.get() {
                    ActiveTab::Chat => view! { <ChatView state=chat_state.clone() /> }.into_any(),
                    ActiveTab::Test => view! { <TestTab state=test_state.clone() /> }.into_any(),
                }}
            </div>

            {move || {
                let notifs = notifications_state.notifications.get();
                if notifs.is_empty() {
                    None
                } else {
                    Some(view! {
                        <div class="fixed top-12 right-4 flex flex-col gap-2 z-50">
                            {notifs.into_iter().enumerate().map(|(index, (title, body))| {
                                let notif_signal = notifications_state.notifications;
                                view! {
                                    <div class="bg-[#161b22] border border-[#30363d] rounded-lg p-3 shadow-lg max-w-xs animate-fade-in">
                                        <div class="flex items-start justify-between gap-2">
                                            <div>
                                                <p class="text-xs font-bold text-[#c9d1d9]">{title}</p>
                                                <p class="text-xs text-[#8b949e] mt-1">{body}</p>
                                            </div>
                                            <button
                                                class="text-[#484f58] hover:text-[#c9d1d9] text-xs cursor-pointer"
                                                on:click=move |_| {
                                                    notif_signal.update(|notifications| {
                                                        if index < notifications.len() {
                                                            notifications.remove(index);
                                                        }
                                                    });
                                                }
                                            >
                                                "✕"
                                            </button>
                                        </div>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    })
                }
            }}
        </div>
    }
}

fn handle_backend_event(state: &AppState, event: BackendEvent) {
    match event {
        BackendEvent::Connected => {
            state.connected.set(true);
            state.status.set(StatusDisplay::Idle);
        }

        BackendEvent::StreamingStarted { session_id } => {
            state.current_session_id.set(Some(session_id));
            state.streaming_text.set(String::new());
            state.thinking_text.set(String::new());
            state.active_tools.set(Vec::new());
        }

        BackendEvent::TextDelta { text } => {
            state.streaming_text.update(|current| current.push_str(&text));
        }

        BackendEvent::ThinkingDelta { text } => {
            state.thinking_text.update(|current| current.push_str(&text));
        }

        BackendEvent::ToolUseStarted { tool_name, tool_id } => {
            state.active_tools.update(|tools| {
                tools.push(ToolUseBlock {
                    tool_name,
                    tool_id,
                    input_json: String::new(),
                    finished: false,
                });
            });
        }

        BackendEvent::ToolUseInputDelta { tool_id, partial_json } => {
            state.active_tools.update(|tools| {
                if let Some(tool) = tools.iter_mut().rev().find(|t| t.tool_id == tool_id || tool_id.is_empty()) {
                    tool.input_json.push_str(&partial_json);
                }
            });
        }

        BackendEvent::ToolUseFinished { tool_id } => {
            state.active_tools.update(|tools| {
                if let Some(tool) = tools.iter_mut().rev().find(|t| t.tool_id == tool_id || tool_id.is_empty()) {
                    tool.finished = true;
                }
            });
        }

        BackendEvent::TurnComplete { .. } => {}

        BackendEvent::RequestComplete { .. } => {
            state.finalize_streaming_message();
        }

        BackendEvent::Error { message } => {
            state.finalize_streaming_message();
            state.messages.update(|messages| {
                messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: format!("Error: {message}"),
                    thinking: String::new(),
                    thinking_duration_ms: 0,
                    tool_uses: Vec::new(),
                });
            });
        }

        BackendEvent::StatusUpdate { status } => {
            if matches!(status, summoner_protocol::AgentStatus::Thinking) && state.thinking_started_at.get_untracked().is_none() {
                state.thinking_started_at.set(Some(js_sys::Date::now()));
            }
            state.status.set(StatusDisplay::from_agent_status(&status));
        }

        BackendEvent::Notification { title, body } => {
            state.notifications.update(|notifications| {
                notifications.push((title, body));
            });
        }

        BackendEvent::ContentDisplay { content, format } => {
            let prefix = match format {
                ContentFormat::Code => "[Code]\n",
                ContentFormat::Markdown => "[Markdown]\n",
                ContentFormat::Text => "",
            };
            state.messages.update(|messages| {
                messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: format!("{prefix}{content}"),
                    thinking: String::new(),
                    thinking_duration_ms: 0,
                    tool_uses: Vec::new(),
                });
            });
        }

        BackendEvent::UserInputRequest { request_id, prompt, options } => {
            state.pending_input_request.set(Some(InputRequest {
                request_id,
                prompt,
                options,
            }));
        }

        BackendEvent::GameStateChanged { has_game, play_state, editor_window_open } => {
            state.has_game.set(has_game);
            state.play_state.set(play_state);
            state.editor_window_open.set(editor_window_open);
        }

        BackendEvent::TestResult { test_name, success, message, duration_ms } => {
            state.test_results.update(|results| {
                if let Some(entry) = results.iter_mut().find(|entry| entry.test_name == test_name) {
                    entry.status = if success { TestStatus::Passed } else { TestStatus::Failed };
                    entry.message = message;
                    entry.duration_ms = duration_ms;
                } else {
                    results.push(TestEntry {
                        test_name,
                        status: if success { TestStatus::Passed } else { TestStatus::Failed },
                        message,
                        duration_ms,
                    });
                }
            });
        }
    }
}
