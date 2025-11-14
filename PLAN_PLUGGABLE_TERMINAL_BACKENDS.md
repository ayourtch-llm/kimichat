# Plan: Pluggable Terminal Backend Architecture

## Goal

Make the `pty_*` tools implementation pluggable to support multiple terminal session backends:
1. **Internal PTY** (current implementation) - Direct PTY process management
2. **Tmux** (new implementation) - Use tmux as session multiplexer

**Key Constraint:** Operations must be identical from LLM perspective - same tools, same parameters, same semantics.

---

## Current Architecture Analysis

### Existing Structure

```
src/terminal/
├── session.rs          # PTY session with background reader
├── manager.rs          # Multi-session management
├── pty_handler.rs      # PTY process and I/O
├── screen_buffer.rs    # VT100 terminal emulation
├── tools.rs            # 11 PTY tool implementations
└── logger.rs           # Session logging
```

### Current Flow

```
Tool Call (pty_launch)
    ↓
TerminalManager::create_session()
    ↓
Session::new() → spawns PTY process
    ↓
Background reader thread → updates ScreenBuffer
    ↓
Tool Call (pty_get_screen) → reads from ScreenBuffer
```

### 11 PTY Tools

| Tool | Current Implementation |
|------|----------------------|
| `pty_launch` | Spawns PTY process, starts background reader |
| `pty_send_keys` | Writes to PTY master fd |
| `pty_get_screen` | Reads from VT100 screen buffer |
| `pty_list_sessions` | Returns HashMap keys |
| `pty_kill_session` | Kills PTY process, stops reader |
| `pty_get_cursor_position` | Reads cursor from screen buffer |
| `pty_resize` | Sends TIOCSWINSZ ioctl to PTY |
| `pty_get_scrollback` | Reads scrollback buffer |
| `pty_capture_start` | Starts logging to file |
| `pty_capture_stop` | Stops logging |
| `pty_request_input` | Placeholder for interactive input |

---

## Proposed Architecture

### Phase 1: Define Backend Abstraction

#### 1.1 Create `TerminalBackend` Trait

**File:** `src/terminal/backend.rs`

```rust
use anyhow::Result;

/// Terminal session metadata
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub command: String,
    pub created_at: std::time::SystemTime,
    pub rows: u16,
    pub cols: u16,
}

/// Cursor position in terminal
#[derive(Debug, Clone)]
pub struct CursorPosition {
    pub row: usize,
    pub col: usize,
}

/// Terminal backend trait - abstraction over PTY and tmux
#[async_trait::async_trait]
pub trait TerminalBackend: Send + Sync {
    /// Launch a new terminal session
    /// Returns session ID
    async fn launch_session(
        &mut self,
        id: String,
        command: String,
        rows: u16,
        cols: u16,
        working_dir: Option<String>,
    ) -> Result<String>;

    /// Send input to a session
    /// For commands, caller must append \n explicitly
    async fn send_keys(
        &mut self,
        session_id: &str,
        keys: &str,
    ) -> Result<()>;

    /// Get current screen content
    /// Returns visible screen area (rows x cols)
    async fn get_screen(
        &self,
        session_id: &str,
        include_colors: bool,
    ) -> Result<String>;

    /// List all active sessions
    async fn list_sessions(&self) -> Result<Vec<SessionInfo>>;

    /// Kill a session
    async fn kill_session(&mut self, session_id: &str) -> Result<()>;

    /// Get cursor position
    async fn get_cursor_position(&self, session_id: &str) -> Result<CursorPosition>;

    /// Resize session
    async fn resize_session(
        &mut self,
        session_id: &str,
        rows: u16,
        cols: u16,
    ) -> Result<()>;

    /// Get scrollback buffer (if supported)
    /// Returns historical output beyond current screen
    async fn get_scrollback(
        &self,
        session_id: &str,
        lines: usize,
    ) -> Result<Option<String>>;

    /// Start capturing session output to file
    async fn capture_start(
        &mut self,
        session_id: &str,
        output_file: String,
    ) -> Result<()>;

    /// Stop capturing session output
    async fn capture_stop(&mut self, session_id: &str) -> Result<()>;

    /// Check if session exists
    async fn session_exists(&self, session_id: &str) -> bool;

    /// Get backend name for debugging
    fn backend_name(&self) -> &str;
}
```

