# How to Add New Tools to kimi-chat

## Overview

This guide documents the process of adding new tools to the kimi-chat Rust CLI application. The tool system is modular and follows a consistent pattern for implementation.

## Tool System Architecture

### Core Components

1. **Tool Types**: Each tool is defined as a Rust struct with specific fields
2. **Function Definitions**: JSON schema-based parameter definitions
3. **Tool Implementation**: Async execution with error handling
4. **System Integration**: Tool registration and validation

## Step-by-Step Process

### 1. Define Tool Parameter Struct

Create a new struct for your tool's parameters:

```rust
#[derive(Debug, Deserialize)]
struct MyToolArgs {
    parameter_name: String,
    // Add other fields as needed
}
```

### 2. Add Tool Definition

Add your tool to the `get_tools()` function:

```rust
fn get_tools() -> Vec<Tool> {
    vec![
        // ... existing tools ...
        Tool {
            tool_type: "function".to_string(),
            function: FunctionDef {
                name: "my_tool".to_string(),
                description: "Description of what your tool does".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "parameter_name": {
                            "type": "string",
                            "description": "Description of the parameter"
                        }
                    },
                    "required": ["parameter_name"]
                }),
            },
        },
    ]
}
```

### 3. Implement Tool Execution

Add your tool execution logic:

```rust
impl KimiChat {
    fn my_tool(&self, parameter: &str) -> Result<String> {
        // Implementation here
        Ok(format!("Tool executed with parameter: {}", parameter))
    }
}
```

### 4. Update execute_tool Method

Add your tool to the match statement:

```rust
async fn execute_tool(&mut self, name: &str, arguments: &str) -> Result<String> {
    match name {
        // ... existing tools ...
        "my_tool" => {
            let args: MyToolArgs = serde_json::from_str(arguments)?
            self.my_tool(&args.parameter_name)
        }
        // ... other cases ...
    }
}
```

## System Integration

### 1. Update System Messages

Update the system prompts to include your new tool:

```rust
let system_content = format!(
    "You are an AI assistant with access to file operations and model switching capabilities. \
    You are currently running as {}. You can switch to other models when appropriate:\n\
    - kimi (Kimi-K2-Instruct-0905): Good for general tasks, coding, and quick responses\n\
    - gpt-oss (GPT-OSS-120B): Good for complex reasoning, analysis, and advanced problem-solving\n\
    Available tools (use ONLY these exact names):\n\
    - read_file: Read file contents\n\
    - write_file: Write/create a file\n\
    - edit_file: Edit existing file by replacing content\n\
    - list_files: List files (single-level patterns only, no **)\n\
    - switch_model: Switch between models\n\
    - my_tool: Your tool description here\n\
    IMPORTANT: Only use the exact tool names listed above. Do not make up tool names.",
    self.current_model.display_name()
);
```

### 2. Update Model Switching Logic

Ensure new tools are available when switching models:

```rust
// Update system message when switching models
if let Some(sys_msg) = messages.first_mut() {
    if sys_msg.role == "system" {
        sys_msg.content = format!(
            "You are an AI assistant with access to file operations and model switching capabilities. \
    You are currently running as {}. You can switch to other models when appropriate:\n\
    - kimi (Kimi-K2-Instruct-0905): Good for general tasks, coding, and quick responses\n\
    - gpt-oss (GPT-OSS-120B): Good for complex reasoning, analysis, and advanced problem-solving\n\
    Available tools (use ONLY these exact names):\n\
    - read_file: Read file contents\n\
    - write_file: Write/create a file\n\
    - edit_file: Edit existing file by replacing content\n\
    - list_files: List files (single-level patterns only, no **)\n\
    - switch_model: Switch between models\n\
    - my_tool: Your tool description here\n\
    IMPORTANT: Only use the exact tool names listed above. Do not make up tool names.",
            self.current_model.display_name()
        );
    }
}
```

## Example: Adding a File Stat Tool

Here's a complete example of adding a file stat tool:

### 1. Define Parameter Struct

```rust
#[derive(Debug, Deserialize)]
struct FileStatArgs {
    file_path: String,
}
```

### 2. Add Tool Definition

```rust
fn get_tools() -> Vec<Tool> {
    vec![
        // ... existing tools ...
        Tool {
            tool_type: "function".to_string(),
            function: FunctionDef {
                name: "file_stat".to_string(),
                description: "Get file statistics and metadata".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file"
                        }
                    },
                    "required": ["file_path"]
                }),
            },
        },
    ]
}
```

### 3. Implement Tool

```rust
impl KimiChat {
    fn file_stat(&self, file_path: &str) -> Result<String> {
        let full_path = self.work_dir.join(file_path);
        
        match fs::metadata(&full_path) {
            Ok(metadata) => {
                Ok(format!(
                    "File: {}\n\
                    Size: {} bytes\n\
                    Modified: {:?}\n\
                    Permissions: {:?}",
                    file_path,
                    metadata.len(),
                    metadata.modified(),
                    metadata.permissions()
                ))
            }
            Err(e) => {
                Err(anyhow::anyhow!("Failed to stat file: {}", e))
            }
        }
    }
}
```

### 4. Update execute_tool

```rust
async fn execute_tool(&mut self, name: &str, arguments: &str) -> Result<String> {
    match name {
        // ... existing cases ...
        "file_stat" => {
            let args: FileStatArgs = serde_json::from_str(arguments)?
            self.file_stat(&args.file_path)
        }
        // ... other cases ...
    }
}
```

## Important Considerations

### 1. Error Handling

Always include proper error handling:

```rust
fn my_tool(&self, parameter: &str) -> Result<String> {
    if parameter.is_empty() {
        anyhow::bail!("Parameter cannot be empty");
    }
    
    // Implementation here
    Ok(format!("Tool executed with parameter: {}", parameter))
}
```

### 2. Safety Checks

Include safety checks for file operations:

```rust
fn file_stat(&self, file_path: &str) -> Result<String> {
    // Validate file path
    if file_path.contains("..") {
        anyhow::bail!("Invalid file path");
    }
    
    // Check if file exists
    let full_path = self.work_dir.join(file_path);
    if !full_path.exists() {
        anyhow::bail!("File not found: {}", file_path);
    }
    
    // Proceed with operation
    // ...
}
```

### 3. Async Operations

For async operations, use appropriate async patterns:

```rust
async fn my_async_tool(&self, parameter: &str) -> Result<String> {
    // Use async operations
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Return result
    Ok(format!("Async tool executed with parameter: {}", parameter))
}
```
```