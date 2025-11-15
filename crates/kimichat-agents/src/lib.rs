//! Multi-agent orchestration system for kimichat
//!
//! This crate provides the agent system including planning coordinator,
//! agent implementations, and LLM client trait.

pub mod agent;
pub mod agent_config;
pub mod agent_factory;
pub mod coordinator;
pub mod task;
pub mod groq_client;
pub mod llama_cpp_client;
pub mod anthropic_client;
pub mod progress_evaluator;
pub mod visibility;
pub mod embedded_configs;

// Re-export commonly used types
pub use agent::*;
pub use agent_config::*;
pub use agent_factory::*;
pub use coordinator::*;
pub use task::*;
pub use groq_client::*;
pub use llama_cpp_client::*;
pub use anthropic_client::*;
pub use progress_evaluator::*;
pub use visibility::*;
