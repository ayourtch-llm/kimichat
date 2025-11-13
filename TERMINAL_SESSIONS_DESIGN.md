# Terminal Session Management - Design Document

## Overview

Add stateful PTY terminal session support to kimichat, allowing LLMs to launch, interact with, and monitor terminal sessions with full VT100/ANSI support.

## Use Cases

1. **Interactive Development**: LLM runs build tools, sees errors, iterates
2. **System Administration**: Monitor long-running processes, check logs
3. **Debugging**: Launch debuggers, inspect state, send commands
4. **Testing**: Run test suites, capture colored output
5. **User Collaboration**: Hand off terminal control to user when stuck

## Architecture

### Core Components

```
src/terminal/
├── mod.rs                  # Public API and exports
├── manager.rs              # TerminalManager singleton
├── session.rs              # TerminalSession implementation
├── pty_handler.rs          # PTY process management
├── screen_buffer.rs        # VT100 screen state
├── logger.rs               # PTY I/O logging
└── tools.rs                # LLM tool implementations

Global State:
- TerminalManager (Arc<Mutex<>>) stored in KimiChat struct
- Registered with tool_registry like PolicyManager
```

### Key Structures

```rust
/// Manages all terminal sessions
pub struct TerminalManager {
    sessions: HashMap<SessionId, Arc<Mutex<TerminalSession>>>,
    next_id: u32,
    log_dir: PathBuf,
}

/// Represents a single terminal session
pub struct TerminalSession {
    id: SessionId,
    pty: Box<dyn PtyMaster>,
    child: Box<dyn Child>,
    parser: vt100::Parser,
    screen_buffer: ScreenBuffer,
    capture_enabled: bool,
    capture_buffer: Vec<u8>,
    logger: SessionLogger,
    metadata: SessionMetadata,
}

/// Terminal screen state snapshot
pub struct ScreenBuffer {
    screen: vt100::Screen,
    cursor_pos: (u16, u16),
    size: (u16, u16),
}

/// Session metadata
pub struct SessionMetadata {
    id: SessionId,
    created_at: DateTime<Utc>,
    command: String,
    working_dir: PathBuf,
    status: SessionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SessionStatus {
    Running,
    Stopped,
    Exited(i32),
}

pub type SessionId = u32;
```

## Dependencies

```toml
[dependencies]
# Existing dependencies...

# Terminal/PTY support
portable-pty = "0.8"      # Cross-platform PTY
vt100 = "0.15"            # VT100/ANSI parser
```

## LLM Tools

### 1. `launch_terminal`

**Purpose**: Create new PTY session

**Parameters**:
```json
{
  "command": "string (optional, default: shell)",
  "working_dir": "string (optional, default: current)",
  "cols": "number (optional, default: 80)",
  "rows": "number (optional, default: 24)"
}
```

**Returns**:
```json
{
  "session_id": 1,
  "command": "bash",
  "working_dir": "/home/user/project",
  "size": [80, 24]
}
```

### 2. `send_keys`

**Purpose**: Send keystrokes to terminal

**Parameters**:
```json
{
  "session_id": 1,
  "keys": "string or array",
  "special": "boolean (optional, interpret escape sequences)"
}
```

**Special key sequences**:
- `\n` - Enter
- `\t` - Tab
- `^C` - Ctrl+C
- `^D` - Ctrl+D
- `[UP]`, `[DOWN]`, `[LEFT]`, `[RIGHT]` - Arrow keys
- `[HOME]`, `[END]`, `[PGUP]`, `[PGDN]`
- `[F1]` through `[F12]`

**Example**:
```json
{
  "session_id": 1,
  "keys": "ls -la\n",
  "special": true
}
```

### 3. `get_terminal_screen`

**Purpose**: Get current terminal screen contents

**Parameters**:
```json
{
  "session_id": 1,
  "include_colors": "boolean (optional, default: false)",
  "include_cursor": "boolean (optional, default: true)"
}
```

**Returns**:
```json
{
  "session_id": 1,
  "contents": "text representation of screen",
  "cursor_position": [10, 5],
  "size": [80, 24],
  "colors": "optional ANSI color codes"
}
```

### 4. `get_cursor_position`

**Purpose**: Get current cursor position

**Parameters**: `{ "session_id": 1 }`

**Returns**: `{ "session_id": 1, "position": [10, 5] }`

### 5. `set_terminal_size`

**Purpose**: Resize terminal

**Parameters**:
```json
{
  "session_id": 1,
  "cols": 100,
  "rows": 30
}
```

### 6. `start_capture` / `stop_capture`

**Purpose**: Control output buffering

**Parameters**: `{ "session_id": 1 }`

**Returns**: For `stop_capture`, returns captured output:
```json
{
  "session_id": 1,
  "captured": "output since start_capture",
  "bytes": 1024
}
```

### 7. `list_terminal_sessions`

**Purpose**: List all active sessions

**Parameters**: None

**Returns**:
```json
{
  "sessions": [
    {
      "id": 1,
      "command": "bash",
      "working_dir": "/home/user",
      "status": "running",
      "created_at": "2025-01-13T10:00:00Z",
      "size": [80, 24]
    }
  ]
}
```