#### 1.2 Create Backend Enum

**File:** `src/terminal/backend.rs`

```rust
/// Configuration for which backend to use
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalBackendType {
    /// Internal PTY implementation (default)
    Pty,
    /// Tmux-based implementation
    Tmux,
}

impl Default for TerminalBackendType {
    fn default() -> Self {
        Self::Pty
    }
}

impl std::str::FromStr for TerminalBackendType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "pty" | "internal" => Ok(Self::Pty),
            "tmux" => Ok(Self::Tmux),
            _ => Err(anyhow::anyhow!("Invalid backend type: {}", s)),
        }
    }
}
```

---

### Phase 2: Refactor Current PTY Implementation

#### 2.1 Create `PtyBackend` Struct

**File:** `src/terminal/pty_backend.rs`

Wrap existing implementation in a struct that implements `TerminalBackend`:

```rust
use super::backend::{TerminalBackend, SessionInfo, CursorPosition};
use super::{Session, ScreenBuffer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// PTY-based terminal backend (current implementation)
pub struct PtyBackend {
    sessions: HashMap<String, Arc<Mutex<Session>>>,
    // Move session management logic here from TerminalManager
}

#[async_trait::async_trait]
impl TerminalBackend for PtyBackend {
    async fn launch_session(
        &mut self,
        id: String,
        command: String,
        rows: u16,
        cols: u16,
        working_dir: Option<String>,
    ) -> Result<String> {
        // Use existing Session::new() logic
        let session = Session::new(&id, &command, rows, cols)?;
        self.sessions.insert(id.clone(), Arc::new(Mutex::new(session)));
        Ok(id)
    }

    async fn send_keys(&mut self, session_id: &str, keys: &str) -> Result<()> {
        // Use existing Session::send_input() logic
        // ...
    }

    async fn get_screen(&self, session_id: &str, include_colors: bool) -> Result<String> {
        // Use existing ScreenBuffer::get_contents() logic
        // ...
    }

    // ... implement other methods ...

    fn backend_name(&self) -> &str {
        "pty"
    }
}
```

**Key Changes:**
- Minimal refactoring of existing code
- Keep all PTY logic: `pty_handler`, `screen_buffer`, background reader
- Just wrap it in the trait implementation
- Move session HashMap from `TerminalManager` to `PtyBackend`

---

### Phase 3: Implement Tmux Backend

#### 3.1 Create `TmuxBackend` Struct

**File:** `src/terminal/tmux_backend.rs`

