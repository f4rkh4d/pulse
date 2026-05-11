# pulse

one window for every local dev server. six tmux panes is still six tmux panes, and you know it.

![boot](./docs/cast-boot.gif)

```bash
cargo install pulse
cd your-project
pulse init    # scans for package.json / docker-compose / Procfile, drafts a pulse.toml
pulse
```

pulse reads `pulse.toml`, spawns every service, probes their health, watches their ports, and draws a tui with logs per service. restart one and its dependents bounce on their own. type `t` to tap live http traffic. type `g` for an ascii dep graph. that's most of it.

## why another one of these

mprocs runs commands in panes. great tool, i used it for a year. pulse is what i kept wishing it did:

| feature                   | pulse | mprocs | tmux | pm2 |
|---------------------------|-------|--------|------|-----|
| multiplex processes       |  yes  |  yes   | yes  | yes |
| per-service log panes     |  yes  |  yes   | diy  | yes |
| http health probe         |  yes  |   no   |  no  | diy |
| port-in-use detection     |  yes  |   no   |  no  |  no |
| restart cascade on deps   |  yes  |   no   |  no  |  no |
| live traffic tap + replay |  yes  |   no   |  no  |  no |
| ascii sentinel per svc    |  yes  |   no   |  no  |  no |
| config in one toml file   |  yes  |  yes   |  no  | yes |
| auto-discover from repo   |  yes  |   no   |  no  |  no |

pm2 is production-shaped and it shows in local dev. tmux is a multiplexer, not a process manager. mprocs is still the thing to beat.

## what you get

- **probes**. per-service http poll, latency + rolling success rate in the sidebar
- **port watch**. `[service.port] expect = 3000` → green `:3000 bound` badge, dim `:3000 free` when it's not
- **traffic tap**. tiny reverse-proxy records the last 500 requests per service. `t` opens the list, `T` splits on the latest request with headers + body preview
- **dep restart**. `depends_on = ["postgres"]` bounces `web` after postgres comes back. 1s grace, configurable
- **ascii sentinels**. one of five species per service (goblin, cat, ghost, robot, blob). they react to state and say things in the status bar
- **auto-discover**. `pulse init` reads `package.json` scripts, `docker-compose.yml` services, `Procfile` entries and drafts a config

## sentinels

teaser, three species:

```
  ᨀ    goblin    "api back up. third time's the charm"
  ≋    blob      "postgres is just vibing at 5432"
  ᓚ    cat       "probe latency is weird, i'm watching it"
```

the robot species is pure ascii for fonts that don't like the others. pick per service via `[service.agent] kind = "..."`.

## gallery

- ![boot](./docs/cast-boot.gif) — cold start, everything coming up
- ![probes](./docs/cast-probe.gif) — probes flipping green, then a service crashing
- ![tap](./docs/cast-tap.gif) — traffic tap + request detail split
- ![graph](./docs/cast-graph.gif) — `g` for the dep graph
- ![share](./docs/cast-share.gif) — `pulse share` making an html snapshot

gifs live in `docs/`. they're placeholders right now, real asciinema casts land before 0.4.

## stack examples

runnable `pulse.toml` for common stacks. copy into your project root, tweak cwds.

- [next.js + postgres + redis + stripe](./examples/next-postgres-redis.toml)
- [django + postgres + celery + beat](./examples/django-celery.toml)
- [rails + postgres + sidekiq + js](./examples/rails-sidekiq.toml)
- [rust api + sqlite + nginx (docker) + esbuild](./examples/rust-api-docker.toml)

each sets probes on the right health endpoints, uses `depends_on` to cascade restarts, picks a different sentinel species, and sets up a traffic tap on the http service.

## minimal config

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

save `pulse.toml`, run `pulse`. edit the file while it's running and the watcher diffs + restarts affected services.

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

- `pulse init` — scan cwd and draft a `pulse.toml`. reads `package.json` scripts (`dev`, `start`, `watch`, `test:watch`, `serve`), `docker-compose.yml` services, and `Procfile` entries
- `pulse ports` — list every process holding a LISTEN tcp port on this box. shells out to `lsof`, unix only
- `pulse logs <service> [--lines N]` — run a single service and print its logs. good for pipelines
- `pulse share [--out path]` — export current config as a single-file html snapshot
- `pulse theme dump` — print the default palette as a starter `theme.toml`
- `pulse completions <shell>` — emit a completion script. bash/zsh/fish/powershell/elvish

shell completion, one-liners:

```bash
pulse completions bash > /usr/local/etc/bash_completion.d/pulse
pulse completions zsh  > ~/.zsh/completions/_pulse
pulse completions fish > ~/.config/fish/completions/pulse.fish
```

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

## compared to mprocs, again

short version: if you want something that works today, rock solid, big community, use mprocs. if you want probes + ports + tap + cascades in one config file, that's this. both are fine choices.

## honest notes

- spike detection for the sentinels is a proxy: fast probe + recent stdout. a real req/s meter lands in 0.4
- the config watcher leaks a single `Watcher` box so it lives for the process. i know. still works
- sentinels use a couple of non-ascii glyphs (ᨀ, ʘ, ≋). on some fonts they render wide, on a few they become boxes. the robot species is pure ascii for those
- windows still untested. i don't have a windows machine and i'm not great at CI
- wrote this as a second-year project. it's held together with tests and optimism

## license

MIT
