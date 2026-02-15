# Watchtower

A fast, responsive desktop frontend for Claude Code.

## Architecture

```
┌───────────────────────────────────────────────────────┐
│                 Watchtower (Native Rust)               │
│                                                       │
│  ┌──────────────┐  ┌──────────────────────────────┐   │
│  │ CLI Worker    │  │ MCP Server (:3334/mcp)       │   │
│  │ claude -p     │  │ show_notification             │   │
│  │ --stream-json │  │ display_content               │   │
│  │              │  │ request_user_input             │   │
│  └──────┬───────┘  └──────────┬───────────────────┘   │
│         │                     │                       │
│         └─────┬───────────────┘                       │
│               ↓                                       │
│         ui() drains events → WebView IPC              │
│                                                       │
│  ┌────────────────────────────────────────────────┐   │
│  │           WebView (Leptos WASM)                 │   │
│  │  Toolbar · Chat · Streaming · Tool Use          │   │
│  └────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────┘
```

### Two-way communication

1. **Watchtower → Claude Code**: User types prompts, backend spawns `claude -p --output-format stream-json`, streams responses to the Leptos WASM frontend via IPC
2. **Claude Code → Watchtower**: Backend hosts an MCP server so Claude Code can command the frontend (show notifications, request user input, etc.)

## Quick Start

### Prerequisites

- Rust 1.90+
- [Trunk](https://trunkrs.dev) (`cargo install trunk`)
- [just](https://just.systems) (`cargo install just`)
- Node.js (for Tailwind CSS)

### Build & Run

```bash
just setup  # install npm dependencies (first time)
just run    # build and launch
```

### Connect Claude Code MCP

```bash
claude mcp add --transport http watchtower http://127.0.0.1:3334/mcp
```

## Project Structure

```
watchtower/
├── src/
│   ├── main.rs          # App state, ui() loop, IPC bridging
│   ├── cli.rs           # CLI worker thread (spawns claude, parses NDJSON)
│   └── mcp_server.rs    # MCP server (tools that command the frontend)
├── protocol/
│   └── src/lib.rs       # Shared IPC message types (no_std)
├── site/
│   └── src/
│       ├── main.rs      # WASM entry point
│       ├── lib.rs       # App root, IPC handler, event routing
│       ├── state.rs     # Reactive state (signals for messages, status, tools)
│       ├── chat.rs      # Chat view (messages + streaming + input)
│       ├── message.rs   # Message bubble component
│       ├── toolbar.rs   # Top toolbar (status indicator, session info)
│       └── tool_use.rs  # Tool use display block
└── justfile
```

## License

MIT OR Apache-2.0
