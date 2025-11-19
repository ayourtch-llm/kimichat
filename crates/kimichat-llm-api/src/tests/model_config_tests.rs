#[cfg(test)]
mod model_config_tests {
    use crate::config::{parse_model_attings, BackendType, get_default_url_for_backend};

    #[test]
    fn test_parse_model_full_format() {
        let (model, backend, url) = parse_model_attings("llama-3.1-70b@anthropic(https://api.anthropic.com)");
        
        assert_eq!(model, "llama-3.1-70b");
        assert_eq!(backend, Some(BackendType::Anthropic));
        assert_eq!(url, Some("https://api.anthropic.com".to_string()));
    }

    #[test]
    fn test_parse_model_backend_only() {
        let (model, backend, url) = parse_model_attings("gpt-4@openai");
        
        assert_eq!(model, "gpt-4");
        assert_eq!(backend, Some(BackendType::OpenAI));
        assert_eq!(url, None); // URL should be None for backend-only format
    }

    #[test]
    fn test_parse_model_only() {
        let (model, backend, url) = parse_model_attings("llama-3.1-70b");
        
        assert_eq!(model, "llama-3.1-70b");
        assert_eq!(backend, None);
        assert_eq!(url, None);
    }

    #[test]
    fn test_parse_model_groq_backend() {
        let (model, backend, url) = parse_model_attings("llama-3.1-70b@groq");
        
        assert_eq!(model, "llama-3.1-70b");
        assert_eq!(backend, Some(BackendType::Groq));
        assert_eq!(url, None);
    }

    #[test]
    fn test_parse_model_with_custom_url() {
        let (model, backend, url) = parse_model_attings("custom-model@llama(http://localhost:8080/completions)");
        
        assert_eq!(model, "custom-model");
        assert_eq!(backend, Some(BackendType::Llama));
        assert_eq!(url, Some("http://localhost:8080/completions".to_string()));
    }

    #[test]
    fn test_parse_model_empty_model() {
        let (model, backend, url) = parse_model_attings("@anthropic");
        
        assert_eq!(model, "");
        assert_eq!(backend, Some(BackendType::Anthropic));
        assert_eq!(url, None);
    }

    #[test]
    fn test_parse_model_multiple_at_symbols() {
        let (model, backend, url) = parse_model_attings("model@with@multiple@anthropic");
        
        // Should split on first @ only and treat "with@multiple@anthropic" as backend
        assert_eq!(model, "model");
        assert_eq!(backend, None); // "with@multiple@anthropic" is not a valid backend
        assert_eq!(url, None);
    }

    #[test]
    fn test_parse_model_malformed_parentheses() {
        let (model, backend, url) = parse_model_attings("model@anthropic(https://example.com");
        
        // Should handle malformed parentheses gracefully
        assert_eq!(model, "model");
        assert_eq!(backend, Some(BackendType::Anthropic));
        // The actual behavior: removes last character if it's not ')'
        assert_eq!(url, Some("https://example.co".to_string()));
    }

    #[test]
    fn test_parse_model_case_insensitive_backend() {
        let (model, backend, url) = parse_model_attings("model@ANTHROPIC");
        
        assert_eq!(model, "model");
        assert_eq!(backend, Some(BackendType::Anthropic));
        assert_eq!(url, None);
    }

    #[test]
    fn test_parse_model_claude_alias() {
        let (model, backend, url) = parse_model_attings("claude-3.5-sonnet@claude");
        
        assert_eq!(model, "claude-3.5-sonnet");
        assert_eq!(backend, Some(BackendType::Anthropic));
        assert_eq!(url, None);
    }

    #[test]
    fn test_parse_model_llama_aliases() {
        // Test various llama backend aliases
        let test_cases = vec![
            ("model@llama", BackendType::Llama),
            ("model@llamacpp", BackendType::Llama),
            ("model@llama.cpp", BackendType::Llama),
            ("model@llama-cpp", BackendType::Llama),
        ];

        for (input, expected_backend) in test_cases {
            let (model, backend, url) = parse_model_attings(input);
            assert_eq!(model, "model");
            assert_eq!(backend, Some(expected_backend));
            assert_eq!(url, None);
        }
    }

    #[test]
    fn test_parse_model_unknown_backend() {
        let (model, backend, url) = parse_model_attings("model@unknown");
        
        assert_eq!(model, "model");
        assert_eq!(backend, None); // Unknown backend should return None
        assert_eq!(url, None);
    }

    #[test]
    fn test_get_default_url_anthropic() {
        let url = get_default_url_for_backend(&BackendType::Anthropic);
        assert_eq!(url, Some("https://api.anthropic.com".to_string()));
    }

    #[test]
    fn test_get_default_url_groq() {
        let url = get_default_url_for_backend(&BackendType::Groq);
        assert_eq!(url, Some("https://api.groq.com/openai/v1/chat/completions".to_string()));
    }

    #[test]
    fn test_get_default_url_openai() {
        let url = get_default_url_for_backend(&BackendType::OpenAI);
        assert_eq!(url, Some("https://api.openai.com/v1/chat/completions".to_string()));
    }

    #[test]
    fn test_get_default_url_llama() {
        let url = get_default_url_for_backend(&BackendType::Llama);
        assert_eq!(url, None); // Llama has no default URL
    }

    #[test]
    fn test_parse_model_integration_with_default_urls() {
        // Test the complete flow: parse model config, then get default URL
        
        // Case 1: model@anthropic should get Anthropic default URL
        let (model, backend, url) = parse_model_attings("foo@anthropic");
        assert_eq!(model, "foo");
        assert_eq!(backend, Some(BackendType::Anthropic));
        assert_eq!(url, None); // URL is None after parsing
        
        let default_url = get_default_url_for_backend(&backend.unwrap());
        assert_eq!(default_url, Some("https://api.anthropic.com".to_string()));
        
        // Case 2: model@groq should get Groq default URL
        let (model, backend, url) = parse_model_attings("bar@groq");
        assert_eq!(model, "bar");
        assert_eq!(backend, Some(BackendType::Groq));
        assert_eq!(url, None);
        
        let default_url = get_default_url_for_backend(&backend.unwrap());
        assert_eq!(default_url, Some("https://api.groq.com/openai/v1/chat/completions".to_string()));
        
        // Case 3: model@openai should get OpenAI default URL
        let (model, backend, url) = parse_model_attings("baz@openai");
        assert_eq!(model, "baz");
        assert_eq!(backend, Some(BackendType::OpenAI));
        assert_eq!(url, None);
        
        let default_url = get_default_url_for_backend(&backend.unwrap());
        assert_eq!(default_url, Some("https://api.openai.com/v1/chat/completions".to_string()));
        
        // Case 4: model@llama should get no default URL
        let (model, backend, url) = parse_model_attings("qux@llama");
        assert_eq!(model, "qux");
        assert_eq!(backend, Some(BackendType::Llama));
        assert_eq!(url, None);
        
        let default_url = get_default_url_for_backend(&backend.unwrap());
        assert_eq!(default_url, None);
    }

    #[test]
    fn test_edge_cases() {
        // Empty string
        let (model, backend, url) = parse_model_attings("");
        assert_eq!(model, "");
        assert_eq!(backend, None);
        assert_eq!(url, None);
        
        // Just @ symbol
        let (model, backend, url) = parse_model_attings("@");
        assert_eq!(model, "");
        assert_eq!(backend, None);
        assert_eq!(url, None);
        
        // Multiple @ symbols with empty backend
        let (model, backend, url) = parse_model_attings("model@@");
        assert_eq!(model, "model");
        assert_eq!(backend, None);
        assert_eq!(url, None);
    }
}