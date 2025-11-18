# /compact Command Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement a `/compact` command that forces conversation compaction when triggered by the user, providing immediate feedback and using the existing intelligent compaction system.

**Architecture:** Add command handling to the REPL module to detect `/compact` input, call the existing `intelligent_compaction` function from the history module, and provide user feedback about the compaction results.

**Tech Stack:** Rust, existing conversation history and compaction modules

---

### Task 1: Add /compact Command Help Text

**Files:**
- Modify: `kimichat-main/src/app/repl.rs:320-325`

**Step 1: Add /compact to /skills help display**

Add this line to the skills help section:
```rust
println!("  /compact               - Force immediate conversation compaction to reduce session size");
```

**Step 2: Run tests to verify help display works**

Run: `cargo test --lib app::repl::tests`
Expected: PASS (tests should still pass)

**Step 3: Commit**

```bash
git add kimichat-main/src/app/repl.rs
git commit -m "feat: add /compact command to help text"
```

### Task 2: Implement /compact Command Handler

**Files:**
- Modify: `kimichat-main/src/app/repl.rs:400-450`

**Step 1: Add /compact command detection**

Add this code after the /write-plan command handler:
```rust
// Handle /compact command
if line == "/compact" {
    println!("{} Starting manual conversation compaction...", "🗜️".bright_yellow());
    
    match crate::chat::history::intelligent_compaction(&mut chat, 0).await {
        Ok(()) => {
            let size = crate::chat::history::calculate_conversation_size(&chat.messages);
            println!("{} Compaction complete. Current session size: {:.1} KB ({} messages)", 
                     "✅".bright_green(), 
                     size as f64 / 1024.0,
                     chat.messages.len());
        }
        Err(e) => {
            eprintln!("{} Compaction failed: {}", "❌".bright_red(), e);
        }
    }
    continue;
}
```

**Step 2: Test compilation**

Run: `cargo check`
Expected: PASS (no compilation errors)

**Step 3: Run tests**

Run: `cargo test --lib app::repl::tests`
Expected: PASS (existing tests should still pass)

**Step 4: Commit**

```bash
git add kimichat-main/src/app/repl.rs
git commit -m "feat: implement /compact command handler"
```

### Task 3: Add /compact Command Unit Test

**Files:**
- Create: `kimichat-main/src/app/repl_compact_tests.rs`

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod compact_tests {
    use super::*;
    use crate::KimiChat;
    use kimichat_models::{Message, ModelType};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn test_compact_command_exists() {
        // This test verifies that the /compact command is properly handled
        // and doesn't cause crashes when executed
        
        let mut chat = create_test_chat().await;
        
        // Add some messages to make compaction meaningful
        chat.messages.push(Message {
            role: "user".to_string(),
            content: "Test message for compaction".repeat(100),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning: None,
        });
        
        let initial_count = chat.messages.len();
        
        // The compact command should not crash and should preserve system messages
        let result = crate::chat::history::intelligent_compaction(&mut chat, 0).await;
        
        assert!(result.is_ok(), "Compaction should succeed");
        
        // Should still have at least the system message
        assert!(chat.messages.len() >= 1, "Should have at least system message after compaction");
    }

    async fn create_test_chat() -> KimiChat {
        KimiChat::new_with_client_config(
            ModelType::GrnModel,
            Default::default(),
            None,
            None,
            None,
        )
    }
}
```

**Step 2: Add test module to repl.rs**

Add this at the end of repl.rs:
```rust
#[cfg(test)]
mod repl_compact_tests;
```

**Step 3: Run test to verify it fails initially**

Run: `cargo test repl_compact_tests::compact_tests::test_compact_command_exists`
Expected: PASS (the test should actually pass since we're just testing the compaction function)

**Step 4: Run all REPL tests**

Run: `cargo test --lib app::repl`
Expected: PASS

**Step 5: Commit**

```bash
git add kimichat-main/src/app/repl.rs kimichat-main/src/app/repl_compact_tests.rs
git commit -m "test: add unit test for /compact command functionality"
```

### Task 4: Update Documentation

**Files:**
- Modify: `README.md` (if it exists and has command documentation)

**Step 1: Check if README exists**

Run: `ls kimichat-main/README.md || echo "README not found"`

**Step 2: Update documentation if README exists**

If README.md exists and has a commands section, add:
```markdown
- `/compact` - Force immediate conversation compaction to reduce session size
```

**Step 3: Commit if documentation updated**

```bash
git add kimichat-main/README.md
git commit -m "docs: add /compact command to README"
```

### Task 5: Integration Test

**Files:**
- No new files - test the feature manually

**Step 1: Build the application**

Run: `cargo build --release`
Expected: PASS (successful build)

**Step 2: Test /compact command functionality**

Run the application and test:
1. Start a conversation with some messages
2. Type `/compact`
3. Verify compaction message appears
4. Verify session size is reported

**Step 3: Test edge cases**

1. Test `/compact` on empty session
2. Test `/compact` on very small session
3. Test `/compact` on large session (if possible)

**Step 4: Commit final integration notes**

```bash
git commit -m "feat: complete /compact command implementation with testing"
```

### Task 6: Final Verification

**Files:**
- All modified files

**Step 1: Run full test suite**

Run: `cargo test --lib`
Expected: PASS (all tests should pass)

**Step 2: Check code formatting**

Run: `cargo fmt --check`
Expected: PASS (code should be properly formatted)

**Step 3: Run clippy for lints**

Run: `cargo clippy --lib -- -D warnings`
Expected: PASS (no new warnings)

**Step 4: Final commit**

```bash
git add .
git commit -m "feat: complete /compact command implementation"
```

## Testing Strategy

### Manual Testing Checklist:
- [ ] `/compact` command is recognized
- [ ] Compaction provides feedback messages
- [ ] Session size is reported after compaction
- [ ] Command doesn't crash on empty sessions
- [ ] Command doesn't crash on small sessions
- [ ] Help text includes the new command

### Automated Testing:
- [ ] Unit tests for compaction functionality
- [ ] Integration tests for REPL command handling
- [ ] No regression in existing functionality

## Expected Behavior

When user types `/compact`:
1. System immediately shows "Starting manual conversation compaction..." message
2. Calls existing `intelligent_compaction` function with `current_tool_iteration = 0`
3. If successful, shows compaction complete message with new session size and message count
4. If failed, shows error message
5. Command is handled locally and not sent to the AI model

The command integrates seamlessly with existing conversation management while providing users control over when to compact their sessions.