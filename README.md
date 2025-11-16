# Kimi Chat - AI-powered Development Assistant

A sophisticated Rust-based AI development assistant that provides a Claude Code-like experience with multi-model support, rich tool integration, web API, and terminal management capabilities. Built for developers who need powerful AI assistance with fine-grained control and extensibility.

## Overview

Kimi Chat is a production-ready AI assistant that seamlessly integrates multiple LLM providers (Groq, Anthropic Claude, OpenAI, and llama.cpp) with a comprehensive toolset for file operations, terminal interaction, task management, and multi-agent workflows. Whether you're working in CLI mode, task mode, or through a web interface, Kimi Chat adapts to your development workflow.

## Key Features

### 🤖 Multi-Model & Multi-Provider Support
- **Four LLM Providers**: Groq, Anthropic Claude, OpenAI, and llama.cpp (local inference)
- **Flexible Model Slots**: BluModel, GrnModel, RedModel, and custom model support
- **Intelligent Model Switching**: Models can autonomously switch based on task requirements
- **Streaming Support**: Real-time response streaming for all providers
- **Automatic Backend Detection**: Smart provider selection from API URLs

### 🛠️ Comprehensive Tool System (20+ Tools)

#### File Operations
- **open_file** - Display file contents with optional line ranges
- **read_file** - Quick file preview (first 10 lines)
- **write_file** - Create and write files to workspace
- **edit_file** - Edit files with old/new content replacement
- **list_files** - List files matching glob patterns
- **plan_edits** - Plan batch edits with diff previews
- **apply_edit_plan** - Apply pre-planned edit operations

#### Search & Analysis
- **search_files** - Full-text search with regex, glob patterns, and `.gitignore` support
- **project_analysis** - Analyze project structure, dependencies, and file types

#### Terminal Management (PTY-based)
- **pty_launch** - Launch new terminal sessions
- **pty_send_keys** - Send keyboard input to terminals
- **pty_get_screen** - Capture terminal screen content
- **pty_get_cursor** - Get cursor position
- **pty_resize** - Resize terminal dimensions
- **pty_set_scrollback** - Configure scrollback buffer
- **pty_start_capture** / **pty_stop_capture** - Output capture control
- **pty_list** - List active sessions (max 15 concurrent)
- **pty_kill** - Terminate sessions
- **pty_request_user_input** - Request user input

**Terminal Features**:
- VT100/ANSI escape sequence interpretation
- Persistent screen buffer (1000 lines default)
- Multiple backend support (native PTY, tmux)
- Session logging

#### Task & Workflow Management
- **todo_write** - Create and manage task lists with status tracking (pending, in_progress, completed)
- **todo_list** - View task progress
- **load_skill** - Load proven workflow patterns
- **list_skills** - Discover available skills
- **find_relevant_skills** - AI-powered skill discovery with semantic search

**Available Skills** (20+ curated workflows):
- Brainstorming, Commands, Condition-based Waiting
- Defense-in-Depth, Dispatching Parallel Agents
- Executing Plans, Finishing Development Branches
- Receiving/Requesting Code Review
- Root Cause Tracing, Sharing Skills
- Subagent-Driven Development, Systematic Debugging
- Test-Driven Development, Testing Anti-Patterns
- Using Git Worktrees, Using Superpowers
- Verification Before Completion, Writing Plans

#### System & Control
- **run_command** - Execute shell commands with security checks
- **switch_model** - Request model switching with justification
- **request_more_iterations** - Request additional processing iterations

### 🌐 Web Server & API

#### HTTP API Endpoints
- `GET /api/sessions` - List all active sessions
- `POST /api/sessions` - Create new session
- `GET /api/sessions/:id` - Get session details
- `DELETE /api/sessions/:id` - Close session

#### WebSocket Support
- **Real-time Streaming** (`/ws/:session_id`) - Bidirectional communication
- **Multi-client Sessions** - Multiple clients per session
- **Tool Confirmation Flow** - Approve/deny tool execution
- **Persistent Storage** - Sessions saved to disk automatically

