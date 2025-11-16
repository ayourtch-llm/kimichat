use std::path::PathBuf;
use tempfile::TempDir;
use serde_json::json;

/// Test fixture data for tools testing
pub struct ToolTestFixtures {
    pub temp_dir: TempDir,
}

impl ToolTestFixtures {
    pub fn new() -> Self {
        Self {
            temp_dir: TempDir::new().expect("Failed to create temp directory"),
        }
    }

    /// Create a complex project structure for testing file operations
    pub fn create_project_structure(&self) -> PathBuf {
        let project_root = self.temp_dir.path().join("test_project");
        std::fs::create_dir_all(&project_root).unwrap();

        // Create src directory
        let src_dir = project_root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        // Create main.rs
        std::fs::write(src_dir.join("main.rs"), r#"fn main() {
    println!("Hello, world!");
}"#).unwrap();

        // Create lib.rs
        std::fs::write(src_dir.join("lib.rs"), r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 2), 4);
    }
}"#).unwrap();

        // Create tests directory
        let tests_dir = project_root.join("tests");
        std::fs::create_dir_all(&tests_dir).unwrap();

        // Create integration test
        std::fs::write(tests_dir.join("integration_test.rs"), r#"use your_crate::add;

#[test]
fn test_integration() {
    assert_eq!(add(1, 1), 2);
}"#).unwrap();

        // Create Cargo.toml
        std::fs::write(project_root.join("Cargo.toml"), r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
"#).unwrap();

        // Create .gitignore
        std::fs::write(project_root.join(".gitignore"), r#"target/
Cargo.lock
*.log
"#).unwrap();

        project_root
    }

    /// Create sample files with various content types
    pub fn create_sample_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();

        // Create a JSON file
        let json_path = self.temp_dir.path().join("config.json");
        std::fs::write(&json_path, json!({
            "name": "test_project",
            "version": "1.0.0",
            "dependencies": ["serde", "tokio"]
        }).to_string()).unwrap();
        files.push(json_path);

        // Create a markdown file
        let md_path = self.temp_dir.path().join("README.md");
        std::fs::write(&md_path, r#"# Test Project

This is a test project for testing file operations.

## Features

- File reading
- File writing
- Search functionality

## Usage

```rust
use test_project::main;

fn main() {
    main();
}
```
"#).unwrap();
        files.push(md_path);

        // Create a large text file
        let large_path = self.temp_dir.path().join("large_file.txt");
        let large_content = "This is a line that will be repeated many times.\n".repeat(1000);
        std::fs::write(&large_path, large_content).unwrap();
        files.push(large_path);

        // Create a binary file (simulated)
        let bin_path = self.temp_dir.path().join("binary.dat");
        let binary_data: Vec<u8> = (0..255).cycle().take(1024).collect();
        std::fs::write(&bin_path, binary_data).unwrap();
        files.push(bin_path);

        files
    }

    /// Create files for search testing
    pub fn create_search_test_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();

        // Create files with specific content for search tests
        let search_files = [
            ("rust_code.rs", r#"fn main() {
    let x = 42;
    println!("The answer is {}", x);
}

fn helper_function() {
    println!("This is a helper");
}
"#),
            ("python_code.py", r#"def main():
    x = 42
    print(f"The answer is {x}")

def helper_function():
    print("This is a helper")
"#),
            ("javascript_code.js", r#"function main() {
    const x = 42;
    console.log(`The answer is ${x}`);
}

function helperFunction() {
    console.log("This is a helper");
}
"#),
            ("config.yaml", r#"app:
  name: test_app
  version: 1.0
  settings:
    debug: true
    port: 8080
"#),
            ("documentation.md", r#"# API Documentation

## Main Function

The main function prints the answer.

## Helper Functions

Helper functions provide utility functionality.
"#),
        ];

        for (filename, content) in search_files.iter() {
            let path = self.temp_dir.path().join(filename);
            std::fs::write(&path, content).unwrap();
            files.push(path);
        }

        files
    }

    /// Get the temporary directory path
    pub fn temp_dir_path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }
}

/// Sample data for tool parameter testing
pub mod parameter_data {
    use serde_json::{json, Value};
    use kimichat_models::tools::{ToolParameter, ToolParameterType};

    pub fn valid_file_read_parameters() -> Vec<(String, Value)> {
        vec![
            ("file_path".to_string(), json!("/path/to/file.txt")),
            ("file_path".to_string(), json!("relative/path.txt")),
            ("file_path".to_string(), json!("./current/file.txt")),
            ("file_path".to_string(), json!("../parent/file.txt")),
        ]
    }

    pub fn invalid_file_read_parameters() -> Vec<(String, Value)> {
        vec![
            ("file_path".to_string(), json!(null)),
            ("file_path".to_string(), json!(123)),
            ("file_path".to_string(), json!([])),
            ("file_path".to_string(), json!({})),
            ("file_path".to_string(), json!("")),
        ]
    }

    pub fn valid_search_parameters() -> Vec<(String, Value)> {
        vec![
            ("query".to_string(), json!("search term")),
            ("query".to_string(), json!("function.*test")),
            ("pattern".to_string(), json!("**/*.rs")),
            ("case_sensitive".to_string(), json!(true)),
            ("case_sensitive".to_string(), json!(false)),
            ("max_results".to_string(), json!(50)),
        ]
    }

    pub fn tool_parameter_examples() -> Vec<ToolParameter> {
        vec![
            ToolParameter {
                name: "required_string".to_string(),
                param_type: ToolParameterType::String,
                required: true,
                description: "A required string parameter".to_string(),
                default_value: None,
            },
            ToolParameter {
                name: "optional_boolean".to_string(),
                param_type: ToolParameterType::Boolean,
                required: false,
                description: "An optional boolean parameter".to_string(),
                default_value: Some(json!(false)),
            },
            ToolParameter {
                name: "optional_number".to_string(),
                param_type: ToolParameterType::Number,
                required: false,
                description: "An optional number parameter".to_string(),
                default_value: Some(json!(42)),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_structure_creation() {
        let fixtures = ToolTestFixtures::new();
        let project_root = fixtures.create_project_structure();
        
        assert!(project_root.exists());
        assert!(project_root.join("src").exists());
        assert!(project_root.join("src/main.rs").exists());
        assert!(project_root.join("Cargo.toml").exists());
        assert!(project_root.join(".gitignore").exists());
    }

    #[test]
    fn test_sample_files_creation() {
        let fixtures = ToolTestFixtures::new();
        let files = fixtures.create_sample_files();
        
        assert_eq!(files.len(), 4);
        
        // Check that all files exist
        for file in files {
            assert!(file.exists(), "File {:?} should exist", file);
        }
    }

    #[test]
    fn test_search_test_files_creation() {
        let fixtures = ToolTestFixtures::new();
        let files = fixtures.create_search_test_files();
        
        assert_eq!(files.len(), 5);
        
        // Check specific files
        let rust_file = files.iter().find(|f| f.ends_with("rust_code.rs")).unwrap();
        let content = std::fs::read_to_string(rust_file).unwrap();
        assert!(content.contains("fn main()"));
    }
}