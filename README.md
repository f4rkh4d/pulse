# pulse

one terminal window for all your local dev servers. because 6 tmux panes is still 6 tmux panes.

![boot](./docs/cast-boot.gif)

## quick start

```bash
cargo install pulse
cd your-project
pulse init       # scans for package.json / docker-compose / Procfile, drafts a pulse.toml
pulse            # reads pulse.toml and boots everything
```

or drop a `pulse.toml` in by hand:

```toml
[[service]]
name = "api"
cmd = "cargo run"
cwd = "./backend"
env = { PORT = "3000" }
color = "cyan"

[service.probe]
url = "http://localhost:3000/health"
interval = "5s"
timeout = "2s"

[service.port]
expect = 3000

[service.agent]
kind = "goblin"

[[service]]
name = "web"
cmd = "npm run dev"
cwd = "./frontend"
depends_on = ["api"]
color = "magenta"
```

restart `api` and `web` bounces automatically after a 1s grace.

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
| `t`           | open the tap panel for the selected service  |
| `T`           | split view on the latest tap request         |
| `g`           | full-screen dep graph                         |
| `?`           | help modal with all keybinds                  |
| `esc`         | close the current overlay                     |
| `q` / `ctrl+c`| quit. sigterm to children, sigkill if stuck  |

## subcommands

- `pulse init` — scan the cwd and draft a `pulse.toml`. reads `package.json` scripts (`dev`, `start`, `watch`, `test:watch`, `serve`), `docker-compose.yml` services, and `Procfile` entries
- `pulse ports` — list every process holding a LISTEN tcp port on this box. shells out to `lsof`, unix only
- `pulse logs <service> [--lines N]` — run a single service and print its logs. good for pipelines
- `pulse share [--out path]` — export current config as an html snapshot
- `pulse theme dump` — print the default palette as a starter `theme.toml`

## agents

each service gets an ASCII sentinel living next to its name. five species: `goblin`, `cat`, `ghost`, `robot`, `blob`. pick one via `[service.agent] kind = "..."`. they react to the service state and occasionally say something in the status bar.

example afternoon:

```
●  ᨀ  api    00:04:21
      200 · 42ms · 99%
      :3000 bound

[api] uh, that service just died
... (crash, exit 1) ...

●  X_X api    00:00:00
[api] rip api, gone but not forgotten (it's been 3s)
```

then you fix your bug, `r` to restart:

```
[api] api back up. third time's the charm
```

the robot species reads like a little sysadmin (`api: systems nominal`). the cat bats at invisible bugs when probes are slow. the ghost flickers. i spent more time on the message pools than i should have.

## compared to mprocs

mprocs is great. i used it for a year. pulse is meant to go further: http probing, port detection, restart cascade on deps, tiny goblins that tell you when something died. v0.2 ships all four. if you want stable today, use mprocs. if you want this direction, stick around.

## what's in v0.2

- **http probes** per service. toml declares `url`, `interval`, `timeout`, optional `expect_status`. sidebar shows status code, latency, rolling success rate over the last 60 probes
- **port detection**. `[service.port] expect = 3000` polls with a tcp-connect every 2s. badge shows `:3000 bound` (green) or `:3000 free` (dim)
- **ascii agents** — goblin / cat / ghost / robot / blob sentinels that change face with state and speak in the status bar on transitions
- **auto-discovery** via `pulse init`
- **dependency-aware restart** — `depends_on = ["postgres"]` makes `web` bounce after postgres comes back, with a 1s grace
- **config hot-reload** via the `notify` crate. save `pulse.toml`, diff runs, new services spawn, removed ones get killed
- **config validation** — misspelled keys, unparseable durations, unknown agent kinds, circular deps all fail loud at load time with a useful error

## what's new in v0.3

three headline features, all things i kept switching to other tools for:

- **traffic tap**. point pulse at your service port, it proxies and logs every request inline. `t` opens a live panel, `T` splits on the latest request with headers + body preview
- **dep graph**. `g` draws a full-screen ascii tree of `depends_on` edges. color-coded by overall stack health
- **`pulse share`**. single-file html snapshot of your stack — services, statuses, last 50 tap events each. no cdn, no fonts. scp it to a coworker

smaller stuff: theme files at `~/.config/pulse/theme.toml`, a `pulse logs <svc>` subcommand for piping, a help modal (`?`), `[global] stop_timeout` for tuning shutdown grace.

## gallery

- ![boot](./docs/cast-boot.gif) — cold start, everything coming up
- ![probes](./docs/cast-probe.gif) — probes flipping green, then a service crashing
- ![tap](./docs/cast-tap.gif) — traffic tap + request detail split
- ![graph](./docs/cast-graph.gif) — `g` for the dep graph
- ![share](./docs/cast-share.gif) — `pulse share` making an html snapshot

gifs live in `docs/`. they're placeholders right now, real asciinema casts land before 0.4.

## benchmarks

measured on an m2 air, macos 14, cold cargo cache. ymmv, obviously.

| thing                                  | number       |
|----------------------------------------|--------------|
| cold boot, 6 services, no probes       | ~180ms       |
| cold boot, 6 services, http probes on  | ~210ms       |
| resident memory, 6 services idle       | ~14 MB rss   |
| per-probe bookkeeping overhead         | ~0.9 µs      |
| tap proxy roundtrip overhead (loopback)| ~0.4 ms p50  |

probe bookkeeping bench is in `benches/probe_overhead.rs`, run with `cargo bench`. boot-time numbers are stopwatch-grade, not statistical, so take them loosely. the tap overhead was measured with `hey -n 5000 -c 20` against a local rust `hello world` with and without `pulse` in front.

## honest notes

- spike detection for the agents is currently a proxy: fast probe + recent activity. a real req/s meter lands later
- the config watcher leaks a single `Watcher` box so it lives for the process. ugly but works
- ascii faces use a handful of non-ascii glyphs (ᨀ, ʘ, •). on some fonts they render wide, on a few they get boxes. the robot species is pure ascii if you hit that
- windows still unsupported, still untested
- i wrote this as a second-year project. it's held together by tests and optimism

## license

MIT