### 8. `kill_terminal_session`

**Purpose**: Terminate session

**Parameters**:
```json
{
  "session_id": 1,
  "signal": "string (optional, default: SIGTERM)"
}
```

Signals: `SIGTERM`, `SIGKILL`, `SIGINT`, `SIGHUP`

### 9. `request_user_input`

**Purpose**: Hand off terminal to user

**Parameters**:
```json
{
  "session_id": 1,
  "message": "Please enter password",
  "timeout_seconds": 300
}
```

**Behavior**:
1. Display message to user
2. Show current terminal screen
3. Give user control (attach stdin/stdout)
4. Return when user signals completion (Ctrl+D) or timeout

## CLI Integration

### New Subcommands

```bash
# View terminal screen
kimichat terminal view <session-id>

# List sessions
kimichat terminal list

# Kill session
kimichat terminal kill <session-id>

# Attach to session (interactive)
kimichat terminal attach <session-id>

# Show session log
kimichat terminal log <session-id>

# Replay session from log
kimichat terminal replay <session-id>
```

## Logging

### Log Structure

```
logs/terminals/
├── session-1.log           # PTY I/O log
├── session-1-screen.log    # Screen state snapshots
└── session-1-meta.json     # Metadata
```

### Log Format (JSONL)

```json
{"timestamp": "2025-01-13T10:00:00Z", "session_id": 1, "direction": "in", "data": "ls -la\n"}
{"timestamp": "2025-01-13T10:00:01Z", "session_id": 1, "direction": "out", "data": "total 48\ndrwxr-xr-x..."}
{"timestamp": "2025-01-13T10:00:01Z", "session_id": 1, "event": "resize", "cols": 100, "rows": 30}
{"timestamp": "2025-01-13T10:00:02Z", "session_id": 1, "event": "screen_snapshot", "screen": "..."}
```

## Implementation Phases

### Phase 1: Core Infrastructure
- [ ] Add dependencies (portable-pty, vt100)
- [ ] Create `src/terminal/` module structure
- [ ] Implement `TerminalManager` with session storage
- [ ] Implement basic `TerminalSession` with PTY
- [ ] Add global TerminalManager to KimiChat

### Phase 2: PTY & Screen State
- [ ] Implement `PtyHandler` for process management
- [ ] Integrate `vt100::Parser` for ANSI sequences
- [ ] Implement `ScreenBuffer` for state tracking
- [ ] Add async PTY output reading
- [ ] Handle screen updates and buffering

### Phase 3: Basic Tools
- [ ] Implement `launch_terminal` tool
- [ ] Implement `send_keys` tool
- [ ] Implement `get_terminal_screen` tool
- [ ] Implement `list_terminal_sessions` tool
- [ ] Implement `kill_terminal_session` tool

### Phase 4: Advanced Features
- [ ] Implement `get_cursor_position` tool
- [ ] Implement `set_terminal_size` tool
- [ ] Implement `start_capture` / `stop_capture` tools
- [ ] Add special key handling (arrows, function keys, etc.)
- [ ] Add color extraction from screen buffer

### Phase 5: Logging & CLI
- [ ] Implement `SessionLogger` for I/O logging
- [ ] Add CLI subcommands (view, list, kill, attach)
- [ ] Implement log replay functionality
- [ ] Add screen snapshot logging

### Phase 6: User Interaction
- [ ] Implement `request_user_input` tool
- [ ] Add terminal attach/detach functionality
- [ ] Handle user control handoff
- [ ] Add timeout and cancellation support

### Phase 7: Polish & Testing
- [ ] Add confirmation policies for terminal operations
- [ ] Add resource limits (max sessions, buffer sizes)
- [ ] Comprehensive error handling
- [ ] Integration tests
- [ ] Documentation updates

## Design Considerations

### 1. State Management

**Problem**: Sessions persist across tool calls
**Solution**: Store `Arc<Mutex<TerminalManager>>` in `KimiChat`, similar to `PolicyManager`

### 2. Async PTY Output

**Problem**: PTY output is async and continuous
**Solution**: Spawn tokio task per session to read PTY output, update screen buffer

### 3. Large Output Handling

**Problem**: Terminal output can be very large
**Solution**:
- Limit screen buffer to visible area (configurable rows)
- Add scrollback buffer with size limit
- Allow LLM to request specific line ranges

### 4. Resource Cleanup

**Problem**: Leaked PTY processes
**Solution**:
- Track all sessions
- Auto-cleanup on KimiChat drop
- Add session timeout/idle detection
- Implement proper SIGTERM/SIGKILL handling

### 5. Security

**Problem**: Arbitrary command execution
**Solution**:
- Integrate with PolicyManager for confirmations
- Add `terminal.launch` policy type
- Require confirmation for `sudo`, `rm -rf`, etc.
- Sanitize special key input

### 6. Concurrent Access

**Problem**: Multiple tool calls accessing same session
**Solution**: `Arc<Mutex<TerminalSession>>` for thread-safe access

### 7. User Interaction Blocking

