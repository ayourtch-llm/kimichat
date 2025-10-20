# JSONL Viewer TUI

A terminal-based JSONL (JSON Lines) viewer built with Rust and Ratatui.

## Project Overview

This is a TUI application that reads JSONL log files and provides an interactive interface for browsing and inspecting them. The application can load JSONL files, parse each line as JSON, and display them in a two-panel interface with navigation and filtering capabilities.

## What This Project Does

This is a terminal-based JSONL viewer that:

1. **Reads JSONL files** - Loads and parses JSON Lines files from command line arguments
2. **Interactive browsing** - Two-panel TUI with keyboard navigation
3. **JSON validation** - Shows valid JSON in green, invalid JSON in red
4. **Filtering** - Toggle between showing all entries or only invalid JSON
5. **Pretty printing** - Formats valid JSON with proper indentation
6. **Keyboard controls** - j/k or arrow keys for navigation, d/u for scrolling, i for toggle invalid, q for quit

The application gracefully handles malformed JSON, empty files, and edge cases while maintaining terminal state cleanup on exit.

### Core Functionality

1. **JSONL Loading**
   - Read a JSONL file specified as command-line argument
   - Parse each line as JSON
   - Track which lines are valid/invalid JSON
   - Handle malformed JSON gracefully

2. **Two-Panel Layout**
   - Left panel (30% width): List of log entries
   - Right panel (70% width): Detailed view of selected entry

3. **Entry List (Left Panel)**
   - Show one line per entry
   - Display preview (first ~50 chars of the JSON line)
   - Color coding:
     - Green: Valid JSON
     - Red: Invalid JSON
   - Highlight selected entry with ">" prefix
   - Auto-scroll to keep selection visible

4. **Detail View (Right Panel)**
   - Show full content of selected entry
   - Pretty-print valid JSON with indentation
   - Show raw text for invalid JSON
   - Support scrolling for long content

5. **Keyboard Controls**
   - `j` or Down Arrow: Select next entry
   - `k` or Up Arrow: Select previous entry
   - `d` or Page Down: Scroll content down
   - `u` or Page Up: Scroll content up
   - `i`: Toggle "show only invalid entries" mode
   - `q` or Esc: Quit application

### Technical Requirements

1. **Dependencies (Cargo.toml)**
   ```toml
   [dependencies]
   ratatui = "0.24"
   crossterm = "0.27"
   serde = { version = "1.0", features = ["derive"] }
   serde_json = "1.0"
   anyhow = "1.0"
   ```

2. **Terminal Setup**
   - Use crossterm for terminal manipulation
   - Enable raw mode
   - Use CrosstermBackend with ratatui
   - Clean up terminal state on exit (restore cursor, disable raw mode)

3. **Error Handling**
   - Use `anyhow::Result` for error handling
   - Gracefully handle file not found
   - Handle empty files
   - Continue processing if some JSON lines are invalid

## Implementation Structure

### Data Structures

```rust
struct JsonlEntry {
    content: String,           // Raw line content
    parsed: Option<Value>,     // Parsed JSON (None if invalid)
    valid: bool,              // Is valid JSON?
}

struct App {
    entries: Vec<JsonlEntry>,  // All entries
    selected_index: usize,     // Currently selected entry
    scroll_offset: usize,      // Vertical scroll in detail view
    show_only_invalid: bool,   // Filter mode
}
```

### Main Components

1. **main()**: Setup terminal, run event loop, cleanup
2. **App::load_jsonl()**: Read and parse JSONL file
3. **draw()**: Render the UI (both panels)
4. **handle_input()**: Process keyboard events
5. **App navigation methods**: next_entry(), previous_entry(), scroll_down(), scroll_up()

## UI Layout

```
┌─────────────────────────────────────────────────────────────┐
│                     JSONL Viewer                            │
├──────────────────────┬──────────────────────────────────────┤
│ Entries              │ Content                              │
│                      │                                      │
│ > {"timestamp": "... │ {                                    │
│   {"timestamp": "... │   "timestamp": "2024-10-20...",     │
│   {"timestamp": "... │   "role": "user",                   │
│   {"timestamp": "... │   "content": "Hello world",         │
│   Invalid JSON line  │   "model": null                     │
│   {"timestamp": "... │ }                                    │
│   ...                │                                      │
│                      │                                      │
│                      │                                      │
│                      │                                      │
└──────────────────────┴──────────────────────────────────────┘
Keys: j/k=navigate, d/u=scroll, i=toggle invalid, q=quit
```

## Usage

```bash
cargo build --release
cargo run -- <jsonl_file>
```

Example:
```bash
cargo run -- ../../logs/kchat-2024-10-20-072629.jsonl
```

## Technical Implementation

The application is built with:
- **Rust** with ratatui for terminal UI
- **crossterm** for keyboard input and terminal manipulation  
- **serde_json** for JSON parsing and formatting
- **anyhow** for error handling

Key components:
- `JsonlEntry` struct for storing parsed entries
- `App` struct for application state
- `draw_ui()` for rendering the two-panel interface
- Keyboard event handling with navigation and filtering

1. **Setup Project**
   - Create new Cargo project if not exists
   - Add dependencies to Cargo.toml

2. **Basic Structure**
   - Define data structures (JsonlEntry, App)
   - Implement file loading and JSON parsing
   - Add basic error handling

3. **Terminal Setup**
   - Initialize crossterm terminal
   - Setup ratatui with CrosstermBackend
   - Implement proper cleanup

4. **UI Rendering**
   - Create two-column layout
   - Render entry list with colors
   - Render detail view with pretty JSON
   - Add borders and titles

5. **Event Handling**
   - Poll for keyboard events
   - Implement navigation (j/k)
   - Implement scrolling (d/u)
   - Implement toggle filter (i)
   - Implement quit (q)

6. **Polish**
   - Handle edge cases (empty list, single entry)
   - Ensure selection wraps at boundaries
   - Add status line with key hints
   - Test with actual log files from ../../logs/

## Example Usage

```bash
cargo build --release
cargo run -- ../../logs/kchat-2025-10-20-072629.jsonl
```

## Testing

Test with the actual log files in `../../logs/`:
- Files with valid JSON on every line
- Files with some invalid JSON lines
- Large files (check performance)
- Empty files

## Success Criteria

- [x] Application compiles without errors
- [x] Can load and display JSONL files from ../../logs/
- [x] Both panels render correctly
- [x] All keyboard controls work as specified
- [x] Valid/invalid JSON entries are colored correctly
- [x] Pretty-printing works for valid JSON
- [x] Can scroll through long content
- [x] Filter mode (toggle invalid) works
- [x] Terminal state is properly restored on exit
- [x] No panics on edge cases (empty file, invalid input, etc.)

## Technical Implementation

The application is built with:
- **Rust** with ratatui for terminal UI
- **crossterm** for keyboard input and terminal manipulation  
- **serde_json** for JSON parsing and formatting
- **anyhow** for error handling

Key components:
- `JsonlEntry` struct for storing parsed entries
- `App` struct for application state
- `draw_ui()` for rendering the two-panel interface
- Keyboard event handling with navigation and filtering

