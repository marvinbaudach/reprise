# reprise-mcp — control Reprise from an agent

`reprise-mcp` is a local, stdio-only [Model Context Protocol](https://modelcontextprotocol.io)
server that exposes your Reprise music library and cached remote sources to an
AI agent (Claude, or any MCP client). An agent can search your library, manage
podcast/YouTube subscriptions and radio favorites, build playlists, create
instrumental versions, and **drive playback** — so you can run your whole music
setup by asking an agent, without touching the GUI.

It is a **controller, not a player**: it produces no audio itself. Playback and
the actual audio pipeline live in the Reprise desktop app; the MCP reaches the
running app over the D-Bus session bus (MPRIS + a small Reprise-specific
interface). For playback to work, **the Reprise app must be running** (it can be
in the background) — otherwise playback tools return a clear "no running Reprise
app" error.

Everything else (search, source management, playlists, concerts, releases,
instrumentals)
reads/writes the library database directly and works whether or not the app is
running.

---

## Build

The playback tools live behind the opt-in `mpris` cargo feature (Linux/D-Bus).
Build the release binary with it enabled:

```sh
cargo build --locked -p reprise-mcp --release --features mpris
# -> target/release/reprise-mcp
```

A plain `cargo build -p reprise-mcp` (no features) stays D-Bus-free and omits
the playback-state, playback-control, targeted-play, and live-queue tools.

Install it somewhere stable, e.g.:

```sh
install -m755 target/release/reprise-mcp ~/.local/bin/reprise-mcp
```

The server opens your live library at the XDG default
(`~/.local/share/reprise/reprise.db`), or wherever `--db <path>` points.

---

## Connect it to an agent

### Codex

Register the server once:

```sh
codex mcp add reprise -- ~/.local/bin/reprise-mcp --db ~/.local/share/reprise/reprise.db
```

Start a fresh Codex session afterward. MCP tool discovery happens when the
session starts, so an already-open session does not see a newly installed
binary or newly added tools.

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
| `music_search_artists` | `query`, optional `limit`/`offset` | Search path-free artist summaries | `library:read` |
| `music_search_albums` | `query`, optional `limit`/`offset` | Search path-free album summaries | `library:read` |
| `music_get_playlist` | `playlist_id`, optional `limit`/`offset` | Read ordered playlist contents | `library:read` |
| `music_create_playlist` | `name`, `track_ids` | Create a manual playlist | `playlist:create` |
| `music_update_playlist` | `action` = `rename`\|`add_tracks`, plus playlist/name/track ids | Safely rename a playlist or append tracks | `playlist:manage` |
| `music_create_instrumental` | `track_ids` | Queue vocal-removal (htdemucs) render jobs | `ai:create` |
| `music_get_job_status` | job/batch id | Progress of a render job | `library:read` |
| `music_manage_podcasts` | `action` = `add`\|`edit`\|`remove`\|`refresh`, plus action fields | Read RSS/YouTube with yt-dlp and manage cached subscriptions | `sources:manage` |
| `music_manage_radio` | `action` = `add`\|`edit`\|`remove`, plus action fields | Manage radio favorites; URL-only add reads ICY metadata | `sources:manage` |
| `music_playback_control` | `action` = `play`\|`pause`\|`stop`\|`next`\|`previous` | Transport-control the running app | `playback:control` |
| `music_get_playback_state` | none | Read live track, position, volume, shuffle, and repeat state | `playback:control` |
| `music_set_playback` | `action` plus `volume`, `offset_seconds`, `enabled`, or `repeat` | Set volume, seek, shuffle, or repeat | `playback:control` |
| `music_play` | exactly one of `track_ids` or `playlist_id` | Play a track list or a whole playlist | `playback:control` |
| `music_queue` | `action` = `status`\|`add_next`\|`add_last`\|`clear` | Read or safely change the manual Play Next queue | `playback:control` |

`music_play` resolves a `playlist_id` to its ordered tracks itself, then tells
the app to play them. Only **present** (non-missing) tracks are playable.

`music_queue` keeps the hidden playback context intact when clearing: `clear`
removes only manually queued Play Next entries. Queue status returns at most
200 ids from each section, together with the complete section totals.

`music_manage_podcasts` accepts an RSS feed URL or a YouTube channel/playlist
URL for `add`. RSS is parsed directly; YouTube is listed through the configured
`yt-dlp`. `refresh` explicitly refreshes every active subscription. `edit`
changes the display title and/or auto-download setting. `remove` keeps already
downloaded media files.

`music_manage_radio` accepts an HTTP(S) stream, PLS, M3U, or HLS URL. PLS/M3U
playlists are resolved to their first playable stream; HLS keeps its manifest
URL. `name` is optional for `add`; when omitted, the tool probes ICY headers for
the station name and metadata. `edit` can replace the name, stream URL, genre,
codec, bitrate, country, or vote count.

### Resources

- `reprise://library/summary` — track/artist/album counts and total duration.
- `reprise://playlists` — the playlist list.
- `reprise://concerts` — upcoming concerts after the saved UI filters.
- `reprise://concerts/all` — every cached concert-event field, including past
  and currently filtered-out events, plus effective non-secret Concerts
  configuration. Provider keys are represented only as configured/not
  configured booleans.
- `reprise://releases` — every durable New Releases history field, including
  hidden entries, timestamps, MusicBrainz ids, announcement links, and derived
  local-library presence.
- `reprise://podcasts` — up to 200 cached subscriptions and recent episodes.
- `reprise://radio` — up to 200 cached radio favorites.

The source resources deliberately omit feed, episode-media, artwork, homepage,
and stream URLs because stored URLs may contain private access tokens. Source
mutations return opaque IDs and display metadata without echoing submitted
URLs.

---

## Capabilities (what the agent is allowed to do)

Each capability is a boolean setting in the library DB, read live by the server.
Defaults follow "read is safe, writes are opt-in":

| Setting key | Default | Grants |
|-------------|---------|--------|
| `agent.capability.library:read` | **on** | search + resources |
| `agent.capability.playback:control` | **on** | transport, live state/settings, targeted play, and queue |
| `agent.capability.playlist:create` | off | `music_create_playlist` |
| `agent.capability.playlist:manage` | off | playlist rename + append tracks |
| `agent.capability.ai:create` | off | `music_create_instrumental` |
| `agent.capability.sources:manage` | off | podcast/YouTube and radio add/edit/remove/refresh |

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
- Track, artist, album, and playlist reads are paginated. The live queue mirror
  is bounded to 200 ids per section, so large libraries and long playback
  contexts do not create oversized MCP responses.
- Podcast and radio resources are cache-only reads bounded to 200 items. Only
  explicit `music_manage_*` calls perform network access or mutate source data.
- **`music_create_instrumental` only enqueues** a job; the actual htdemucs render
  is done by a worker — the running app's background worker, or
  `reprise-cli jobs work` — and needs the stem model provisioned. Track progress
  with `music_get_job_status`.
- The server is stdio-only: stdout carries MCP protocol frames, all logs go to
  stderr (set `REPRISE_LOG=debug` for verbose logs).
- Responses never leak filesystem paths, cache/db locations, or credentials.
