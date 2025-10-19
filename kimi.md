# Kimi Chat - Rust CLI Application

## Project Overview

kimi-chat is a Rust-based CLI application that provides a Claude Code-like experience with multi-model AI support, file operations, and intelligent model switching capabilities.

## Project Structure

```
kimichat/
├── Cargo.toml          # Dependencies and package configuration
├── src/
│   └── main.rs         # Main application (~600 lines)
├── workspace/          # Working directory (auto-created)
├── README.md           # Documentation
├── .env.example        # Environment configuration
├── .gitignore          # Git ignore rules
├── kimi.md           # Project documentation
└── target/           # Build artifacts
```

## Key Components

### Dependencies
- **tokio**: Async runtime
- **reqwest**: HTTP client for API calls
- **serde**: Serialization
- **colored**: Terminal colors
- **rustyline**: Interactive CLI
- **glob**: File pattern matching
- **anyhow**: Error handling
- **dotenvy**: Environment variables
- **regex**: Regular expressions
- **dotenvy**: Environment variables
- **regex**: Regular expressions

### Models
- **Kimi-K2-Instruct-0905**: General tasks, coding, quick responses
- **GPT-OSS-120B**: Complex reasoning, analysis, problem-solving

### Features
- Multi-model support with automatic switching
- File operations (read/write/edit/list)
- Tool calling system
- Conversation history management
- Rate limiting and retry logic
- Terminal UI with colors
- Workspace directory for file operations
- Search files with regex support
- Interactive command execution

## Architecture

### Core Types
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ModelType {
    Kimi,
    GptOss,
}
```

### Message System
- Role-based messages (user, assistant, system, tool)
- Tool calling support
- Conversation history

### Tool System
- Extensible tool framework
- File operations (read/write/edit/list)
- Model switching
- Safe workspace operations

### API Integration
- Groq API integration
- Model-aware requests
- Token usage tracking
- Rate limiting protection

## Configuration

- **Environment**: GROQ_API_KEY required
- **Working Directory**: Current directory
- **Models**: Kimi-K2-Instruct-0905 and GPT-OSS-120B

## Usage

```bash
# Build
cargo build --release

# Run
cargo run

# Release build
./target/release/kimichat
```

## Important Notes - MUST FOLLOW!

**Conversation History**: Check existing conversation history before deciding whether to perform operations - avoid redundant calls
**File Operations**: Use specific patterns like `"src/*.rs"` instead of `"*.rs"` to locate files in the src directory
**Repeat operations**: If your history already has a file read, do not read it again - as this will overload the history. Likewise, if you are doing an edit - do not attempt to do it multiple times, if something fails, ask the user to verify.

## Key Functions

- **summarize_and_trim_history**: Manages conversation history with automatic summarization
- **call_api**: API calls with retry logic and rate limiting protection
- **execute_tool**: Tool execution with validation
- **switch_model**: Model switching with reason tracking
- **file operations**: Read/write/edit/list files with safety checks

## Safety Features

- Recursive pattern prevention
- Empty content validation
- Tool hallucination detection
- Rate limiting protection
- Conversation history management

## Terminal UI

- Colored output
- Model indicators
- Interactive prompts
- Error handling
- Conversation history

## Dependencies

- **Core**: tokio, reqwest, serde, colored, rustyline, glob, anyhow
- **Features**: Full async, JSON, terminal colors, pattern matching

## Project Summary

kimi-chat is a sophisticated Rust CLI application that provides a Claude Code-like experience with multi-model AI support, intelligent model switching, and comprehensive file operations. It features automatic model switching, conversation history management, and safe workspace operations in a terminal environment.
