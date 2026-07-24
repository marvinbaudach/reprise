# reprise-mcp — control Reprise from an agent

`reprise-mcp` is a local, stdio-only [Model Context Protocol](https://modelcontextprotocol.io)
server that exposes your Reprise music library to an AI agent (Claude, or any
MCP client). An agent can search your library, build playlists, create
instrumental versions, and **drive playback** — so you can run your whole music
setup by asking an agent, without touching the GUI.

It is a **controller, not a player**: it produces no audio itself. Playback and
the actual audio pipeline live in the Reprise desktop app; the MCP reaches the
running app over the D-Bus session bus (MPRIS + a small Reprise-specific
interface). For playback to work, **the Reprise app must be running** (it can be
in the background) — otherwise playback tools return a clear "no running Reprise
app" error.

Everything else (search, playlists, instrumentals) reads/writes the library
database directly and works whether or not the app is running.

---

## Build

The playback tools live behind the opt-in `mpris` cargo feature (Linux/D-Bus).
Build the release binary with it enabled:

```sh
cargo build --locked -p reprise-mcp --release --features mpris
# -> target/release/reprise-mcp
```

A plain `cargo build -p reprise-mcp` (no features) stays D-Bus-free and simply
omits `music_playback_control` / `music_play`.

Install it somewhere stable, e.g.:

```sh
install -m755 target/release/reprise-mcp ~/.local/bin/reprise-mcp
```

The server opens your live library at the XDG default
(`~/.local/share/reprise/reprise.db`), or wherever `--db <path>` points.

---

## Connect it to an agent

### Claude Code (CLI)

Register it once at **user scope** (personal, not committed — the DB path is
machine-specific). Either run:

```sh
claude mcp add reprise --scope user -- ~/.local/bin/reprise-mcp --db ~/.local/share/reprise/reprise.db
```

…or add it directly to the `mcpServers` block of `~/.claude.json`:

```json
{
  "mcpServers": {
    "reprise": {
      "command": "/home/<you>/.local/bin/reprise-mcp",
      "args": ["--db", "/home/<you>/.local/share/reprise/reprise.db"]
    }
  }
}
```

Restart / reload the Claude Code session — MCP servers are loaded at session
start. Then ask, e.g. *"how many tracks do I have?"*, *"play my Focus
playlist"*, *"make an instrumental of this track"*.

### Claude Desktop

Add the same server to Claude Desktop's `claude_desktop_config.json`
(`~/.config/Claude/claude_desktop_config.json` on Linux), under `mcpServers`,
with the same `command`/`args`, and restart the app. Because the MCP is a local
stdio server that talks to your local library and your local Reprise app, the
client that launches it must run on the **same machine** as Reprise.

---

## What the agent can do

### Tools

| Tool | Arguments | Effect | Capability |
|------|-----------|--------|------------|
| `music_search_tracks` | `query`, optional `limit`/`offset` | Search the library | `library:read` |
| `music_create_playlist` | `name`, `track_ids` | Create a manual playlist | `playlist:create` |
| `music_create_instrumental` | `track_ids` | Queue vocal-removal (htdemucs) render jobs | `ai:create` |
| `music_get_job_status` | job/batch id | Progress of a render job | `library:read` |
| `music_playback_control` | `action` = `play`\|`pause`\|`stop`\|`next`\|`previous` | Transport-control the running app | `playback:control` |
| `music_play` | exactly one of `track_ids` or `playlist_id` | Play a track list or a whole playlist | `playback:control` |

`music_play` resolves a `playlist_id` to its ordered tracks itself, then tells
the app to play them. Only **present** (non-missing) tracks are playable.

### Resources

- `reprise://library/summary` — track/artist/album counts and total duration.
- `reprise://playlists` — the playlist list.

---

## Capabilities (what the agent is allowed to do)

Each capability is a boolean setting in the library DB, read live by the server.
Defaults follow "read is safe, writes are opt-in":

| Setting key | Default | Grants |
|-------------|---------|--------|
| `agent.capability.library:read` | **on** | search + resources |
| `agent.capability.playback:control` | **on** | transport + `music_play` |
| `agent.capability.playlist:create` | off | `music_create_playlist` |
| `agent.capability.ai:create` | off | `music_create_instrumental` |

To grant a write capability, set its key to `1` in the library DB, e.g.:

```sh
sqlite3 ~/.local/share/reprise/reprise.db \
  "INSERT INTO settings(key,value) VALUES('agent.capability.playlist:create','1')
   ON CONFLICT(key) DO UPDATE SET value='1';"
```

A fresh **grant** takes effect the next time the MCP server starts (i.e. your
next agent session); a **revocation** (`'0'`) is honored immediately on the next
call. `playback:control` and `library:read` are read live, so they also take
effect on the next call.

---

## Notes & limits

- **Playback needs the running app** (audio + D-Bus live there). No app → a clear
  "no running Reprise app on the session bus — start the app first" error.
- **`music_create_instrumental` only enqueues** a job; the actual htdemucs render
  is done by a worker — the running app's background worker, or
  `reprise-cli jobs work` — and needs the stem model provisioned. Track progress
  with `music_get_job_status`.
- The server is stdio-only: stdout carries MCP protocol frames, all logs go to
  stderr (set `REPRISE_LOG=debug` for verbose logs).
- Responses never leak filesystem paths, cache/db locations, or credentials.
