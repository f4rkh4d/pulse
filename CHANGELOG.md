# changelog

all notable changes to pulse. dates in iso, semver-ish. lowercase because that's how i roll.

## 0.3.0 — 2026-05-04

the traffic-tap release. got tired of flipping to a separate browser devtools just to see what my own api was doing.

### added
- **traffic tap**. `[service.tap] mode = "proxy" listen = 13000` spawns a tcp reverse-proxy on that port, forwards to your service, records every round-trip. sidebar gets a live `t` panel, `T` opens a request detail split with headers + body preview (4kb cap). ring buffer of 500 per service
- **dep graph view**. `g` pops a full-screen ascii layout of `depends_on` edges. layered top-to-bottom, box-drawing glyphs, header color flips green/yellow/red with overall health
- **`pulse share`**. dumps current state to `pulse-snapshot-<ts>.html`. self-contained, no cdn, no fonts. good for airdropping a repro to a coworker
- **theme files**. drops at `~/.config/pulse/theme.toml`. override colors + border type. `pulse theme dump` prints defaults as a starter. `pulse theme path` shows where it'd look
- **`pulse logs <service>`**. tails a single service without the tui. `--lines N` for a bounded window, `0` to follow
- **`[global]` config**. `stop_timeout = "3s"` controls the sigterm→sigkill grace (was hardcoded 1.5s). `log_buffer` field reserved, current ring stays at 2000
- **help modal**. `?` pops a boxed overlay with every keybind grouped by intent. `esc` closes overlays

### changed
- `Config` struct gained a `global` field. `ServiceSpec` gained `tap`
- internal Palette type with hex-color fallback so theming is a single source of truth

### notes
- passive-mode tap is a stub. says so in the system log when you configure it. proxy mode is the real feature and what you should use
- the proxy forwards raw bytes after the first header block. websocket upgrades pass through but don't get re-parsed
- the share exporter run as a subcommand only sees config, not live probe/tap state — the subcommand stubs those fields. same file format as the in-tui export would produce

## 0.2.0 — 2026-03-21

the one where services got agents and started talking.

### added
- **http probes**. `[service.probe]` with `url`, `interval`, `timeout`, optional `expect_status`. sidebar badge shows last status code, latency, rolling success rate over 60 samples
- **port detection**. `[service.port] expect = 3000` pings tcp every 2s, shows bound/free
- **ascii agents**. five species (goblin, cat, ghost, robot, blob). faces change with status, they say things in the statusbar on transitions
- **auto-discovery** via `pulse init` — reads package.json scripts, docker-compose services, Procfile entries
- **`depends_on`** with restart cascade. bouncing a parent bounces its children after a 1s grace
- **config hot-reload**. edit `pulse.toml`, diff runs, new services spawn, removed ones get killed
- **config validation** — unknown keys, unparseable durations, bad agent kinds, circular deps all fail at load with a useful message

## 0.1.0 — 2026-02-14

the mvp. enough to replace my tmux setup.

### added
- tui with sidebar + logs panel + statusbar
- tokio supervisor: spawn, stdout/stderr piping, sigterm→sigkill shutdown, restart with backoff
- `pulse.toml` parsing (toml + serde)
- `j/k/r/s/S/enter//c/q` keymap with filter mode
- `pulse init` and `pulse ports` subcommands
- ci on ubuntu + macos
