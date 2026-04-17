# pulse

one terminal window for all your local dev servers. because 6 tmux panes is still 6 tmux panes.

![demo](./docs/demo.gif)

## quick start

```bash
cargo install pulse
cd your-project && cp ~/.cargo/registry/src/*/pulse-*/examples/pulse.toml ./pulse.toml
pulse
```

or drop a `pulse.toml` next to your code by hand:

```toml
[[service]]
name = "api"
cmd = "cargo run"
cwd = "./backend"
env = { PORT = "3000" }
color = "cyan"

[[service]]
name = "web"
cmd = "npm run dev"
cwd = "./frontend"
color = "magenta"

[[service]]
name = "postgres"
cmd = "postgres -D /tmp/pg"
color = "yellow"
```

run `pulse`. all three boot together. pick one on the left, read its logs on the right. ctrl+c and everything stops.

## keybinds

| key           | does                                         |
|---------------|----------------------------------------------|
| `j` / `k`     | move up/down the service list (arrows work)  |
| `r`           | restart the highlighted service              |
| `s`           | stop the highlighted service                 |
| `S`           | stop every service                            |
| `enter`       | toggle logs focus (bigger view)              |
| `/`           | start a regex filter on the current logs     |
| `c`           | clear the current service's log buffer       |
| `q` / `ctrl+c`| quit. sigterm to children, sigkill if stuck  |

## compared to mprocs

mprocs is great. i used it for a year. pulse is meant to go further: http probing, port auto-detection, live traffic tap. those land in v0.2. today v0.1 is honestly "mprocs with nicer defaults and a roadmap." rounded borders, tokyonight-ish palette, per-service color tags on every log line, ansi passthrough from children.

if you need stable today, use mprocs. if you want the direction pulse is going, stick around.

## roadmap

**v0.2** — the observation layer
- http probes (user defines `GET /health`, pulse shows up/down/latency)
- port detection (noticing which ports a service opens)
- auto-discovery (scan package.json, cargo manifests, docker-compose for likely services)

**v0.3** — deeper taps
- live http traffic tap between services (think mitmproxy, but inline)
- dependency graph (`web` waits for `api` ready, `api` waits for `postgres`)
- share a session over the network for pair debugging

## honest note

probably has rough edges. i'm a first-year-out-of-highschool generalist and this is my first serious rust tui. the event loop polls exited children on a 150ms tick instead of watching handles cleanly. ansi rendering works for simple cases but complex cursor-movement escapes (think `cargo watch` clearing the screen) will look weird. windows is unsupported right now, not tested. pr + issues welcome.

## license

MIT