#### Session Features
- **Editable Titles** - Auto-generated and customizable
- **Chat History** - Full conversation persistence
- **UUID-based IDs** - Unique session identification
- **Session Types** - Web, TUI, and Shared sessions

### 🔒 Security & Policy System

- **Action-based Policies** - Fine-grained control over:
  - File operations (read, write, edit, delete)
  - Command execution
  - Edit planning and application

- **Policy Types**:
  - `Allow` - Auto-approve actions
  - `Deny` - Block actions
  - `Ask` - Require user confirmation

- **Pattern Matching** - Glob patterns for files, string patterns for commands

### 🚀 Operating Modes

1. **REPL Mode** - Interactive command-line conversation (default)
2. **Task Mode** - One-shot task execution with `--task` flag
3. **Web Server Mode** - Full HTTP/WebSocket API for web interfaces
4. **Agent Mode** - Multi-agent coordination for complex tasks (`--agents` flag)

### 📊 Session Persistence & Logging

- **Conversation Logging** - Automatic logging to files
- **Session Metadata** - Model info, tokens, timestamps
- **State Management** - Save/load conversation history (JSON format)
- **Token Tracking** - Usage metrics per session

### 🌍 WebAssembly Frontend

- Browser-based chat interface (WASM)
- WebSocket client integration
- Markdown rendering
- Local storage for session persistence

## Prerequisites

- **Rust** (latest stable version)
- **API Keys** for your chosen provider(s):
  - Groq API key (for Groq models)
  - Anthropic API key (for Claude models)
  - OpenAI API key (for OpenAI models)
  - Or use llama.cpp for local models (no API key needed)

## Installation

1. Clone this repository:
```bash

git clone <repository-url>
cd kimichat
```

2. Build the project:
```bash

cargo build --release
```

3. (Optional) Generate shell completions:
```bash
./target/release/kimichat --generate bash > kimichat-completion.bash
source kimichat-completion.bash
```

## Configuration

### Environment Variables

Set API keys for your chosen provider(s):

```bash
# For Groq
export GROQ_API_KEY=your_groq_api_key

# For Anthropic Claude (supports multiple model slots)
export ANTHROPIC_AUTH_TOKEN_BLU=your_claude_key_for_blu_model
export ANTHROPIC_AUTH_TOKEN_GRN=your_claude_key_for_grn_model
export ANTHROPIC_AUTH_TOKEN_RED=your_claude_key_for_red_model

# For OpenAI
export OPENAI_API_KEY=your_openai_api_key
```

### Command-Line Options

#### Model Configuration
```bash
# Use custom API URLs for each model slot
--api-url-blu-model <URL>
--api-url-grn-model <URL>
--api-url-red-model <URL>

# Override model names
--model-blu-model <NAME>
--model-grn-model <NAME>
--model-red-model <NAME>

# Quick llama.cpp setup
--llama-cpp-url <URL>
```

#### Mode Selection
```bash
# Run single task and exit
--task "Your task here"

# Enable multi-agent system
--agents

# Enable streaming responses
--stream

# Force interactive mode
--interactive
```

#### Debug & Output
```bash
# Enable verbose output
--verbose

# Set debug level (0-5)
--debug <LEVEL>

# Pretty-print JSON output
--pretty
```

## Usage

### REPL Mode (Interactive)

Start an interactive session:

```bash
cargo run
# or
./target/release/kimichat
```

The application will:
1. Create a workspace directory if needed
2. Start an interactive chat session
3. Allow natural conversation with AI models
4. Execute tools automatically when needed

**Example interaction:**
```
[GrnModel] You: Create a Rust project structure for a web API

🔧 Calling tool: write_file with args: {"file_path":"Cargo.toml", ...}
📋 Result: Successfully wrote to Cargo.toml

🔧 Calling tool: write_file with args: {"file_path":"src/main.rs", ...}
📋 Result: Successfully wrote to src/main.rs

[GrnModel] Assistant: I've created a basic Rust web API project structure...
```

### Task Mode (One-shot)

Execute a single task:

```bash
kimichat --task "Analyze all Rust files and create a summary report"
```

