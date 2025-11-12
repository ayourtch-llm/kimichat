# Main.rs Refactoring Map

## Current Structure (3,610 lines)

### Lines 1-44: Imports & Constants
- **Destination**: Stay in `main.rs` (reduced), distribute to modules
- **Content**: use statements, GROQ_API_URL, MAX_CONTEXT_TOKENS, MAX_RETRIES

### Lines 45-122: CLI Struct
- **Destination**: `src/cli.rs`
- **Content**: `Cli` struct with all command-line arguments

### Lines 124-195: Commands Enum
- **Destination**: `src/cli.rs`
- **Content**: `Commands` enum definition

### Lines 197-342: Commands impl
- **Destination**: `src/cli.rs`
- **Content**: `impl Commands { fn execute() }` with all command implementations

### Lines 345-379: ModelType Enum & impl
- **Destination**: `src/models/types.rs`
- **Content**: `ModelType` enum + impl (as_str, display_name, from_str)

### Lines 383-392: Deserializer helper
- **Destination**: `src/models/types.rs`
- **Content**: `deserialize_string_or_null` function

### Lines 394-419: Core Message Types
- **Destination**: `src/models/types.rs`
- **Content**: Message, ToolCall, FunctionCall structs

### Lines 421-433: Tool Definition Types
- **Destination**: `src/models/requests.rs`
- **Content**: Tool, FunctionDef structs

### Lines 435-443: ChatRequest
- **Destination**: `src/models/requests.rs`
- **Content**: ChatRequest struct

### Lines 445-476: ChatResponse & Choice
- **Destination**: `src/models/responses.rs`
- **Content**: Usage, ChatResponse, Choice structs

### Lines 478-532: Streaming Response Types
- **Destination**: `src/models/responses.rs`
- **Content**: StreamChunk, StreamChoice, StreamDelta, StreamToolCallDelta, StreamFunctionDelta

### Lines 534-595: Tool Argument Types
- **Destination**: `src/models/types.rs`
- **Content**: ReadFileArgs, WriteFileArgs, ListFilesArgs, EditFileArgs, SwitchModelArgs, RunCommandArgs, SearchFilesArgs, OpenFileArgs
- **Note**: These are used by tools, so keep with types

### Lines 596-616: ClientConfig
- **Destination**: `src/config.rs`
- **Content**: ClientConfig struct

### Lines 618-625: ChatState
- **Destination**: `src/chat/state.rs`
- **Content**: ChatState struct (serializable state)

### Lines 627-649: KimiChat struct
- **Destination**: Split across modules
- **Strategy**: Keep core struct in `src/chat/session.rs`, but methods distributed

### Lines 651-3177: impl KimiChat (THE BIG ONE - 2,526 lines)

#### Lines 654-744: XML Parsing
- **Destination**: `src/tools_execution/parsing.rs`
- **Content**: `parse_xml_tool_calls()` - 90 lines

#### Lines 747-759: System Prompt
- **Destination**: `src/chat/session.rs`
- **Content**: `get_system_prompt()` - 12 lines

#### Lines 761-775: URL Normalization
- **Destination**: `src/config.rs`
- **Content**: `normalize_api_url()` - 14 lines

#### Lines 777-811: API URL Logic
- **Destination**: `src/config.rs`
- **Content**: `get_api_url()` - 34 lines

#### Lines 813-844: API Key Logic
- **Destination**: `src/config.rs`
- **Content**: `get_api_key()` - 31 lines

#### Lines 846-858: Constructor (basic)
- **Destination**: `src/chat/session.rs`
- **Content**: `new()` - 12 lines

#### Lines 860-873: Constructor with agents
- **Destination**: `src/chat/session.rs`
- **Content**: `new_with_agents()` - 13 lines

#### Lines 875-887: Debug level methods
- **Destination**: `src/chat/session.rs`
- **Content**: `set_debug_level()`, `get_debug_level()`, `should_show_debug()` - 12 lines

#### Lines 889-953: Main constructor with config
- **Destination**: `src/chat/session.rs`
- **Content**: `new_with_config()` - 64 lines

#### Lines 955-981: Tool Registry Initialization
- **Destination**: `src/config.rs`
- **Content**: `initialize_tool_registry()` - 26 lines

#### Lines 983-1104: Agent System Initialization
- **Destination**: `src/config.rs`
- **Content**: `initialize_agent_system()` - 121 lines

#### Lines 1106-1121: Get Tools
- **Destination**: `src/tools_execution/executor.rs`
- **Content**: `get_tools()` - 15 lines

#### Lines 1123-1189: Process with Agents
- **Destination**: `src/chat/session.rs`
- **Content**: `process_with_agents()` - 66 lines

#### Lines 1191-1199: Read File
- **Destination**: `src/tools_execution/executor.rs`
- **Content**: `read_file()` - 8 lines

#### Lines 1201-1234: Switch Model
- **Destination**: `src/chat/session.rs`
- **Content**: `switch_model()` - 33 lines

