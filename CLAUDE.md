# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build

# Run
cargo run

# Format
cargo fmt --all

# Linting
cargo clippy --all-targets --all-features -- -D warnings

# Test all
cargo test

# Run a single test by name
cargo test <test_name>

# Run tests in a specific module
cargo test core::store::tests

# Run ignored tests
cargo test -- --ignored
```

The server starts on port 3000. To create a timer:
```bash
curl -X POST http://localhost:3000/timer -d '{"interval_ms": 5000}'
```

## Architecture

The project is a timer service built around a **hierarchical timing wheel** — the same data structure used in Linux kernel timer management. Timers are stored in one of four places depending on how far in the future they fire:

1. **Short wheel** (`core/wheel.rs`) — 128 buckets × 8ms resolution ≈ 1 second range
2. **Long wheel** (`core/wheel.rs`) — 4096 buckets × ~1 second resolution ≈ 1 hour range
3. **Heap** (`core/wheel.rs`, `TimerHeap`) — tombstone-based lazy deletion for timers beyond ~1 hour
4. **Overdue bucket** (`core/store.rs`) — timers that were already past-due when inserted

`Store` (`core/store.rs`) owns all four structures and a `Clock` dependency. `Store::pop()` advances the internal tick, cascades timers down from heap → long wheel → short wheel as time passes, and returns the set of expired timers.

### Data flow

```
HTTP POST /timer
    → TimerHandler (infra/handlers.rs)
    → TimerEvent::Insert sent on mpsc channel
    → EventReceiver (infra/event_receiver.rs) streams events
    → App (infra/app.rs) selects on event stream + fired timer channel
    → EventHandler (core/event_handler.rs) owns Store, ticks every 2ms
    → Expired timers sent on timer_sender channel back to App
```

### Layers

- `core/` — pure logic: `Timer`, `TimerId`, `Clock` trait, `Wheel`, `TimerHeap`, `Store`, `EventHandler`
- `infra/` — wiring: `App`, `MainProgram`, `EventReceiver`, HTTP handlers
- `utils/` — HTTP server/router abstraction, `Logger` trait + `StdoutLogger`

## Development Standards
- **Formatting:** ALWAYS run `cargo fmt --all` before committing.
- **Linting:** Code MUST pass `cargo clippy --all-targets --all-features -- -D warnings`.
- **Testing:** Run `cargo test` to verify changes.
