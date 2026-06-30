# TermFX

<img width="1640" height="856" alt="Yeni Proje (2)" src="https://github.com/user-attachments/assets/aca509fc-b2c2-4d41-b1d8-3319462921ef" />

[English](README.md) | [Türkçe](README.tr.md)

Terminal-native video editor with FFmpeg rendering, a rich TUI, and MCP tools
for AI-assisted editing.

TermFX is designed to combine Premiere-style linear editing with
After-Effects-style layered compositing inside a terminal workflow. It also
exposes the project as an MCP server so an AI assistant can inspect media, cut
clips, apply effects, and prepare smart edit plans through JSON-RPC tools.

## Purpose

TermFX focuses on three editing workflows:

- **Sequencing:** Add media to a timeline, trim clips, cut ranges, ripple-delete
  gaps, and mix audio.
- **Effects and compositing:** Build FFmpeg complex filtergraphs for text
  overlays, fades, black-and-white, glitch, and `s_shake`-style motion effects.
- **AI control:** Let Claude, ChatGPT, or another MCP client operate the editor
  through a stdio JSON-RPC server.

The current repository is a production-oriented core implementation: project
serialization, the timeline model, FFmpeg command generation, the terminal UI,
and MCP tool handlers are already wired together.

## Features

- Rust + Tokio architecture
- FFmpeg complex filtergraph builder
- Frame-based timeline model
- Separate video and audio tracks
- Clip append, trim, and ripple delete
- Effect stack support:
  - `black_and_white`
  - `glitch`
  - `fade_in`
  - `fade_out`
  - `s_shake`
  - `text_overlay`
- Terminal UI built with Ratatui and Crossterm
- MCP stdio server:
  - `list_media`
  - `append_media`
  - `cut_video`
  - `apply_effect`
  - `smart_edit`
- JSON project file format
- Tested baseline render path

## Requirements

- macOS, Linux, or Windows
- Rust toolchain
- FFmpeg and FFprobe
- GitHub CLI (`gh`) only if you want to publish changes to GitHub

On macOS:

```bash
brew install rust ffmpeg gh
```

If Rust is already installed:

```bash
brew install ffmpeg gh
```

Verify the installation:

```bash
rustc --version
cargo --version
ffmpeg -version
ffprobe -version
gh --version
```

## Installation

Clone the repository:

```bash
git clone https://github.com/shazeus/TermFX.git
cd TermFX
```

Build the project:

```bash
cargo build
```

Run the tests:

```bash
cargo test
```

Show CLI help:

```bash
cargo run -- --help
```

## Quick Start

Create a new project:

```bash
cargo run -- new --name demo --project termfx.project.json
```

Add media to the project:

```bash
cargo run -- add-media \
  --project termfx.project.json \
  --path ./shot.mp4 \
  --kind video
```

The command returns a media id:

```text
Added media shot (6508eba6-7a9b-4eea-b9d0-6f7b92835c18)
```

Append that media to the timeline:

```bash
cargo run -- add-clip \
  --project termfx.project.json \
  --media-id 6508eba6-7a9b-4eea-b9d0-6f7b92835c18 \
  --track 0 \
  --start-seconds 0 \
  --duration-seconds 5
```

Open the terminal UI:

```bash
cargo run -- tui --project termfx.project.json
```

Preview the FFmpeg command without rendering:

```bash
cargo run -- render \
  --project termfx.project.json \
  --output out.mp4 \
  --dry-run
```

Render the video:

```bash
cargo run -- render \
  --project termfx.project.json \
  --output out.mp4
```

## MCP Server

Run the TermFX MCP server over stdio:

```bash
cargo run -- mcp --project termfx.project.json
```

Example MCP client configuration:

```json
{
  "mcpServers": {
    "termfx": {
      "command": "cargo",
      "args": [
        "run",
        "--manifest-path",
        "/absolute/path/to/TermFX/Cargo.toml",
        "--",
        "mcp",
        "--project",
        "/absolute/path/to/project/termfx.project.json"
      ]
    }
  }
}
```

Supported MCP lifecycle methods:

- `initialize`
- `notifications/initialized`
- `ping`
- `tools/list`
- `tools/call`

## MCP Tool Examples

