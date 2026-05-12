# contributing

hey, thanks for looking. a few quick things before you open a pr.

## what lands easily

- bug fixes with a repro case (a failing test is ideal, a paste of the error + your `pulse.toml` works too)
- small docs / readme fixes, typos, clearer error wording
- more examples in `examples/` for stacks i haven't covered
- a new sentinel species if you want to write one (see `src/agents.rs`)
- windows port work. i can't test it myself so i'd merge careful pr's gratefully

## what's worth discussing first

- any new top-level subcommand
- new fields on `pulse.toml` (schema is `deny_unknown_fields`, so adding a key means breaking someone's config)
- anything that touches the supervisor or shutdown path
- new dependencies. binary size matters

open an issue and we'll talk. "please add X" is fine, no formal rfc needed, just a paragraph.

## local setup

you need rust 1.75 or later. no other system deps.

```bash
git clone https://github.com/f4rkh4d/pulse
cd pulse
cargo test
cargo run -- --config examples/pulse.toml
```

if you're touching the tui, `cargo run` in a second terminal is usually fastest. if you're touching parsing/config, the unit tests are the fastest loop.

## before you open a pr

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

all three clean. if any of them aren't, say so in the pr description.

## commit style

lowercase conventional-commits, scope optional, subject ≤ 60 chars. no trailers, no emojis.

examples:

```
fix: tap proxy leaks fd on early client close
readme: shorter pitch, drop outdated feature flag
ui: make help modal resize on tiny terminals
```

one logical change per commit when you can. if you have to mash them together, that's fine too, don't stress.

## code of conduct

be chill. disagreement is fine, condescension isn't. if someone's new to rust or tuis, help them out instead of dunking.