```rust
use super::backend::{TerminalBackend, SessionInfo, CursorPosition};
use std::collections::HashMap;
use tokio::process::Command;

/// Tmux-based terminal backend
pub struct TmuxBackend {
    sessions: HashMap<String, TmuxSessionInfo>,
    capture_files: HashMap<String, String>,
}

struct TmuxSessionInfo {
    id: String,
    tmux_session_name: String,
    command: String,
    created_at: std::time::SystemTime,
    rows: u16,
    cols: u16,
}

impl TmuxBackend {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            capture_files: HashMap::new(),
        }
    }

    /// Run tmux command and get output
    async fn run_tmux(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("tmux")
            .args(args)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("tmux command failed: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[async_trait::async_trait]
impl TerminalBackend for TmuxBackend {
    async fn launch_session(
        &mut self,
        id: String,
        command: String,
        rows: u16,
        cols: u16,
        working_dir: Option<String>,
    ) -> Result<String> {
        // Generate unique tmux session name
        let tmux_name = format!("kimichat_{}", id);

        // Create tmux session with command
        let mut args = vec![
            "new-session",
            "-d",  // detached
            "-s", &tmux_name,  // session name
            "-x", &cols.to_string(),  // width
            "-y", &rows.to_string(),  // height
        ];

        if let Some(dir) = &working_dir {
            args.extend(&["-c", dir]);
        }

        args.push(&command);

        self.run_tmux(&args).await?;

        // Store session info
        self.sessions.insert(id.clone(), TmuxSessionInfo {
            id: id.clone(),
            tmux_session_name: tmux_name,
            command,
            created_at: std::time::SystemTime::now(),
            rows,
            cols,
        });

        Ok(id)
    }

    async fn send_keys(&mut self, session_id: &str, keys: &str) -> Result<()> {
        let info = self.sessions.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Send keys to tmux session
        // Note: tmux send-keys automatically handles special keys
        self.run_tmux(&[
            "send-keys",
            "-t", &info.tmux_session_name,
            keys,
        ]).await?;

        Ok(())
    }

    async fn get_screen(&self, session_id: &str, include_colors: bool) -> Result<String> {
        let info = self.sessions.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Capture pane content
        // -p: print to stdout
        // -e: include escape sequences (for colors)
        let mut args = vec![
            "capture-pane",
            "-t", &info.tmux_session_name,
            "-p",  // print to stdout
        ];

        if include_colors {
            args.push("-e");  // include ANSI escape sequences
        }

        let output = self.run_tmux(&args).await?;
        Ok(output)
    }

    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        Ok(self.sessions.values().map(|info| SessionInfo {
            id: info.id.clone(),
            command: info.command.clone(),
            created_at: info.created_at,
            rows: info.rows,
            cols: info.cols,
        }).collect())
    }

    async fn kill_session(&mut self, session_id: &str) -> Result<()> {
        let info = self.sessions.remove(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Kill tmux session
        self.run_tmux(&[
            "kill-session",
            "-t", &info.tmux_session_name,
        ]).await?;

        // Remove capture file if exists
        self.capture_files.remove(session_id);

        Ok(())
    }

    async fn get_cursor_position(&self, session_id: &str) -> Result<CursorPosition> {
        let info = self.sessions.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Get cursor position from tmux
        // tmux display-message -t session -p "#{cursor_x},#{cursor_y}"
        let output = self.run_tmux(&[
            "display-message",
            "-t", &info.tmux_session_name,
            "-p", "#{cursor_y},#{cursor_x}",
        ]).await?;

        let parts: Vec<&str> = output.trim().split(',').collect();
        if parts.len() != 2 {
            anyhow::bail!("Unexpected cursor position format: {}", output);
        }

        Ok(CursorPosition {
            row: parts[0].parse()?,
            col: parts[1].parse()?,
        })
    }

    async fn resize_session(&mut self, session_id: &str, rows: u16, cols: u16) -> Result<()> {
        let info = self.sessions.get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Resize tmux window
        self.run_tmux(&[
            "resize-window",
            "-t", &info.tmux_session_name,
            "-x", &cols.to_string(),
            "-y", &rows.to_string(),
        ]).await?;

        info.rows = rows;
        info.cols = cols;

        Ok(())
    }

    async fn get_scrollback(&self, session_id: &str, lines: usize) -> Result<Option<String>> {
        let info = self.sessions.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Capture scrollback from tmux
        // -S: start line (negative = from scrollback)
        let output = self.run_tmux(&[
            "capture-pane",
            "-t", &info.tmux_session_name,
            "-p",
            "-S", &format!("-{}", lines),
        ]).await?;

        Ok(Some(output))
    }

    async fn capture_start(&mut self, session_id: &str, output_file: String) -> Result<()> {
        let info = self.sessions.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Start tmux pipe-pane to capture output
        self.run_tmux(&[
            "pipe-pane",
            "-t", &info.tmux_session_name,
            "-o",  // open mode (append)
            &format!("cat >> {}", output_file),
        ]).await?;

        self.capture_files.insert(session_id.to_string(), output_file);

        Ok(())
    }

    async fn capture_stop(&mut self, session_id: &str) -> Result<()> {
        let info = self.sessions.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Stop tmux pipe-pane
        self.run_tmux(&[
            "pipe-pane",
            "-t", &info.tmux_session_name,
        ]).await?;

        self.capture_files.remove(session_id);

        Ok(())
    }

    async fn session_exists(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    fn backend_name(&self) -> &str {
        "tmux"
    }
}
```