List media and timeline state:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "list_media",
    "arguments": {}
  }
}
```

Append media to the timeline:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "append_media",
    "arguments": {
      "media_id": "6508eba6-7a9b-4eea-b9d0-6f7b92835c18",
      "track": 0,
      "start_seconds": 0,
      "duration_seconds": 5
    }
  }
}
```

Cut a timeline range with ripple delete:

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "cut_video",
    "arguments": {
      "mode": "remove_range",
      "start_seconds": 1.2,
      "end_seconds": 2.1,
      "ripple": true
    }
  }
}
```

Apply an `s_shake` effect to a clip:

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "apply_effect",
    "arguments": {
      "clip_id": "33c6f411-29d9-4e77-b606-4f444c0b5817",
      "effect": "s_shake",
      "params": {
        "amplitude_px": 18,
        "frequency_hz": 10,
        "seed": 0.4
      }
    }
  }
}
```

Add a text overlay:

```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "tools/call",
  "params": {
    "name": "apply_effect",
    "arguments": {
      "clip_id": "33c6f411-29d9-4e77-b606-4f444c0b5817",
      "effect": "text_overlay",
      "params": {
        "text": "FINAL CUT",
        "x": 120,
        "y": 80,
        "font_size": 56,
        "color": "white",
        "start_seconds": 0,
        "duration_seconds": 2.5
      }
    }
  }
}
```

Create a silence or beat-sync analysis plan:

```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "tools/call",
  "params": {
    "name": "smart_edit",
    "arguments": {
      "mode": "silence",
      "threshold_db": -35,
      "min_silence_seconds": 0.35,
      "dry_run": true
    }
  }
}
```

## Terminal UI

The TUI is organized into project assets, preview, inspector, timeline, and MCP
status panels:

```text
+--------------------------------------------------------------------------------+
| Project: TermFX       FPS: 30       Render: idle       MCP: connected           |
+----------------------+------------------------------------+--------------------+
| Project Assets       | Video Preview                      | Inspector          |
| - shot_01.mp4        | +---------------- preview --------+ | Track: V1          |
| - music.wav          | | ASCII/sixel/mpv preview         | | Clip params        |
| - logo.png           | | waveform/cache thumbnails       | | Effects: s_shake   |
|                      | +---------------------------------+ | Text/color/fade     |
+----------------------+------------------------------------+--------------------+
| Timeline & Layers                                                             |
| time    |0------------------------------|-----------------------------------> |
| V2      |........TITLE######...................................................|
| V1      |intro############....broll#############....outro########..............|
| A1      |music================================================================|
+--------------------------------------------------------------------------------+
| AI / MCP  list_media ok | apply_effect s_shake queued | render 42%             |
+--------------------------------------------------------------------------------+
```

Shortcuts:

- `q`: quit
- `up/down`: select track

## Project Structure

```text
src/
  core/
    effect.rs          Effect model and keyframe data types
    media.rs           Media asset model
    smart.rs           Smart edit analysis plan
    time.rs            FPS and frame/seconds conversion
    timeline.rs        Tracks, clips, trim, and ripple delete
  mcp/
    protocol.rs        JSON-RPC request/response types
    server.rs          MCP stdio server loop
    tools.rs           MCP tool schemas and handlers
  render/
    ffmpeg.rs          FFmpeg command and filtergraph builder
    filtergraph.rs     Escaping and time helpers
    progress.rs        Render progress model
  tui/
    app.rs             Terminal lifecycle and event loop
    layout.rs          TUI panel layout
    timeline_widget.rs Timeline drawing
  project.rs           JSON project model
  main.rs              CLI entrypoint
```

Detailed Turkish architecture notes:

[ARCHITECTURE_TR.md](ARCHITECTURE_TR.md)

## Development

Format:

```bash
cargo fmt
```

Test:

```bash
cargo test
```

Dry-run render:

```bash
cargo run -- render \
  --project termfx.project.json \
  --output out.mp4 \
  --dry-run
```

## Status

TermFX is in active development. The core timeline, MCP tool handlers, and
FFmpeg render path are functional. Planned production work includes:

- Automatic media metadata extraction with FFprobe
- Background render queue and progress parsing
- Preview cache, waveform, and proxy systems
- Keyframe interpolation
- Turning smart edit plans into real timeline mutations
- MCP resource support

## License

MIT