Results are logged to `~/.kimichat/sessions/` by default.

### Web Server Mode

Start the web server:

```bash
kimichat --web --bind 127.0.0.1:8080
```

Then interact via HTTP API or WebSocket. Example using curl:

```bash
# Create a new session
curl -X POST http://localhost:8080/api/sessions

# List sessions
curl http://localhost:8080/api/sessions

# Connect via WebSocket (use any WebSocket client)
# ws://localhost:8080/ws/<session-id>
```

### Agent Mode

Enable multi-agent system for complex tasks:

```bash
kimichat --agents --task "Design and implement a complete authentication system"
```

Multiple specialized agents coordinate to complete the task.

## Advanced Features

### Custom Model Configuration

Use llama.cpp for local inference:

```bash
kimichat --llama-cpp-url http://localhost:8080/v1 \
         --model-blu-model "llama3-8b" \
         --model-grn-model "llama3-70b"
```

### Policy-Based Security

Create a policy file (TOML) to control tool behavior:

```toml
[[policy]]
action = "FileWrite"
pattern = "*.rs"
decision = "Ask"  # Require confirmation for Rust files

[[policy]]
action = "CommandExecution"
pattern = "rm *"
decision = "Deny"  # Block dangerous commands
```

### Skill System

Load proven workflows:

```
[Model] You: Load the test-driven-development skill

[Model] Assistant: Loaded TDD skill. I'll now follow test-first methodology...
```

Find relevant skills:

```
[Model] You: Find skills related to debugging

[Model] Assistant: Found: systematic-debugging, root-cause-tracing...
```

## Architecture

### Technology Stack

**Core**:
- Rust 2021 Edition
- Tokio (async runtime)
- Axum (web framework)
- Reqwest (HTTP client)

**LLM Integration**:
- Anthropic Claude API
- Groq API
- OpenAI API
- llama.cpp

**Terminal & UI**:
- portable-pty (PTY sessions)
- vt100 (ANSI parsing)
- rustyline (REPL)
- colored (terminal colors)

**Data & Search**:
- serde/serde_json (serialization)
- regex (pattern matching)
- ignore (gitignore-aware traversal)
- fastembed (semantic search for skills)

### Project Structure

```
kimichat/
├── Cargo.toml              # Workspace configuration
├── kimichat-main/          # Main binary and CLI
├── kimichat-agents/        # Multi-agent orchestration
├── kimichat-llm-api/       # Unified LLM client interface
├── kimichat-models/        # Data structures and types
├── kimichat-toolcore/      # Tool execution framework
├── kimichat-tools/         # 20+ implemented tools
├── kimichat-terminal/      # PTY session management
├── kimichat-skills/        # Skill registry and loading
├── kimichat-policy/        # Security and approval system
├── kimichat-logging/       # Conversation logging
├── kimichat-todo/          # Task tracking
├── kimichat-wasm/          # WebAssembly frontend
└── skills/                 # Skill definitions (SKILL.md files)
```

### Component Overview

```
KimiChat
├── Chat System (messages, history, state)
├── API Layer (multi-provider LLM integration)
├── Tool System (extensible framework)
├── Web Server (HTTP + WebSocket)
├── Terminal Manager (PTY sessions)
├── Policy Manager (security/approval)
├── Skill Registry (workflow patterns)
├── Logger (conversation tracking)
├── Task Coordinator (todo management)
└── Multi-Agent System (specialized agents)
```

## Contributing

Contributions are welcome! Areas for enhancement:

- Additional LLM providers
- New tools and capabilities
- Enhanced web UI features
- Additional skills and workflows
- Performance optimizations
- Documentation improvements

## Code Metrics

- **12,800+ lines** of Rust code
- **12 modular crates** for clean separation
- **20+ tools** for diverse operations
- **20+ curated skills** for proven workflows
- **4 LLM providers** supported
- **15 concurrent** terminal sessions
- **Full test coverage** (in progress)

## License

This project is provided as-is for educational and development purposes.

## Credits

Inspired by Anthropic's Claude Code and built with a focus on extensibility, security, and developer experience.