<h1 align="center">summoner</h1>

<p align="center">
  <a href="https://github.com/matthewjberger/summoner"><img alt="github" src="https://img.shields.io/badge/github-matthewjberger/summoner-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20"></a>
  <a href="https://github.com/matthewjberger/summoner/blob/main/LICENSE-MIT"><img alt="license" src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=for-the-badge&labelColor=555555" height="20"></a>
</p>

<p align="center"><strong>AI game creation platform powered by Claude Code and the Nightshade engine.</strong></p>

<p align="center">
  <code>just run</code>
</p>

Summoner is a desktop app that lets you describe a game to Claude and watch it come to life. Claude generates complete, playable 3D games from natural language, and you can edit them live by talking to Claude.

Built on the [Nightshade](https://github.com/matthewjberger/nightshade) engine with Rhai scripting, Summoner gives Claude full control over game creation through an MCP server with tools for spawning entities, attaching scripts, managing game state, and more.

## Architecture

```
┌───────────────────────────────────────────────────────────┐
│                   Summoner (Native Rust)                   │
│                                                           │
│  ┌──────────────┐  ┌──────────────────────────────────┐   │
│  │ CLI Worker    │  │ MCP Server (:3334/mcp)           │   │
│  │ claude -p     │  │                                  │   │
│  │ --stream-json │  │ Game Creation + Editing Tools     │   │
│  └──────┬───────┘  └──────────┬───────────────────────┘   │
│         │                     │                           │
│         └─────┬───────────────┘                           │
│               ↓                                           │
│      ui() + run_systems() → WebView IPC + Scripting       │
│                                                           │
│  ┌────────────────────────────────────────────────────┐   │
│  │           WebView (Leptos WASM)                     │   │
│  │  Toolbar · Chat · Streaming · Tool Use              │   │
│  └────────────────────────────────────────────────────┘   │
│                                                           │
│  ┌────────────────────────────────────────────────────┐   │
│  │           Nightshade Engine                         │   │
│  │  3D Rendering · Rhai Scripting · ECS · Physics      │   │
│  └────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────┘
```

## How It Works

1. You describe a game to Claude ("make a breakout game", "create a 3D platformer")
2. Claude uses the `create_game` MCP tool to send a complete game definition (entities, scripts, camera, lighting)
3. Summoner builds the scene and spawns it in a Nightshade engine window with live Rhai scripting
4. You ask Claude to make changes ("make the paddle faster", "add a score display", "change the background")
5. Claude uses editing tools to modify the running game in real-time

## MCP Tools

| Tool | Description |
|------|-------------|
| `create_game` | Create a complete game from a JSON definition with entities, scripts, camera, lighting, and game state |
| `update_entity_script` | Replace the Rhai script on a named entity in the running game |
| `add_game_entity` | Add a new entity with mesh, material, position, and optional script |
| `remove_game_entity` | Remove a named entity from the running game |
| `set_game_state` | Set a game state variable (f64) accessible from all scripts |
| `get_game_state` | Read all game state variables |
| `get_scene_info` | Get full scene info: entity names, positions, scripts, game state |
| `reset_game` | Re-spawn the game from its original definition |
| `undo` | Undo the last operation |
| `redo` | Redo the last undone operation |
| `get_history` | View the full operation history tree |
| `export_scene` | Export the game as a Nightshade scene file for the editor |

## Operation History

Every game modification is tracked in an undo/redo tree. Operations include creating games, adding/removing entities, updating scripts, and changing game state. The tree structure preserves branching history so you can explore different directions without losing previous work.

## Scripting

Games use [Rhai](https://rhai.rs/) scripts attached to entities. Scripts have access to:

| Variable | Description |
|----------|-------------|
| `pos_x`, `pos_y`, `pos_z` | Entity position (read/write) |
| `rot_x`, `rot_y`, `rot_z` | Entity rotation in radians (read/write) |
| `scale_x`, `scale_y`, `scale_z` | Entity scale (read/write) |
| `dt` | Delta time in seconds |
| `time` | Total elapsed time |
| `pressed_keys` | Currently held keys (array of strings) |
| `just_pressed_keys` | Keys pressed this frame |
| `mouse_x`, `mouse_y` | Mouse position |
| `entities` | Map of entity name to position `[x, y, z]` |
| `entity_names` | Array of all entity names |
| `state` | Game state map (string keys, f64 values) |
| `despawn_names` | Push entity names here to despawn them |
| `do_spawn_cube`, `do_spawn_sphere` | Push `[x, y, z]` to spawn primitives |

## Export

Games can be exported as Nightshade scene files (`.json`), preserving all entities, transforms, materials, scripts, and scene settings. Exported scenes can be opened directly in the Nightshade editor for further development.

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
claude mcp add --transport http summoner http://127.0.0.1:3334/mcp
```

## Project Structure

```
summoner/
├── src/
│   ├── main.rs          # App state, run_systems(), ui() loop, MCP command handling
│   ├── cli.rs           # CLI worker thread (spawns claude, parses NDJSON)
│   ├── mcp_server.rs    # MCP server with game creation and editing tools
│   ├── game.rs          # Game definition parsing and scene construction
│   ├── history.rs       # Tree-based operation undo/redo system
│   └── scene.rs         # Scene state management
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
