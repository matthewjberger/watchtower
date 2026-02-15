use leptos::prelude::*;
use web_sys::wasm_bindgen::JsCast;

use crate::message::MessageBubble;
use crate::state::{AppState, ChatMessage, MessageRole, StatusDisplay};
use crate::tool_use::ToolUseDisplay;
use watchtower_protocol::FrontendCommand;

#[component]
pub fn ChatView(state: AppState) -> impl IntoView {
    let (input_text, set_input_text) = signal(String::new());
    let messages = state.messages;
    let streaming_text = state.streaming_text;
    let thinking_text = state.thinking_text;
    let active_tools = state.active_tools;
    let status = state.status;
    let pending_input = state.pending_input_request;

    let is_busy = move || {
        !matches!(
            status.get(),
            StatusDisplay::Idle | StatusDisplay::Disconnected
        )
    };

    let can_send = move || {
        !input_text.get().trim().is_empty() && !is_busy()
    };

    let send_prompt = move || {
        let text = input_text.get_untracked();
        if text.trim().is_empty() {
            return;
        }

        state.messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: MessageRole::User,
                content: text.clone(),
                thinking: String::new(),
                thinking_duration_ms: 0,
                tool_uses: Vec::new(),
            });
        });

        nightshade::webview::send(&FrontendCommand::SendPrompt {
            prompt: text,
            session_id: state.current_session_id.get_untracked(),
            model: None,
        });

        set_input_text.set(String::new());
    };

    let cancel = move |_| {
        nightshade::webview::send(&FrontendCommand::CancelRequest);
    };

    let on_keydown = move |event: web_sys::KeyboardEvent| {
        if event.key() == "Enter" && event.ctrl_key() && can_send() {
            event.prevent_default();
            send_prompt();
        }
    };

    let on_send_click = move |_| {
        if can_send() {
            send_prompt();
        }
    };

    view! {
        <div class="flex flex-col h-full">
            <div class="flex-1 overflow-y-auto px-4 py-4" id="chat-scroll-container">
                {move || {
                    let msgs = messages.get();
                    let is_thinking = matches!(status.get(), StatusDisplay::Thinking);
                    if msgs.is_empty() && streaming_text.get().is_empty() && thinking_text.get().is_empty() && !is_thinking {
                        view! {
                            <div class="flex items-center justify-center h-full text-[#484f58] text-sm">
                                "Send a prompt to get started"
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div>
                                {msgs.into_iter().map(|message| {
                                    view! { <MessageBubble message=message /> }
                                }).collect_view()}

                                {move || {
                                    let thinking = thinking_text.get();
                                    let text = streaming_text.get();
                                    let tools = active_tools.get();
                                    let current_status = status.get();
                                    let is_thinking = matches!(current_status, StatusDisplay::Thinking);
                                    let is_active = !text.is_empty() || !tools.is_empty() || !thinking.is_empty() || is_thinking;

                                    if is_active {
                                        Some(view! {
                                            <div class="flex justify-start mb-3">
                                                <div class="max-w-[80%] px-4 py-2.5 rounded-lg bg-[#161b22] text-[#c9d1d9] border border-[#30363d]">
                                                    {if !thinking.is_empty() {
                                                        view! {
                                                            <div class="mb-3 pb-3 border-b border-[#30363d]">
                                                                <div class="flex items-center gap-1.5 mb-1">
                                                                    <span class="text-yellow-500 text-xs">"Thinking"</span>
                                                                </div>
                                                                <pre class="whitespace-pre-wrap break-words font-mono text-xs leading-relaxed m-0 text-[#8b949e]">{thinking}</pre>
                                                            </div>
                                                        }.into_any()
                                                    } else if is_thinking && text.is_empty() {
                                                        view! {
                                                            <div class="mb-3 pb-3 border-b border-[#30363d]">
                                                                <div class="flex items-center gap-1.5">
                                                                    <span class="text-yellow-500 text-xs animate-pulse">"Thinking..."</span>
                                                                </div>
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! { <div></div> }.into_any()
                                                    }}
                                                    {if !text.is_empty() {
                                                        Some(view! {
                                                            <pre class="whitespace-pre-wrap break-words font-mono text-sm leading-relaxed m-0">{text}</pre>
                                                        })
                                                    } else {
                                                        None
                                                    }}
                                                    {if !tools.is_empty() {
                                                        Some(view! {
                                                            <div class="mt-2">
                                                                {tools.into_iter().map(|tool| {
                                                                    view! { <ToolUseDisplay tool=tool /> }
                                                                }).collect_view()}
                                                            </div>
                                                        })
                                                    } else {
                                                        None
                                                    }}
                                                    <span class="inline-block w-2 h-4 bg-[#c9d1d9] animate-pulse ml-0.5"></span>
                                                </div>
                                            </div>
                                        })
                                    } else {
                                        None
                                    }
                                }}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            {move || {
                let request = pending_input.get();
                request.map(|req| {
                    let request_id = req.request_id.clone();
                    let options = req.options.clone();
                    view! {
                        <div class="mx-4 mb-2 p-3 bg-[#1c2129] border border-[#30363d] rounded-lg">
                            <p class="text-sm text-[#c9d1d9] mb-2">{req.prompt.clone()}</p>
                            <div class="flex flex-wrap gap-2">
                                {options.into_iter().map(|option| {
                                    let opt_clone = option.clone();
                                    let rid = request_id.clone();
                                    let pending_ref = state.pending_input_request;
                                    view! {
                                        <button
                                            class="px-3 py-1 text-xs bg-[#21262d] text-[#c9d1d9] border border-[#30363d] rounded hover:bg-[#30363d] cursor-pointer"
                                            on:click=move |_| {
                                                nightshade::webview::send(&FrontendCommand::UserInputResponse {
                                                    request_id: rid.clone(),
                                                    response: opt_clone.clone(),
                                                });
                                                pending_ref.set(None);
                                            }
                                        >
                                            {option}
                                        </button>
                                    }
                                }).collect_view()}
                            </div>
                        </div>
                    }
                })
            }}

            <div class="px-4 py-3 bg-[#161b22] border-t border-[#30363d]">
                <div class="flex gap-2">
                    <textarea
                        class="flex-1 bg-[#0d1117] text-[#c9d1d9] border border-[#30363d] rounded-lg px-3 py-2 text-sm font-mono resize-none focus:outline-none focus:border-[#58a6ff] placeholder-[#484f58]"
                        placeholder="Type a prompt... (Ctrl+Enter to send)"
                        rows="3"
                        prop:value=move || input_text.get()
                        on:input=move |event| {
                            let target = event.target().unwrap();
                            let textarea: web_sys::HtmlTextAreaElement = target.unchecked_into();
                            set_input_text.set(textarea.value());
                        }
                        on:keydown=on_keydown
                    />
                    <div class="flex flex-col gap-1">
                        <button
                            class="px-4 py-2 bg-[#238636] text-white text-sm rounded-lg hover:bg-[#2ea043] disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
                            on:click=on_send_click
                            disabled=move || !can_send()
                        >
                            "Send"
                        </button>
                        <button
                            class="px-4 py-2 bg-[#da3633] text-white text-sm rounded-lg hover:bg-[#f85149] disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
                            on:click=cancel
                            disabled=move || !is_busy()
                        >
                            "Cancel"
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}