**Tmux Command Reference:**
- `tmux new-session -d -s name -x cols -y rows command` - Create session
- `tmux send-keys -t session text` - Send input
- `tmux capture-pane -t session -p` - Get screen content
- `tmux display-message -t session -p format` - Query session info
- `tmux kill-session -t session` - Kill session
- `tmux resize-window -t session -x cols -y rows` - Resize
- `tmux pipe-pane -t session -o "command"` - Capture output

---

### Phase 4: Configuration System

#### 4.1 Add Configuration Option

**File:** `src/config/mod.rs`

```rust
pub struct ClientConfig {
    // ... existing fields ...

    /// Terminal backend to use (pty or tmux)
    pub terminal_backend: TerminalBackendType,
}
```

#### 4.2 CLI Argument

**File:** `src/main.rs` (clap args)

```rust
#[derive(Parser)]
struct Args {
    // ... existing args ...

    /// Terminal backend to use for pty_* tools
    #[arg(long, default_value = "pty")]
    terminal_backend: String,
}
```

#### 4.3 Environment Variable

Support `KIMICHAT_TERMINAL_BACKEND=tmux` environment variable.

---

### Phase 5: Update TerminalManager

#### 5.1 Make TerminalManager Backend-Agnostic

**File:** `src/terminal/manager.rs`

```rust
use super::backend::{TerminalBackend, TerminalBackendType};
use super::pty_backend::PtyBackend;
use super::tmux_backend::TmuxBackend;

pub struct TerminalManager {
    backend: Box<dyn TerminalBackend>,
}

impl TerminalManager {
    pub fn new(backend_type: TerminalBackendType) -> Self {
        let backend: Box<dyn TerminalBackend> = match backend_type {
            TerminalBackendType::Pty => Box::new(PtyBackend::new()),
            TerminalBackendType::Tmux => Box::new(TmuxBackend::new()),
        };

        Self { backend }
    }

    // Delegate all methods to backend
    pub async fn create_session(
        &mut self,
        id: String,
        command: String,
        rows: u16,
        cols: u16,
    ) -> Result<String> {
        self.backend.launch_session(id, command, rows, cols, None).await
    }

    pub async fn send_input(&mut self, session_id: &str, input: &str) -> Result<()> {
        self.backend.send_keys(session_id, input).await
    }

    // ... delegate other methods ...
}
```

---

### Phase 6: Update Tools

#### 6.1 Update Tool Implementations

**File:** `src/terminal/tools.rs`

Tools already use `TerminalManager` - no changes needed if manager delegates correctly!

```rust
// Example: pty_launch tool (no changes needed)
async fn execute(&self, params: ToolParameters, context: &ToolContext) -> ToolResult {
    // ... parse parameters ...

    let mut manager = context.terminal_manager.lock().unwrap();
    let session_id = manager.create_session(id, command, rows, cols).await?;

    // Works with both backends!
}
```

The tools are already backend-agnostic because they use the `TerminalManager` interface.

---

### Phase 7: Testing Strategy

#### 7.1 Unit Tests

**File:** `src/terminal/backend.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pty_backend_basic() {
        let mut backend = PtyBackend::new();
        let id = backend.launch_session(
            "test".to_string(),
            "echo hello".to_string(),
            24, 80, None
        ).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let screen = backend.get_screen(&id, false).await.unwrap();
        assert!(screen.contains("hello"));
    }

    #[tokio::test]
    async fn test_tmux_backend_basic() {
        // Skip if tmux not available
        if Command::new("tmux").arg("-V").output().await.is_err() {
            return;
        }

        let mut backend = TmuxBackend::new();
        let id = backend.launch_session(
            "test".to_string(),
            "echo hello".to_string(),
            24, 80, None
        ).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let screen = backend.get_screen(&id, false).await.unwrap();
        assert!(screen.contains("hello"));
    }
}
```

#### 7.2 Integration Tests

Test all 11 tools with both backends:

```bash
# Test PTY backend
cargo run -- --terminal-backend pty -i
# Run tool tests

# Test Tmux backend
cargo run -- --terminal-backend tmux -i
# Run same tool tests
```

#### 7.3 Compatibility Matrix

