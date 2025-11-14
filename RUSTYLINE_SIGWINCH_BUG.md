# rustyline SIGWINCH Panic Bug Report

## Summary

rustyline's global SIGWINCH (window resize) signal handler can panic with `fd != -1` when:
1. A temporary `DefaultEditor` is created and dropped
2. The signal handler persists after the editor is dropped
3. The terminal is resized while the program is in a waiting state (e.g., tokio runtime parking)

## Reproduction

We provide two standalone programs that reproduce this issue. Both demonstrate the **nested editors** pattern that triggers the bug.

### Simple Reproduction (Nested Editors)

```bash
cargo run --bin rustyline_sigwinch_repro
```

1. Program creates a main REPL with `DefaultEditor`
2. Type `confirm` to trigger a nested temporary editor
3. Answer `y` to the confirmation prompt
4. Nested editor is dropped
5. **Resize your terminal window multiple times**
6. May panic with `fd != -1` from rustyline's SIGWINCH handler

The key is **nesting**: main REPL editor + temporary confirmation editors creates conflicting signal handler state.

### Tokio-based Reproduction (Most Accurate)

This closely simulates the actual kimichat multi-agent scenario:

```bash
cargo run --bin rustyline_sigwinch_tokio_repro
```

1. Program creates main REPL with `DefaultEditor`
2. Type `run` to start an agent task
3. Agent creates **nested temporary editors** for tool confirmations
4. Answer `y` to each confirmation (3 total)
5. After each confirmation, agent returns to tokio async execution
6. **Resize terminal window during agent processing (5-second countdown)**
7. May panic from SIGWINCH handler while tokio runtime is parked

This version is most likely to reproduce because:
- Multiple nested editor creations
- Active async execution between confirmations
- Tokio runtime parking/waiting states
- Exactly matches the kimichat usage pattern

## Panic Details

```
thread 'main' panicked at /Users/[user]/.cargo/registry/src/.../rustyline-14.0.0/src/tty/unix.rs:1197:28:
fd != -1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

thread 'main' panicked at library/core/src/panicking.rs:225:5:
panic in a function that cannot unwind
```

Key stack frames:
- `rustyline::tty::unix::sigwinch_handler` - Signal handler fires
- `parking_lot::condvar::Condvar::wait` - Waiting in condvar
- `tokio::runtime::park::CachedParkThread::park` - Tokio runtime parked

## Root Cause

1. **Signal Handler Registration**: When `DefaultEditor::new()` is called, rustyline registers a global SIGWINCH signal handler via `sigaction()`

2. **Handler Persistence**: This signal handler is registered at the process level and persists even after the `DefaultEditor` is dropped

3. **Invalid File Descriptors**: The handler expects to access file descriptors (stdin/stdout) that may no longer be valid or may be in use elsewhere

4. **Panic in Signal Context**: When window resize happens, the signal handler fires in async context (during tokio parking) and panics when it finds `fd == -1`

5. **Non-Unwinding Panic**: Signal handlers cannot unwind, so this becomes a fatal abort

## Problematic Usage Pattern

The bug is most easily triggered with **nested editors**:

```rust
// ❌ PROBLEMATIC: Nested rustyline editors with async execution
#[tokio::main]
async fn main() {
    // Main REPL editor
    let mut main_rl = DefaultEditor::new()?;

    loop {
        let line = main_rl.readline(">>> ")?;

        if line == "run_agent" {
            run_agent_with_confirmations().await; // ← Window resize during this = PANIC
        }
    }
}

async fn run_agent_with_confirmations() {
    // Agent does work...

    // Creates NESTED temporary editor for tool confirmation
    if get_tool_confirmation() {
        // Nested editor dropped, back in tokio async context
        do_tool_work().await; // ← SIGWINCH here can panic
    }
}

fn get_tool_confirmation() -> bool {
    let mut rl = DefaultEditor::new()?; // ← NESTED editor!
    let response = rl.readline("Execute? >>> ")?;
    response.trim() == "y"
    // Nested editor dropped, but signal handlers may conflict with main editor
}
```

**Why nesting matters:**
1. Main REPL editor registers SIGWINCH handler
2. Nested editor may register another handler or interfere with the first
3. When nested editor drops, signal handler state is inconsistent
4. Window resize fires handler with invalid/conflicting FD state
5. Panic in signal handler context → abort

## Workaround

Use standard `stdin` for simple prompts instead of rustyline:

```rust
// ✅ SAFE: No signal handlers
fn get_confirmation() -> bool {
    use std::io::{self, BufRead, Write};

    print!(">>> ");
    io::stdout().flush().unwrap();

    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let mut response = String::new();
    handle.read_line(&mut response).unwrap();

    response.trim() == "y"
}
```

Reserve rustyline for long-lived REPL sessions, not temporary prompts.

## Expected Behavior

One of:
1. Signal handler should be unregistered when `DefaultEditor` is dropped
2. Signal handler should safely handle the case where FD is invalid
3. Documentation should warn against creating temporary editors
4. Provide a signal-handler-free mode for simple use cases

## Environment

- **rustyline**: 14.0.0
- **OS**: macOS (also affects Linux)
- **Rust**: 1.83.0
- **tokio**: 1.48.0

## Related Issues

This affects any program that:
- Creates temporary `DefaultEditor` instances for prompts
- Runs in an async runtime (tokio, async-std, etc.)
- Can receive SIGWINCH during async execution
- Uses rustyline for non-REPL purposes

## Suggested Fix Locations

File: `src/tty/unix.rs:1197`
```rust
pub extern "C" fn sigwinch_handler(_: libc::c_int) {
    let fd = SIGWINCH_PIPE.load(Ordering::Relaxed);
    assert!(fd != -1); // ← This panics when handler is orphaned
    // ...
}
```

Potential fixes:
1. Check `fd != -1` and return early if invalid instead of asserting
2. Unregister handler in `Drop` implementation
3. Use thread-local or instance-specific handlers instead of global
