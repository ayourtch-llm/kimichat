# Kimi Chat - Rust CLI Application

## Important Notes - MUST FOLLOW!

**Conversation History**: Check existing conversation history before deciding whether to perform operations - avoid redundant calls
**File Operations**: Use specific patterns like `"src/*.rs"` instead of `"*.rs"` to locate files in the src directory
**Repeat operations**: If your history already has a file read, do not read it again - as this will overload the history. Likewise, if you are doing an edit - do not attempt to do it multiple times, if something fails, ask the user to verify.

## Project Overview

kimi-chat is a Rust-based CLI application that provides a Claude Code-like experience with multi-model AI support, file operations, and intelligent model switching capabilities. It uses Groq's API to interact with AI models and provides a sophisticated tool-calling system.

## Project Structure

```
kimichat/
├── Cargo.toml          # Dependencies and package configuration
├── src/
│   ├── main.rs         # Main application with API integration
│   ├── logging.rs      # Conversation logging with JSON format
│   ├── open_file.rs    # File opening utilities with line range support
│   └── preview.rs      # Two-word preview generation for tasks
├── agents/             # Agent configurations
│   └── configs/
│       ├── code_analyzer.json
│       ├── file_manager.json
│       ├── search_specialist.json
│       └── system_operator.json
├── workspace/          # Working directory (auto-created)
├── logs/              # Log files directory
├── README.md          # User documentation
├── .env.example       # Environment configuration template
├── .gitignore         # Git ignore rules
├── kimi.md            # This project documentation
├── how_to_new_tool.md # Guide for adding new tools
├── subagent.md        # Subagent documentation
├── wishlist.md        # Feature wishlist
└── target/            # Build artifacts
```

## Key Components

### Dependencies
- **tokio**: Async runtime with full features
- **reqwest**: HTTP client for API calls with JSON support
- **serde**: Serialization with derive features
- **colored**: Terminal colors for output formatting
- **rustyline**: Interactive CLI with error handling
- **glob**: File pattern matching
- **anyhow**: Error handling with context
- **dotenvy**: Environment variable loading
- **regex**: Regular expression support
- **chrono**: Date/time handling for logging
- **thiserror**: Custom error types
- **serde_json**: JSON serialization
- **similar**: Text diffing capabilities

### AI Models
- **Kimi-K2-Instruct-0905** (`moonshotai/kimi-k2-instruct-0905`): General tasks, coding, quick responses
- **GPT-OSS-120B** (`openai/gpt-oss-120b`): Complex reasoning, analysis, problem-solving

### Features
- Multi-model support with automatic switching based on task complexity
- File operations (read/write/edit/list) with workspace safety
- Tool calling system with validation and repair
- Conversation history management with automatic summarization
- Rate limiting and retry logic with exponential backoff
- Terminal UI with colored output and model indicators
- Workspace directory for safe file operations
- Search files with regex support
- Interactive command execution
- Agent-based specialized configurations
- JSON-formatted conversation logging

## Architecture

### Core Types
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ModelType {
    Kimi,
    GptOss,
}

struct App {
    client: reqwest::Client,
    api_key: String,
    conversation_history: Vec<Message>,
    current_model: ModelType,
    workspace_dir: PathBuf,
}
```

### Message System
- Role-based messages (user, assistant, system, tool)
- Tool calling support with automatic repair
- Conversation history with token management
- Message serialization for logging

### Tool System
- Extensible tool framework defined in main.rs
- Built-in tools: read_file, open_file, write_file, edit_file, list_files, search_files, run_command, switch_model
- Tool validation and error handling
- Automatic tool repair for hallucinations

### API Integration
- Groq API integration (`https://api.groq.com/openai/v1/chat/completions`)
- Model-aware requests with proper model strings
- Token usage tracking and cost management
- Rate limiting protection with automatic retries
- Error handling with model switching for tool-related issues

### Agent System
- Pre-configured agent types in `agents/configs/`
- Specialized tool sets for different tasks (code analysis, file management, search, system operations)
- Model preferences per agent type

## Configuration

- **Environment**: `GROQ_API_KEY` required in `.env` file
- **Working Directory**: Uses `workspace/` subdirectory for file operations
- **Models**: Switch between Kimi and GPT-OSS based on task requirements
- **Logging**: JSON-formatted conversation logs in `logs/` directory

## Usage

```bash
# Build
cargo build --release

# Run
cargo run

# Release build
./target/release/kimichat

# Set up environment
cp .env.example .env
# Edit .env to add your GROQ_API_KEY
```

## Key Functions

- **summarize_and_trim_history**: Manages conversation history with automatic summarization and token limits
- **call_api**: API calls with retry logic, rate limiting protection, and model switching
- **execute_tool**: Tool execution with validation and automatic repair
- **switch_model**: Model switching with reason tracking and validation
- **repair_tool_call_with_model**: Automatic tool repair for hallucinations
- **log_conversation**: JSON-formatted conversation logging with metadata
- **preview.rs::two_word_preview**: Generate concise task previews

## Safety Features

- Workspace directory isolation for file operations
- Recursive pattern prevention in glob operations
- Empty content validation for file operations
- Tool hallucination detection and repair
- Rate limiting protection with exponential backoff
- Conversation history management with token limits
- File operation validation and error handling

## Terminal UI

- Colored output with model indicators
- Interactive prompts with rustyline
- Error handling with anyhow context
- Real-time conversation display
- Model switching notifications

## Agent Configurations

The system includes pre-configured agents optimized for different tasks:
- **code_analyzer**: Code analysis with read/search tools
- **file_manager**: File operations with full tool access
- **search_specialist**: Search operations with regex support
- **system_operator**: System operations with command execution

## Dependencies

- **Core**: tokio, reqwest, serde, colored, rustyline, glob, anyhow
- **Features**: Full async support, JSON handling, terminal colors, pattern matching
- **Logging**: chrono for timestamps, serde_json for structured logging

## Project Summary

kimi-chat is a sophisticated Rust CLI application that provides a Claude Code-like experience with multi-model AI support, intelligent model switching, and comprehensive file operations. It features automatic model switching based on task complexity, conversation history management with summarization, safe workspace operations, and a robust tool-calling system with validation and repair capabilities. The application uses Groq's API to access state-of-the-art AI models while providing a terminal-based interface with colored output and interactive features.
