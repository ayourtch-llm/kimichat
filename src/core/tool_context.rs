use std::path::PathBuf;
use std::collections::HashMap;

/// Tool execution context
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub work_dir: PathBuf,
    pub session_id: String,
    pub environment: HashMap<String, String>,
}

impl ToolContext {
    pub fn new(work_dir: PathBuf, session_id: String) -> Self {
        Self {
            work_dir,
            session_id,
            environment: HashMap::new(),
        }
    }

    pub fn with_env(mut self, key: String, value: String) -> Self {
        self.environment.insert(key, value);
        self
    }
}