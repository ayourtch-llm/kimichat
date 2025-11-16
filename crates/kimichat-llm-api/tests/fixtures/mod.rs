use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::*;
use serde_json::json;
use kimichat_models::llm_api::{LLMRequest, LLMResponse, Message, Role};

/// Mock server utilities for testing LLM API clients
pub struct LLMMockServer {
    server: MockServer,
}

impl LLMMockServer {
    pub async fn new() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    pub fn uri(&self) -> &str {
        self.server.uri()
    }

    /// Mock successful Anthropic API response
    pub async fn mock_anthropic_success(&self, request_content: &str, response_content: &str) {
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-api-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(body_json_partial(json!({
                "model": "claude-3-sonnet-20240229",
                "messages": [{
                    "role": "user",
                    "content": request_content
                }],
                "max_tokens": 4096
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "msg_test123",
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "text",
                    "text": response_content
                }],
                "model": "claude-3-sonnet-20240229",
                "stop_reason": "end_turn",
                "stop_sequence": null,
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 20
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock Anthropic API error response
    pub async fn mock_anthropic_error(&self, error_type: &str, error_message: &str) {
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "type": "error",
                "error": {
                    "type": error_type,
                    "message": error_message
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock Anthropic API rate limit error
    pub async fn mock_anthropic_rate_limit(&self) {
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(429).set_body_json(json!({
                "type": "error",
                "error": {
                    "type": "rate_limit_error",
                    "message": "Rate limit exceeded"
                }
            })).insert_header("Retry-After", "60"))
            .mount(&self.server)
            .await;
    }

    /// Mock successful Groq API response
    pub async fn mock_groq_success(&self, request_content: &str, response_content: &str) {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test-api-key"))
            .and(body_json_partial(json!({
                "model": "mixtral-8x7b-32768",
                "messages": [{
                    "role": "user",
                    "content": request_content
                }],
                "max_tokens": 4096
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl_test123",
                "object": "chat.completion",
                "created": 1700000000,
                "model": "mixtral-8x7b-32768",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": response_content
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 20,
                    "total_tokens": 30
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock successful llama.cpp API response
    pub async fn mock_llamacpp_success(&self, request_content: &str, response_content: &str) {
        Mock::given(method("POST"))
            .and(path("/completion"))
            .and(body_json_partial(json!({
                "prompt": request_content,
                "n_predict": 4096
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": response_content,
                "stop": true,
                "model": "test-model",
                "tokens_predicted": 20,
                "tokens_evaluated": 10,
                "generation_settings": {
                    "frequency_penalty": 0.0,
                    "presence_penalty": 0.0,
                    "temperature": 0.7,
                    "top_p": 0.9
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock llama.cpp streaming response
    pub async fn mock_llamacpp_streaming(&self, response_chunks: Vec<&str>) {
        for (index, chunk) in response_chunks.iter().enumerate() {
            Mock::given(method("POST"))
                .and(path("/completion"))
                .and(header("accept", "text/event-stream"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "content": chunk,
                    "stop": index == response_chunks.len() - 1,
                    "slot_id": 0
                })))
                .mount(&self.server)
                .await;
        }
    }

    /// Mock server error
    pub async fn mock_server_error(&self) {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "error": "Internal server error"
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock network timeout
    pub async fn mock_timeout(&self) {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(408).set_body_delay(std::time::Duration::from_secs(30)))
            .mount(&self.server)
            .await;
    }
}

/// Test data generators for LLM API testing
pub mod test_data {
    use super::*;

    /// Create a sample LLM request
    pub fn create_sample_request(content: &str) -> LLMRequest {
        LLMRequest {
            model: "claude-3-sonnet-20240229".to_string(),
            messages: vec![
                Message {
                    role: Role::User,
                    content: content.to_string(),
                }
            ],
            max_tokens: Some(4096),
            temperature: Some(0.7),
            top_p: None,
            stream: false,
        }
    }

    /// Create a sample LLM response
    pub fn create_sample_response(content: &str) -> LLMResponse {
        LLMResponse {
            id: "test_response_123".to_string(),
            model: "claude-3-sonnet-20240229".to_string(),
            content: content.to_string(),
            usage: kimichat_models::llm_api::Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            },
            finish_reason: Some("stop".to_string()),
        }
    }

    /// Create a complex conversation
    pub fn create_complex_conversation() -> Vec<Message> {
        vec![
            Message {
                role: Role::System,
                content: "You are a helpful AI assistant.".to_string(),
            },
            Message {
                role: Role::User,
                content: "What is Rust programming?".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "Rust is a systems programming language focused on safety and performance.".to_string(),
            },
            Message {
                role: Role::User,
                content: "Can you show me a simple example?".to_string(),
            },
        ]
    }

    /// Various test cases for different request types
    pub fn test_requests() -> Vec<LLMRequest> {
        vec![
            create_sample_request("Hello, world!"),
            create_sample_request("Explain quantum computing"),
            create_sample_request("Write a Rust function that sorts an array"),
            LLMRequest {
                model: "mixtral-8x7b-32768".to_string(),
                messages: create_complex_conversation(),
                max_tokens: Some(1024),
                temperature: Some(0.5),
                top_p: Some(0.9),
                stream: false,
            },
        ]
    }

    /// Error scenarios for testing
    pub mod error_scenarios {
        use serde_json::json;

        pub fn authentication_error() -> serde_json::Value {
            json!({
                "type": "error",
                "error": {
                    "type": "authentication_error",
                    "message": "Invalid API key"
                }
            })
        }

        pub fn rate_limit_error() -> serde_json::Value {
            json!({
                "type": "error",
                "error": {
                    "type": "rate_limit_error",
                    "message": "Rate limit exceeded. Try again later."
                }
            })
        }

        pub fn invalid_request_error() -> serde_json::Value {
            json!({
                "type": "error",
                "error": {
                    "type": "invalid_request_error",
                    "message": "Invalid request parameters"
                }
            })
        }

        pub fn server_error() -> serde_json::Value {
            json!({
                "type": "error",
                "error": {
                    "type": "server_error",
                    "message": "Internal server error"
                }
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server_creation() {
        let mock_server = LLMMockServer::new().await;
        assert!(!mock_server.uri().is_empty());
    }

    #[test]
    fn test_test_data_creation() {
        let request = test_data::create_sample_request("Test message");
        assert_eq!(request.model, "claude-3-sonnet-20240229");
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].content, "Test message");
    }

    #[test]
    fn test_complex_conversation() {
        let conversation = test_data::create_complex_conversation();
        assert_eq!(conversation.len(), 4);
        assert!(matches!(conversation[0].role, Role::System));
        assert!(matches!(conversation[1].role, Role::User));
        assert!(matches!(conversation[2].role, Role::Assistant));
        assert!(matches!(conversation[3].role, Role::User));
    }

    #[test]
    fn test_error_scenarios() {
        let auth_error = test_data::error_scenarios::authentication_error();
        assert!(auth_error["error"]["type"].is_string());
        assert_eq!(auth_error["error"]["type"], "authentication_error");
    }
}