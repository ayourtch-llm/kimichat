# current_cost() Function Specification

## Overview

Implement a function `current_cost() -> f32` that returns the current billing amount from Groq API based on usage statistics.

## Function Signature

```rust
pub fn current_cost() -> f32
```

## Requirements

- Return the current billing amount in USD as f32
- Fetch directly from Groq API usage statistics
- Include all models (Kimi-K2-Instruct and GPT-OSS)
- Handle API errors gracefully
- Return 0.0 if no usage data available

## Implementation Details

### Data Sources
- Groq API usage statistics endpoint
- Token usage per model
- Current billing amount from Groq
- Conversation history

### Pricing
- Direct from Groq API pricing data
- Kimi-K2-Instruct: $0.01 per 1000 tokens
- GPT-OSS: $0.02 per 1000 tokens
- Input tokens: 1x rate
- Output tokens: 1x rate

### Error Handling
- API connection failures
- Invalid usage data
- Missing pricing information
- Timeout scenarios

## API Integration

The function should make a direct API call to Groq's usage/billing endpoint to get the current cost. This ensures accuracy rather than calculating based on local token usage.

## Usage Example

```rust
let cost = current_cost();
println!("Current billing: ${:.2}", cost);
```