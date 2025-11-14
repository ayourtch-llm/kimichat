# KimiChat - Multi-Agent AI CLI

## Project Overview

KimiChat is a Rust CLI application providing a Claude Code-like experience with tool-enabled LLM interactions. It operates in two modes:

1. **Single LLM Mode** - Direct conversation with one LLM that has full tool access
2. **Multi-Agent Mode** (`--agents`) - Sophisticated planner-first architecture with specialized agents for complex tasks

Supports multiple LLM backends: Groq, Anthropic, and llama.cpp.

## Core Architecture

KimiChat operates in two modes:

### 1. Single LLM Mode (Default)

**Simple Direct Flow:**
1. User sends message
2. Single LLM processes request with full tool access
3. Tool execution loop: LLM can call tools, see results, and continue
4. LLM responds directly to user

**Features:**
- Direct conversation with one LLM instance
- Access to all available tools (file ops, search, commands, etc.)
- Simpler for straightforward tasks and conversations
- Tool calling with confirmations for destructive operations
- Conversation history with summarization to prevent context overflow

### 2. Multi-Agent System (with `--agents` flag)

**Planner-First Flow:**
1. User request → PlanningCoordinator
2. **Planner agent** analyzes and decomposes request into subtasks
3. Planner assigns subtasks to specialized agents
4. Specialized agents execute tasks using their available tools
5. Results are synthesized into final response

**Specialized Agents:**
- `planner` - Task decomposition and agent assignment (NO tool access, dynamically discovers available agents)
- `code_analyzer` - Code analysis and architecture review
- `code_reviewer` - Code review with mandatory skill-based workflows (quality assurance)
- `file_manager` - File operations (read, write, edit)
- `search_specialist` - Search and discovery across codebase
- `system_operator` - Command execution, builds, and batch operations
- `terminal_specialist` - Interactive terminal sessions, PTY management, REPLs, long-running processes

**Agent Characteristics:**
- Specific tool access (defined in `agents/configs/*.json`)
- Iteration limits with dynamic extension via `request_more_iterations` tool
- System prompts tuned for their specialty
- Better for complex multi-step tasks requiring specialized expertise
- Planner dynamically discovers available agents at runtime (no hardcoded agent list)

The system uses a single configurable agent implementation (`ConfigurableAgent`) that is instantiated based on JSON configurations. All agents are instances of the same underlying class, configured differently for their specific roles.

### Key Design Patterns

**Tool Registry:**
- Central registry (`src/core/tool_registry.rs`)
- Tools implement `Tool` trait
- Categories: file_ops, search, system, model_management, agent_control, skills

**Agent Factory:**
- Creates agents from JSON configurations
- Filters tools by agent's allowed list
- Handles tool execution loop with iteration management

**Visibility System:**
- Tracks task execution hierarchically
- Shows task progress and agent assignments
- Phases: Planning → AgentSelection → TaskExecution → Aggregation → Completed

**Skills System:**
- Proven workflows and best practices as reusable templates
- Skills are MANDATORY when available (following superpowers philosophy)
- Dynamic skill discovery from `skills/` directory
- Three access patterns: tools, slash commands, and session hooks
- Enforced through agent system prompts and session initialization

## Skills System Architecture