#### Lines 1236-1256: Save State
- **Destination**: `src/chat/state.rs`
- **Content**: `save_state()` - 20 lines

#### Lines 1258-1277: Load State
- **Destination**: `src/chat/state.rs`
- **Content**: `load_state()` - 19 lines

#### Lines 1279-1306: Execute Tool
- **Destination**: `src/tools_execution/executor.rs`
- **Content**: `execute_tool()` - 27 lines

#### Lines 1308-1568: Summarize and Trim History (LARGE)
- **Destination**: `src/chat/history.rs`
- **Content**: `summarize_and_trim_history()` - 260 lines

#### Lines 1570-1656: Repair Tool Call
- **Destination**: `src/tools_execution/validation.rs`
- **Content**: `repair_tool_call_with_model()` - 86 lines

#### Lines 1658-1750: Validate and Fix Tool Calls
- **Destination**: `src/tools_execution/validation.rs`
- **Content**: `validate_and_fix_tool_calls_in_place()` - 92 lines

#### Lines 1752-1794: Log Request (console)
- **Destination**: `src/logging/request_logger.rs`
- **Content**: `log_request()` - 42 lines

#### Lines 1796-1852: Log Request to File
- **Destination**: `src/logging/request_logger.rs`
- **Content**: `log_request_to_file()` - 56 lines

#### Lines 1854-1903: Log Response
- **Destination**: `src/logging/request_logger.rs`
- **Content**: `log_response()` - 49 lines

#### Lines 1905-1918: Log Stream Chunk
- **Destination**: `src/logging/request_logger.rs`
- **Content**: `log_stream_chunk()` - 13 lines

#### Lines 1920-2150: Call API Streaming (old)
- **Destination**: `src/api/streaming.rs`
- **Content**: `call_api_streaming()` - 230 lines

#### Lines 2152-2381: Call API (non-streaming, old)
- **Destination**: `src/api/client.rs`
- **Content**: `call_api()` - 229 lines

#### Lines 2383-2564: Call API with LLM Client
- **Destination**: `src/api/client.rs`
- **Content**: `call_api_with_llm_client()` - 181 lines

#### Lines 2566-2774: Call API Streaming with LLM Client
- **Destination**: `src/api/streaming.rs`
- **Content**: `call_api_streaming_with_llm_client()` - 208 lines

#### Lines 2776-3177: Main Chat Method (HUGE)
- **Destination**: `src/chat/session.rs`
- **Content**: `chat()` - 401 lines
- **Note**: This is the main chat loop with tool execution

### Lines 3180-3610: async fn main() (430 lines)
- **Destination**: `src/main.rs` (refactored)
- **Content**: Main entry point - will be significantly simplified

## Module Structure Summary

```
src/
├── main.rs              (~150 lines) - Entry point, basic setup
├── cli.rs               (~200 lines) - Cli + Commands
├── config.rs            (~250 lines) - ClientConfig + initialization
├── models/
│   ├── mod.rs          (~30 lines)
│   ├── types.rs        (~250 lines) - Core types, ModelType, Message, ToolCall, args
│   ├── requests.rs     (~50 lines) - Request types
│   └── responses.rs    (~150 lines) - Response types
├── api/
│   ├── mod.rs          (~30 lines)
│   ├── client.rs       (~450 lines) - Non-streaming API
│   └── streaming.rs    (~480 lines) - Streaming API
├── chat/
│   ├── mod.rs          (~30 lines)
│   ├── state.rs        (~80 lines) - Save/load state
│   ├── history.rs      (~280 lines) - History management
│   └── session.rs      (~650 lines) - Main chat loop, KimiChat struct
├── tools_execution/
│   ├── mod.rs          (~30 lines)
│   ├── parsing.rs      (~100 lines) - XML parsing
│   ├── executor.rs     (~100 lines) - Tool execution
│   └── validation.rs   (~200 lines) - Tool validation & repair
└── logging/
    └── request_logger.rs (~180 lines) - API logging
```

## Refactoring Order (dependency-first)

1. ✅ models/types.rs - No dependencies
2. ✅ models/requests.rs - Depends on types
3. ✅ models/responses.rs - Depends on types
4. ✅ models/mod.rs - Re-exports
5. ✅ tools_execution/parsing.rs - Depends on models
6. ✅ config.rs - Depends on models
7. ✅ logging/request_logger.rs - Depends on models, config
8. ✅ api/client.rs - Depends on models, config, logging
9. ✅ api/streaming.rs - Depends on models, config, logging
10. ✅ api/mod.rs - Re-exports
11. ✅ chat/state.rs - Depends on models
12. ✅ chat/history.rs - Depends on models, api
13. ✅ tools_execution/executor.rs - Depends on models, tools
14. ✅ tools_execution/validation.rs - Depends on models, api
15. ✅ tools_execution/mod.rs - Re-exports
16. ✅ chat/session.rs - Depends on everything
17. ✅ chat/mod.rs - Re-exports
18. ✅ cli.rs - Depends on tools, core
19. ✅ main.rs - Refactor to minimal entry point
