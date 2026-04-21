# aiem Web UI

Headless browser-based management for the aiem skill & MCP ecosystem. Designed
to run on a remote Linux server and be accessed from your laptop via SSH port
forwarding — no public port, no passwords, no Node.js.

## Quick start

```bash
# On the remote server:
cargo install --path crates/aiem-cli
aiem serve                       # listens on 127.0.0.1:8787

# On your laptop:
ssh -L 8787:localhost:8787 user@server
# then open http://localhost:8787 in any browser
```

Flags:

- `--host 127.0.0.1` (default, loopback only — the safe choice)
- `--port 8787`
- `--open` open the browser automatically (only useful locally)

## Persistent service (systemd)

```bash
# On the remote Linux box:
sudo cp crates/aiem-web/packaging/aiem.service /etc/systemd/system/
sudo sed -i 's/USERNAME/'"$USER"'/g' /etc/systemd/system/aiem.service
sudo systemctl daemon-reload
sudo systemctl enable --now aiem
sudo systemctl status aiem
```

## Features (MVP)

All 9 tabs mirror the desktop GUI:

| Tab       | Path        | Capabilities                                                     |
|-----------|-------------|------------------------------------------------------------------|
| Skills    | /skills     | list / add / update / remove / deploy / group-sync               |
| MCP       | /mcp        | list / add stdio / toggle / remove / sync-all                    |
| Secrets   | /secrets    | list / set / delete (OS keyring)                                 |
| Profiles  | /profiles   | create / activate / deactivate / remove                          |
| Projects  | /projects   | register / remove                                                |
| Discover  | /discover   | scan disk + IDE configs, import unmanaged skills/MCP             |
| Store     | /store      | search smithery.ai · glama.ai · skills online                    |
| IDEs      | /ides       | read-only list of supported IDE targets                          |
| Settings  | /settings   | GITHUB_TOKEN management, paths, host info                        |

## Architecture

```
browser  ─── HTML/HTMX ───▶  axum 0.7  ──▶  aiem-core (SkillRegistry,
   ▲                          │                         McpRegistry, Vault,
   │                          │                         ProfileStore, ...)
   └── SSE  /events ──────────┘
```

- **No authentication** — loopback-only binding makes SSH your auth boundary.
- **SSE** pushes toast / task progress / invalidation events to every open tab.
- **HTMX** listens for `invalidate` events and refreshes the relevant list in
  place without a full page reload.
- **Single binary** — `aiem serve` is a subcommand of the existing `aiem` CLI
  under a default-on `web` cargo feature. Disable with `--no-default-features`
  to build a pure CLI without the HTTP stack.

## Remote security checklist

- [ ] Never use `--host 0.0.0.0` unless you put a reverse proxy in front
- [ ] SSH key-only access to the server
- [ ] Run `aiem serve` as a non-root user
- [ ] Keep `GITHUB_TOKEN` in the keyring, not in systemd Environment=