KimiChat includes a comprehensive skills system inspired by [obra/superpowers](https://github.com/obra/superpowers). Skills are proven workflows and best practices that agents MUST follow when applicable.

### Philosophy

**Skills are MANDATORY, not optional.** When a skill exists for a task:
- Agents MUST check for relevant skills before starting work
- Agents MUST load and follow applicable skills exactly as written
- Skipping or improvising instead of following skills is prohibited

This enforces battle-tested workflows and prevents agents from "winging it" on complex tasks.

### Skills Directory Structure

```
skills/
├── test-driven-development/
│   └── SKILL.md              # TDD workflow for all code changes
├── systematic-debugging/
│   └── SKILL.md              # Structured debugging process
├── writing-plans/
│   └── SKILL.md              # Creating implementation plans
├── executing-plans/
│   └── SKILL.md              # Executing plans with checkpoints
├── brainstorming/
│   └── SKILL.md              # Socratic method for design refinement
├── requesting-code-review/
│   └── SKILL.md              # How to perform code reviews
├── receiving-code-review/
│   └── SKILL.md              # How to respond to review feedback
├── using-superpowers/
│   └── SKILL.md              # Meta-skill explaining skill usage
└── ... (20+ total skills)
```

Each `SKILL.md` contains:
- YAML frontmatter (name, description)
- Detailed workflow instructions
- Examples and best practices
- Decision trees and checklists

### Three Ways to Use Skills

**1. Tool-Based Access (Agents)**

All agents have access to three skill tools:
- `load_skill` - Load and read a specific skill by name
- `list_skills` - List all available skills
- `find_relevant_skills` - Search for skills matching a task description

Agents are instructed in their system prompts to ALWAYS check for relevant skills before starting work.

**2. Slash Commands (Users)**

Interactive REPL provides direct skill invocation:
- `/brainstorm` - Launch brainstorming skill for design refinement
- `/write-plan` - Create detailed implementation plan
- `/execute-plan` - Execute plan with review checkpoints
- `/skills` - Show available skill commands

These inject the skill as a system message, making it active for the current conversation.

**3. Session Hook (Automatic)**

The `hooks/session-start.sh` script runs at REPL startup and injects the `using-superpowers` skill as foundational context. This ensures agents understand the skills system from the beginning of every session.

### Implementation Details

**SkillRegistry** (`src/skills/mod.rs`):
- Loads all `SKILL.md` files from `skills/` directory
- Parses YAML frontmatter for metadata
- Provides lookup and relevance matching
- Shared across all agents via `Arc<SkillRegistry>`

**Context Propagation:**
```
KimiChat.skill_registry (Arc<SkillRegistry>)
  → ExecutionContext.skill_registry
  → ToolContext.skill_registry
  → Skill Tools (load_skill, etc.)
```

**Mandatory Enforcement:**

All agent system prompts include:
```
═══════════════════════════════════════════════════════════════
🎯 MANDATORY SKILL USAGE
═══════════════════════════════════════════════════════════════

BEFORE starting ANY task, you MUST:
1. Use find_relevant_skills to check for applicable skills
2. If relevant skills found, use load_skill to read them
3. Follow the skill exactly as written - NO exceptions
4. Announce: "I'm using the [skill-name] skill to [what you're doing]"

IF A SKILL EXISTS FOR YOUR TASK, USING IT IS MANDATORY. Not optional.
```

### Skill-Based Agents

Some agents are specifically designed around skills:

**code_reviewer** (`agents/configs/code_reviewer.json`):
- Mandatory use of `requesting-code-review` and `receiving-code-review` skills
- Reviews code against plans and quality standards
- Structured output: Strengths, Issues (Critical/Important/Minor), Assessment
- Clear verdict: Ready to merge / With fixes / Needs work

Add new skill-based agents by:
1. Creating agent config in `agents/configs/`
2. Specifying required skills in system prompt
3. Including skill tools in tools array
4. Agent auto-discovered by planner

### Available Skills

Core skills ported from superpowers:
- **Development Workflows**: test-driven-development, subagent-driven-development
- **Planning**: writing-plans, executing-plans, brainstorming
- **Debugging**: systematic-debugging, root-cause-tracing
- **Quality**: requesting-code-review, receiving-code-review, verification-before-completion
- **Architecture**: planning-architectural-changes, planning-migrations
- **Process**: working-iteratively, using-git-history, rubber-duck-debugging
- And 10+ more...

Each skill is a battle-tested workflow that prevents common mistakes and ensures thorough execution.

## Project Structure

### Core Modules (used by both modes)

```
src/
├── main.rs              # KimiChat struct, main entry point
├── app/                 # Application modes (setup, REPL, task)
├── api/                 # LLM API clients (streaming/non-streaming)
├── chat/                # Conversation management
│   ├── session.rs       # Main chat loop (single LLM mode)
│   ├── history.rs       # History summarization
│   └── state.rs         # State persistence
├── config/              # Configuration management
│   └── helpers.rs       # API URL/key resolution
├── tools/               # Tool implementations
│   ├── file_ops.rs      # File operations with confirmations
│   ├── search.rs        # Code search
│   ├── system.rs        # Command execution
│   ├── iteration_control.rs # Dynamic iteration requests
│   └── skill_tools.rs   # Skill loading and discovery (load_skill, list_skills, find_relevant_skills)
├── terminal/            # PTY/terminal session management
│   ├── session.rs       # Terminal session with background reader
│   ├── manager.rs       # Multi-session management
│   ├── pty_handler.rs   # PTY process and I/O
│   ├── screen_buffer.rs # VT100 terminal emulation
│   ├── tools.rs         # 11 PTY tool implementations
│   └── logger.rs        # Session logging
├── models/              # Data structures and types
├── logging/             # Request/response logging (JSONL format)
└── skills/              # Skills system (SkillRegistry, Skill struct)
```

### Skills and Hooks

```
skills/                  # Proven workflows and best practices (20+ skills)
├── test-driven-development/
├── systematic-debugging/
├── writing-plans/
├── executing-plans/
├── brainstorming/
├── requesting-code-review/
├── receiving-code-review/
├── using-superpowers/
└── ... (more skills)

hooks/                   # Session lifecycle hooks
└── session-start.sh     # Injects skill context at REPL startup
```

### Multi-Agent System (used only with `--agents`)

```
src/agents/
├── agent.rs             # Configurable agent implementation
├── agent_factory.rs     # Agent creation and execution loop
├── agent_config.rs      # Agent configuration handling
├── coordinator.rs       # Planning and task distribution
├── progress_evaluator.rs # Agent progress tracking
├── visibility.rs        # Task tracking and progress display
└── task.rs              # Task and subtask definitions

agents/configs/          # Agent configurations (JSON)
├── planner.json         # Task planning and decomposition
├── code_analyzer.json   # Code analysis and architecture
├── code_reviewer.json   # Code review with skill-based workflows
├── file_manager.json    # File operations
├── search_specialist.json # Code search and discovery
├── system_operator.json # Command execution
└── terminal_specialist.json # PTY/terminal session management
```

**Agent Configurations** (`agents/configs/*.json`) define:
- Model selection
- Available tools
- System prompts
- Tool access permissions

**Dynamic Agent Discovery:**
- Planner automatically discovers all agents from loaded configurations
- No hardcoded agent lists - just add new `.json` files to `agents/configs/`
- Agent list with tools is built at runtime and passed to planner
- Makes system easily extensible without modifying planner code

## CLI Usage

```bash
# Interactive mode with multi-agent system
cargo run -- --agents -i

# Interactive mode with single LLM
cargo run -- -i

# One-off task with agents
cargo run -- --agents --task "analyze the codebase"

# Stream responses (default)
cargo run -- --agents -i --stream

# Auto-confirm all actions
cargo run -- --agents -i --auto-confirm

# Use custom llama.cpp server
cargo run -- --llama-cpp-url http://localhost:8080 -i
```

## Key Features

### Tool System (both modes)
- **Tool Confirmations:** File edits show unified diffs, commands require approval
- **Batch Edits:** `plan_edits`/`apply_edit_plan` for multi-file changes
- **Smart Error Handling:** AI-powered tool call repair for malformed JSON
- **File Operations:** Automatic line range clamping, gitignore respect
- **XML Support:** Fallback parsing for models that prefer XML format
- **Loop Detection:** Enhanced detection with separate thresholds for consecutive vs scattered repeats, leniency for read-only operations

### Conversation Management (both modes)
- **History Summarization:** AI-powered summarization when conversation exceeds 200KB
- **State Persistence:** Save/load conversation state
- **Streaming:** Real-time response streaming (default)
- **Model Switching:** Dynamic model changes during conversation

### Iteration Management (multi-agent mode)
- Default 50 iterations per agent (prevents infinite loops)
- Warnings at iteration 47+
- Agents can request more iterations with justification
- Dynamic limit adjustment mid-execution

### Terminal/PTY Sessions (both modes)
- **Background Reader:** Each PTY session has a continuous background thread updating screen buffer
- **Always Current:** Screen buffer is automatically updated as output arrives
- **No Polling Needed:** Tools read from buffer directly without manual updates
- **Graceful Shutdown:** Non-blocking thread cleanup prevents hangs on session kill
- **11 PTY Tools:** Launch, send keys, get screen, list, kill, cursor, resize, scrollback, capture start/stop, request input
- **Explicit \n Requirement:** Tool descriptions emphasize newline requirement for command execution

### Interruption Support
- **Single LLM Mode:** Ctrl-C at readline prompt cancels input (existing rustyline behavior)
- **Multi-Agent Mode:** Ctrl-C during tool execution interrupts agent gracefully via cancellation tokens
- **Cancellation Propagation:** Token flows through coordinator → tasks → agents
- **Clean Exit:** Returns to REPL prompt without hanging

### Logging (both modes)
- JSONL format in `logs/` directory
- Full conversation history with tool calls
- Request/response logging with timestamps
- Multi-agent mode adds: task_id, parent_task_id, agent_name

## Working with the Codebase

### Adding New Tools (applies to both modes)
1. Implement `Tool` trait in `src/tools/`
2. Register in tool registry initialization (`src/main.rs`)
3. Tools are automatically available in single LLM mode
4. For multi-agent mode: Add to relevant agent configs

### Adding New Agents (multi-agent mode only)
1. Create config in `agents/configs/your_agent.json`
2. Define tools, model, and system prompt
3. Agent will be automatically loaded on startup
4. Planner will automatically discover and use the new agent (no code changes needed)

### Modifying Agent Behavior (multi-agent mode only)
- Edit system prompts in agent config files
- Adjust tool permissions in `allowed_tools` list
- Configure iteration limits (future: via config)

## Configuration

### Environment Variables
- `GROQ_API_KEY` - Groq API access
- `ANTHROPIC_API_KEY` - Anthropic API access

### CLI Options
- `--agents` - Enable multi-agent system
- `--api-url-*-model` - Custom API endpoints
- `--model-*-model` - Override model names
- `--policy-file` - Custom policy file path
- `--learn-policies` - Learn from user decisions
- `--auto-confirm` - Skip all confirmations
- `--verbose` - Debug output

## Design Philosophy

1. **Explicit over Implicit** - Direct module calls rather than wrapper methods
2. **Modularity** - Single-responsibility modules with clear boundaries
3. **Safety** - Confirmations for destructive operations
4. **Extensibility** - Easy to add new agents, tools, and models
5. **Transparency** - Detailed logging and progress visibility

## Notes

**Both modes:**
- Tool execution is async with proper error handling
- Conversation state persists across tool calls
- All file operations respect gitignore patterns
- Batch edits use file-based state (`.kimichat_edit_plan.json`)
- PTY sessions use background reader threads for continuous screen buffer updates
- Loop detection distinguishes between consecutive and scattered repeats
- Read-only tools (file reads, searches) allowed more repetitions than write operations

**Skills system:**
- All agents have mandatory skill checking in their system prompts
- Skills loaded dynamically from `skills/` directory on startup
- Three access patterns: agent tools, user slash commands, session hooks
- Session hook (`hooks/session-start.sh`) injects using-superpowers skill at startup
- Skills are MANDATORY when available (not optional suggestions)
- SkillRegistry shared across all agents via Arc for efficient access
- Slash commands: `/brainstorm`, `/write-plan`, `/execute-plan`, `/skills`

**Multi-agent mode specific:**
- Planner agent is used only for planning, not execution (has no tools)
- Planner dynamically discovers available agents at runtime
- Task decomposition only happens for complex multi-step requests
- Simple requests use `single_task` strategy (no decomposition)
- Ctrl-C interruption support with cancellation token propagation
- Cancellation tokens are optional and default to `None` in non-interactive modes

**Terminal/PTY implementation:**
- Background reader thread per session (spawned at session creation)
- Thread continuously reads from PTY and updates screen buffer
- Non-blocking kill operation (thread finishes asynchronously)
- Tools just read from buffer without triggering manual updates
- Prevents race conditions and data loss from orphaned threads

For detailed refactoring history, see `REFACTORING_SUMMARY.md`.
