use leptos::prelude::*;
use watchtower_protocol::FrontendCommand;

use crate::state::{AppState, TestStatus};

struct TestDefinition {
    name: &'static str,
    label: &'static str,
    description: &'static str,
}

const TESTS: &[TestDefinition] = &[
    TestDefinition {
        name: "ipc_echo",
        label: "IPC Echo",
        description: "Sends RunTest to the backend and waits for TestResult. Proves IPC round-trip works and shows latency.",
    },
    TestDefinition {
        name: "mcp_round_trip",
        label: "MCP Round-Trip",
        description: "Backend HTTP-calls its own MCP server's show_notification tool at 127.0.0.1:3334/mcp. Proves MCP server is reachable.",
    },
    TestDefinition {
        name: "show_notification",
        label: "Show Notification",
        description: "Sends a test notification. A toast should appear in the UI.",
    },
    TestDefinition {
        name: "display_content",
        label: "Display Content",
        description: "Backend sends markdown content via ContentDisplay. Verifies the content rendering pipeline.",
    },
    TestDefinition {
        name: "status_cycle",
        label: "Status Cycle",
        description: "Rapidly cycles through all AgentStatus values with 500ms delays. The toolbar dot should change color.",
    },
    TestDefinition {
        name: "cli_prompt",
        label: "CLI Prompt",
        description: "Spawns claude CLI with a test prompt and streams the result. Tests the full CLI pipeline.",
    },
];

#[component]
pub fn TestTab(state: AppState) -> impl IntoView {
    let test_results = state.test_results;

    let run_test = move |test_name: &'static str| {
        state.test_results.update(|results| {
            if let Some(entry) = results.iter_mut().find(|entry| entry.test_name == test_name) {
                entry.status = TestStatus::Running;
                entry.message = String::new();
                entry.duration_ms = 0;
            } else {
                results.push(crate::state::TestEntry {
                    test_name: test_name.to_string(),
                    status: TestStatus::Running,
                    message: String::new(),
                    duration_ms: 0,
                });
            }
        });
        nightshade::webview::send(&FrontendCommand::RunTest {
            test_name: test_name.to_string(),
        });
    };

    let run_all = move |_| {
        for test in TESTS {
            run_test(test.name);
        }
    };

    view! {
        <div class="flex flex-col h-full">
            <div class="px-4 py-3 border-b border-[#30363d] flex items-center justify-between">
                <div>
                    <h2 class="text-sm font-bold text-[#c9d1d9]">"System Tests"</h2>
                    <p class="text-xs text-[#484f58] mt-0.5">"Verify that all subsystems are working correctly"</p>
                </div>
                <button
                    class="px-4 py-1.5 text-xs font-medium bg-[#238636] text-white rounded-md hover:bg-[#2ea043] cursor-pointer"
                    on:click=run_all
                >
                    "Run All"
                </button>
            </div>
            <div class="flex-1 overflow-y-auto px-4 py-4 space-y-3">
                {TESTS.iter().map(|test| {
                    let test_name = test.name;
                    let label = test.label;
                    let description = test.description;
                    let run = move |_| run_test(test_name);

                    view! {
                        <TestCard
                            test_name=test_name
                            label=label
                            description=description
                            test_results=test_results
                            on_run=run
                        />
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

#[component]
fn TestCard(
    test_name: &'static str,
    label: &'static str,
    description: &'static str,
    test_results: RwSignal<Vec<crate::state::TestEntry>>,
    on_run: impl Fn(web_sys::MouseEvent) + 'static,
) -> impl IntoView {
    let test_name_owned = test_name.to_string();

    view! {
        <div class="bg-[#161b22] border border-[#30363d] rounded-lg p-4">
            <div class="flex items-start justify-between gap-3">
                <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2">
                        {move || {
                            let results = test_results.get();
                            let entry = results.iter().find(|entry| entry.test_name == test_name_owned);
                            match entry.map(|entry| &entry.status) {
                                Some(TestStatus::Running) => view! {
                                    <span class="text-yellow-500 animate-pulse text-sm">"⟳"</span>
                                }.into_any(),
                                Some(TestStatus::Passed) => view! {
                                    <span class="text-green-500 text-sm">"✓"</span>
                                }.into_any(),
                                Some(TestStatus::Failed) => view! {
                                    <span class="text-red-500 text-sm">"✗"</span>
                                }.into_any(),
                                _ => view! {
                                    <span class="text-[#484f58] text-sm">"○"</span>
                                }.into_any(),
                            }
                        }}
                        <h3 class="text-sm font-bold text-[#c9d1d9]">{label}</h3>
                    </div>
                    <p class="text-xs text-[#484f58] mt-1">{description}</p>
                    {move || {
                        let results = test_results.get();
                        let entry = results.iter().find(|entry| entry.test_name == test_name);
                        entry.and_then(|entry| {
                            if entry.message.is_empty() && entry.status == TestStatus::Running {
                                return None;
                            }
                            if entry.status == TestStatus::Pending {
                                return None;
                            }
                            let status_class = match entry.status {
                                TestStatus::Passed => "text-green-400",
                                TestStatus::Failed => "text-red-400",
                                TestStatus::Running => "text-yellow-400",
                                TestStatus::Pending => "text-[#484f58]",
                            };
                            let duration_text = if entry.duration_ms > 0 {
                                format!(" ({}ms)", entry.duration_ms)
                            } else {
                                String::new()
                            };
                            Some(view! {
                                <div class=format!("mt-2 px-3 py-2 bg-[#0d1117] rounded text-xs font-mono {status_class}")>
                                    {entry.message.clone()}
                                    <span class="text-[#484f58]">{duration_text}</span>
                                </div>
                            })
                        })
                    }}
                </div>
                <button
                    class="px-3 py-1 text-xs bg-[#21262d] text-[#c9d1d9] border border-[#30363d] rounded hover:bg-[#30363d] cursor-pointer shrink-0"
                    on:click=on_run
                >
                    "Run"
                </button>
            </div>
        </div>
    }
}