**Problem**: `request_user_input` blocks LLM
**Solution**:
- Set reasonable timeout (5 minutes)
- Allow async handling
- Clear UI indicators
- Ability to cancel and resume LLM

## Integration with Existing Code

### 1. KimiChat Structure

```rust
pub struct KimiChat {
    // Existing fields...
    pub(crate) terminal_manager: Arc<Mutex<TerminalManager>>,
}
```

### 2. Tool Registration

```rust
// In src/config/mod.rs initialize_tool_registry()
registry.register_with_categories(
    LaunchTerminalTool,
    vec!["terminal".to_string()]
);
registry.register_with_categories(
    SendKeysTool,
    vec!["terminal".to_string()]
);
// ... etc
```

### 3. Policy Integration

```toml
# policies.toml
[terminal]
launch = "confirm"  # Require confirmation to launch
send_keys = "allow"  # Allow once launched
kill = "confirm"     # Require confirmation to kill
```

### 4. Agent Configurations

```json
// agents/configs/system_operator.json
{
  "tools": [
    "launch_terminal",
    "send_keys",
    "get_terminal_screen",
    "list_terminal_sessions",
    "kill_terminal_session"
  ]
}
```

## API Examples

### Example 1: Run a Build

```rust
// LLM Tool Calls:

// 1. Launch terminal
launch_terminal({
  "command": "bash",
  "working_dir": "/home/user/project"
})
// Returns: { "session_id": 1 }

// 2. Run build
send_keys({
  "session_id": 1,
  "keys": "cargo build\n",
  "special": true
})

// 3. Wait a bit, then check output
get_terminal_screen({
  "session_id": 1,
  "include_colors": false
})
// Returns screen contents with any errors

// 4. If errors, iterate...
send_keys({
  "session_id": 1,
  "keys": "^C",  // Ctrl+C to cancel
  "special": true
})

// 5. Clean up
kill_terminal_session({ "session_id": 1 })
```

### Example 2: Interactive Debugging

```rust
// 1. Launch debugger
launch_terminal({
  "command": "gdb ./myprogram",
  "working_dir": "/home/user/project"
})

// 2. Set breakpoint
send_keys({
  "session_id": 1,
  "keys": "break main\n",
  "special": true
})

// 3. Run
send_keys({
  "session_id": 1,
  "keys": "run\n",
  "special": true
})

// 4. Check state
get_terminal_screen({ "session_id": 1 })

// 5. LLM can't figure it out, ask user
request_user_input({
  "session_id": 1,
  "message": "I've set a breakpoint at main() and the program is paused. Please inspect the variables and continue when ready.",
  "timeout_seconds": 300
})

// User interacts, then continues LLM
```

## Open Questions

1. **Scrollback**: How much history to keep? Default 1000 lines?
2. **Performance**: Benchmark vt100 parser with large outputs
3. **Windows Support**: portable-pty claims cross-platform, verify
4. **Binary Output**: How to handle non-text output (images, etc.)?
5. **Multi-user**: Should sessions be user-scoped or global?
6. **Persistence**: Should sessions persist across kimichat restarts?
7. **Copy-on-write**: Should screen snapshots use COW for efficiency?

## Testing Strategy

1. **Unit Tests**: Each component in isolation
2. **Integration Tests**: Full tool workflows
3. **Manual Tests**:
   - vim interaction
   - htop monitoring
   - Build tools (cargo, make, npm)
   - Interactive programs (less, python REPL)
4. **Stress Tests**: Many sessions, large outputs
5. **Security Tests**: Command injection attempts

## Documentation Updates

- [ ] Update CLAUDE.md with terminal session concepts
- [ ] Add terminal tools to system prompt
- [ ] Create user guide with examples
- [ ] Add troubleshooting section

## Future Enhancements (Post-MVP)

1. **Session Persistence**: Save/restore sessions across restarts
2. **Screen Recording**: Export session as video/gif
3. **Collaborative Sessions**: Multiple LLMs/users in same session
4. **Terminal Multiplexing**: Built-in tmux-like functionality
5. **Remote Sessions**: SSH into remote machines
6. **File Transfer**: Upload/download files via terminal
7. **Clipboard Integration**: Copy terminal contents
8. **Search**: Search terminal history and output

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| PTY leaks | High | Proper cleanup, session tracking, timeouts |
| Large memory usage | Medium | Buffer limits, scrollback limits |
| Command injection | High | Policy system, input sanitization |
| Performance degradation | Medium | Async I/O, efficient parsing, benchmarks |
| Platform compatibility | Low | Use portable-pty, test on Linux/Mac/Windows |
| User experience | Medium | Clear indicators, good error messages |

## Success Criteria

- [ ] Can launch terminal and run commands
- [ ] Can read terminal screen state accurately
- [ ] Handles colors and special characters
- [ ] Supports interactive programs (vim, less)
- [ ] Logs all activity for debugging
- [ ] User can attach to sessions
- [ ] Proper resource cleanup
- [ ] Zero security vulnerabilities
- [ ] Performance < 100ms per tool call
- [ ] Comprehensive test coverage (>80%)
