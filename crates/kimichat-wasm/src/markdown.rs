use pulldown_cmark::{html, Options, Parser};

/// Render markdown to HTML
pub fn render_markdown(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);

    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    // Post-process to add syntax highlighting classes
    // We'll use hljs classes that match highlight.js
    add_code_highlighting_classes(&html_output)
}

/// Add code highlighting classes to code blocks
fn add_code_highlighting_classes(html: &str) -> String {
    // Simple replacement to add hljs class to code blocks
    // This is a simplified version - a more robust implementation would use
    // proper HTML parsing
    html.replace("<pre><code", "<pre><code class=\"hljs\"")
        .replace("<code>", "<code class=\"hljs\">")
}

/// Extract language from code fence
pub fn extract_code_language(info: &str) -> Option<String> {
    info.split_whitespace().next().map(|s| s.to_string())
}

/// Render a message with markdown support
pub fn render_message_content(content: &str, use_markdown: bool) -> String {
    if use_markdown {
        render_markdown(content)
    } else {
        crate::utils::escape_html(content).replace('\n', "<br>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_markdown() {
        let md = "# Hello\n\nThis is **bold** text.";
        let html = render_markdown(md);
        assert!(html.contains("<h1>"));
        assert!(html.contains("<strong>"));
    }

    #[test]
    fn test_render_code_block() {
        let md = "```rust\nfn main() {}\n```";
        let html = render_markdown(md);
        assert!(html.contains("<pre>"));
        assert!(html.contains("<code"));
    }
}