| Feature | PTY Backend | Tmux Backend | Notes |
|---------|-------------|--------------|-------|
| Launch session | ✅ | ✅ | Both support |
| Send keys | ✅ | ✅ | Both support |
| Get screen | ✅ | ✅ | Both support |
| Cursor position | ✅ | ✅ | Both support |
| Resize | ✅ | ✅ | Both support |
| Scrollback | ✅ | ✅ | Both support |
| Capture to file | ✅ | ✅ | Both support |
| VT100 emulation | ✅ | ⚠️ | Tmux has own terminal emulation |
| Background reader | ✅ | N/A | Tmux handles buffering |
| Color support | ✅ | ✅ | Both support ANSI colors |

---

## Implementation Phases

### Phase 1: Foundation (Week 1)
- [ ] Create `src/terminal/backend.rs` with `TerminalBackend` trait
- [ ] Create `TerminalBackendType` enum
- [ ] Add configuration fields to `ClientConfig`
- [ ] Add CLI argument parsing

### Phase 2: PTY Refactor (Week 1)
- [ ] Create `src/terminal/pty_backend.rs`
- [ ] Move session management from `TerminalManager` to `PtyBackend`
- [ ] Implement `TerminalBackend` for `PtyBackend`
- [ ] Update `TerminalManager` to delegate to backend
- [ ] Test existing functionality still works

### Phase 3: Tmux Implementation (Week 2)
- [ ] Create `src/terminal/tmux_backend.rs`
- [ ] Implement `TerminalBackend` for `TmuxBackend`
- [ ] Test each method individually with tmux commands
- [ ] Handle edge cases (tmux not installed, session naming conflicts)

### Phase 4: Integration (Week 2)
- [ ] Update `TerminalManager::new()` to accept backend type
- [ ] Thread configuration through application startup
- [ ] Verify tools work with both backends
- [ ] Add backend name to tool output for debugging

### Phase 5: Testing & Documentation (Week 3)
- [ ] Write unit tests for both backends
- [ ] Write integration tests
- [ ] Add backend selection to CUSTOMIZING_AGENTS_AND_SKILLS.md
- [ ] Update CLAUDE.md with backend architecture
- [ ] Create troubleshooting guide

---

## Migration Path

### Backward Compatibility

**Default Behavior:**
- Default to PTY backend (no breaking changes)
- Existing configurations continue to work
- No changes to tool interface

**Opt-In:**
```bash
# Use tmux backend
cargo run -- --terminal-backend tmux -i

# Or via environment
export KIMICHAT_TERMINAL_BACKEND=tmux
cargo run -- -i
```

### Deprecation Strategy

No deprecation needed - PTY remains fully supported as default.

---

## Edge Cases & Considerations

### 1. Tmux Not Installed

**Problem:** User selects tmux backend but tmux isn't available.

**Solution:**
```rust
impl TmuxBackend {
    pub fn new() -> Result<Self> {
        // Check if tmux is available
        let output = std::process::Command::new("tmux")
            .arg("-V")
            .output()?;

        if !output.status.success() {
            anyhow::bail!("tmux not found. Install tmux or use --terminal-backend pty");
        }

        Ok(Self {
            sessions: HashMap::new(),
            capture_files: HashMap::new(),
        })
    }
}
```

### 2. Session Name Conflicts

**Problem:** Multiple KimiChat instances using tmux with same session names.

**Solution:**
- Include process ID in tmux session names: `kimichat_{pid}_{session_id}`
- Or use `tmux new-session -P` to get unique name from tmux

### 3. Tmux Server State

**Problem:** Tmux sessions persist after KimiChat exits.

**Solution:**
- Option 1: Clean up on exit (kill all kimichat_* sessions)
- Option 2: Allow reattachment in future runs (session persistence feature)
- Document behavior clearly

### 4. Color/ANSI Escape Sequences

**Problem:** PTY uses VT100 emulation, tmux has own terminal type.

**Solution:**
- Both support ANSI escape sequences
- Use `tmux capture-pane -e` to preserve escapes
- Document that rendering may differ slightly

### 5. Performance

**Problem:** Tmux adds process overhead (spawning tmux commands).

**Solution:**
- Cache tmux session info to minimize commands
- Use tmux command batching where possible
- Benchmark both backends and document performance characteristics

---

## Benefits of This Approach

### For Users

1. **Choice:** Pick backend based on needs
   - PTY: Maximum performance, no dependencies
   - Tmux: Session persistence, familiar tool

2. **Flexibility:** Can switch backends per-project

3. **Debugging:** Can use tmux attach to inspect sessions manually

### For Development

1. **Separation of Concerns:** Backend logic isolated from tool logic

2. **Testability:** Easy to mock backends for testing

3. **Extensibility:** Can add more backends later:
   - Screen backend
   - SSH backend (remote execution)
   - Docker backend (containerized sessions)

4. **Maintainability:** Changes to one backend don't affect others

---

## Future Extensions

### Possible Additional Backends

1. **Screen Backend:** Similar to tmux but uses GNU screen
2. **SSH Backend:** Run sessions on remote machines
3. **Docker Backend:** Run sessions in containers
4. **Kubernetes Backend:** Run sessions in pods

### Configuration Enhancements

```rust
// Per-session backend selection
pub struct SessionConfig {
    pub backend: Option<TerminalBackendType>,
    pub backend_options: HashMap<String, String>,
}

// Allow backend-specific options
let options = HashMap::from([
    ("tmux.socket_name", "/tmp/custom.sock"),
    ("pty.pty_type", "unix98"),
]);
```

---

## Success Criteria

✅ **LLM Perspective:** All 11 tools work identically with both backends
✅ **Configuration:** Users can choose backend via CLI or environment
✅ **Default Behavior:** PTY backend is default (backward compatible)
✅ **Testing:** Both backends pass same test suite
✅ **Documentation:** Clear guide on choosing and using backends
✅ **Error Handling:** Graceful failures with helpful error messages

---

## Questions to Resolve

1. **Session Persistence:** Should tmux sessions persist after KimiChat exit?
   - Pro: Can reattach later, debug issues
   - Con: Orphaned sessions if not cleaned up

2. **Backend Per-Session vs Global:** Should we support mixing backends?
   - Global: Simpler, consistent behavior
   - Per-session: More flexible but complex

3. **Tmux Configuration:** Should we respect user's `.tmux.conf`?
   - Respect: User-friendly, familiar
   - Override: Predictable, testable

4. **Performance Priority:** Optimize for latency or feature parity?
   - Latency: Minimize tmux command calls (cache aggressively)
   - Features: Match PTY capabilities exactly (more commands)

---

## Appendix: Command Mappings

### PTY Backend → Tmux Commands

| Operation | PTY Implementation | Tmux Command |
|-----------|-------------------|--------------|
| Launch | `spawn_pty()` | `tmux new-session -d` |
| Send input | `write(master_fd)` | `tmux send-keys` |
| Read screen | `screen_buffer.get_contents()` | `tmux capture-pane -p` |
| Get cursor | `screen_buffer.cursor_pos()` | `tmux display-message -p "#{cursor_y},#{cursor_x}"` |
| Resize | `ioctl(TIOCSWINSZ)` | `tmux resize-window` |
| Kill | `kill(pid)` | `tmux kill-session` |
| Capture | Write to file in reader | `tmux pipe-pane` |

### Tmux Format Strings

```bash
# Get session info
tmux list-sessions -F "#{session_id}:#{session_name}:#{session_created}"

# Get window info
tmux list-windows -F "#{window_id}:#{window_width}:#{window_height}"

# Get pane info
tmux list-panes -F "#{pane_id}:#{cursor_x}:#{cursor_y}"

# Display custom message
tmux display-message -p "#{session_name} is #{session_width}x#{session_height}"
```

---

## Next Steps

1. **Review this plan** with stakeholders
2. **Prototype** `TerminalBackend` trait and basic tmux commands
3. **Validate** tmux commands work as expected in various scenarios
4. **Implement** in phases as outlined above
5. **Test** thoroughly with both backends
6. **Document** usage and troubleshooting
7. **Release** as opt-in feature with PTY as default
