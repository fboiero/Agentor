//! Vision-capable wrappers for Claude, OpenAI (GPT-4o) and Gemini.
//!
//! Each wrapper exposes a static `build_messages_payload()` that produces the
//! provider-specific JSON body for a [`MultimodalMessage`]. This is pure JSON
//! construction — no HTTP is performed — so the payload builders can be unit
//! tested and reused by higher-level clients.
//!
//! The [`VisionBackend::ask_with_image`] implementations intentionally return a
//! stub placeholder right now. Real HTTP wiring (if desired) can be added
//! behind a feature flag without changing the public surface.

use crate::multimodal::{ImageInput, MultimodalMessage, VisionBackend, VisionCapability};
use argentor_core::ArgentorResult;
use async_trait::async_trait;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Claude
// ---------------------------------------------------------------------------

/// Claude vision backend — wraps the Anthropic Messages API with multimodal
/// content blocks.
///
/// Claude accepts an array of content blocks per message. Each block is either
/// `{"type": "text", "text": "..."}` or `{"type": "image", "source": {...}}`.
/// URL sources use `{"type": "url", "url": "..."}` and inline data uses
/// `{"type": "base64", "media_type": "...", "data": "..."}`.
pub struct ClaudeVisionBackend {
    api_key: String,
    model_id: String,
    api_base_url: String,
}

impl ClaudeVisionBackend {
    /// Construct a new Claude vision backend.
    pub fn new(api_key: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model_id: model_id.into(),
            api_base_url: "https://api.anthropic.com".to_string(),
        }
    }

    /// Override the API base URL (useful for proxies or testing).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.api_base_url = url.into();
        self
    }

    /// Get the model ID that will be used for requests.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Get the configured API base URL.
    pub fn api_base_url(&self) -> &str {
        &self.api_base_url
    }

    /// Get the configured API key. Useful for tests and for callers that
    /// need to build requests manually.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Build the Claude messages payload with image content blocks.
    ///
    /// Returns a JSON object of the form:
    /// ```json
    /// {
    ///   "role": "user",
    ///   "content": [
    ///     { "type": "text", "text": "..." },
    ///     { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "..." } }
    ///   ]
    /// }
    /// ```
    pub fn build_messages_payload(message: &MultimodalMessage) -> Value {
        let mut content: Vec<Value> = Vec::with_capacity(1 + message.images.len());
        content.push(json!({ "type": "text", "text": message.text }));

        for img in &message.images {
            let block = match img {
                ImageInput::Url(url) => json!({
                    "type": "image",
                    "source": { "type": "url", "url": url },
                }),
                ImageInput::Base64 { media_type, data } => json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": data,
                    },
                }),
            };
            content.push(block);
        }

        json!({ "role": "user", "content": content })
    }
}

#[async_trait]
impl VisionBackend for ClaudeVisionBackend {
    fn vision_capability(&self) -> VisionCapability {
        VisionCapability::Full
    }

    fn provider_name(&self) -> &str {
        "claude"
    }

    async fn ask_with_image(&self, message: &MultimodalMessage) -> ArgentorResult<String> {
        Ok(format!(
            "[claude-vision-stub] would process {} image(s) with text: {}",
            message.image_count(),
            message.text
        ))
    }
}

// ---------------------------------------------------------------------------
// OpenAI (GPT-4o / gpt-4-vision-preview)
// ---------------------------------------------------------------------------

/// OpenAI vision backend — wraps the chat completions API with
/// multimodal content parts.
///
/// OpenAI accepts a content array where each entry is either
/// `{"type": "text", "text": "..."}` or
/// `{"type": "image_url", "image_url": {"url": "..."}}`. The `url` may be a
/// public http(s) URL or a `data:` URI with inlined base64.
pub struct OpenAiVisionBackend {
    api_key: String,
    model_id: String,
    api_base_url: String,
}

impl OpenAiVisionBackend {
    /// Construct a new OpenAI vision backend.
    pub fn new(api_key: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model_id: model_id.into(),
            api_base_url: "https://api.openai.com".to_string(),
        }
    }

    /// Override the API base URL (useful for proxies or testing).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.api_base_url = url.into();
        self
    }

    /// Get the model ID that will be used for requests.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Get the configured API base URL.
    pub fn api_base_url(&self) -> &str {
        &self.api_base_url
    }

    /// Get the configured API key.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Build the OpenAI messages payload with image_url parts.
    ///
    /// Returns a JSON object of the form:
    /// ```json
    /// {
    ///   "role": "user",
    ///   "content": [
    ///     { "type": "text", "text": "..." },
    ///     { "type": "image_url", "image_url": { "url": "data:image/png;base64,..." } }
    ///   ]
    /// }
    /// ```
    pub fn build_messages_payload(message: &MultimodalMessage) -> Value {
        let mut content: Vec<Value> = Vec::with_capacity(1 + message.images.len());
        content.push(json!({ "type": "text", "text": message.text }));

        for img in &message.images {
            let url = match img {
                ImageInput::Url(u) => u.clone(),
                ImageInput::Base64 { media_type, data } => {
                    format!("data:{media_type};base64,{data}")
                }
            };
            content.push(json!({
                "type": "image_url",
                "image_url": { "url": url },
            }));
        }

        json!({ "role": "user", "content": content })
    }
}

#[async_trait]
impl VisionBackend for OpenAiVisionBackend {
    fn vision_capability(&self) -> VisionCapability {
        VisionCapability::Full
    }

    fn provider_name(&self) -> &str {
        "openai"
    }

    async fn ask_with_image(&self, message: &MultimodalMessage) -> ArgentorResult<String> {
        Ok(format!(
            "[openai-vision-stub] would process {} image(s) with text: {}",
            message.image_count(),
            message.text
        ))
    }
}

// ---------------------------------------------------------------------------
// Gemini (gemini-2.0-flash and family)
// ---------------------------------------------------------------------------

/// Gemini vision backend — wraps Google's `generateContent` API with inline
/// image parts.
///
/// Gemini accepts a `contents` array where each entry has a `parts` list. A
/// part can be `{"text": "..."}`, `{"inline_data": {"mime_type": "...",
/// "data": "..."}}`, or `{"file_data": {"mime_type": "...", "file_uri":
/// "..."}}` for URL-hosted files.
pub struct GeminiVisionBackend {
    api_key: String,
    model_id: String,
    api_base_url: String,
}

impl GeminiVisionBackend {
    /// Construct a new Gemini vision backend.
    pub fn new(api_key: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model_id: model_id.into(),
            api_base_url: "https://generativelanguage.googleapis.com".to_string(),
        }
    }

    /// Override the API base URL (useful for proxies or testing).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.api_base_url = url.into();
        self
    }

    /// Get the model ID that will be used for requests.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Get the configured API base URL.
    pub fn api_base_url(&self) -> &str {
        &self.api_base_url
    }

    /// Get the configured API key.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Build the Gemini contents payload with inline image parts.
    ///
    /// Returns a JSON object of the form:
    /// ```json
    /// {
    ///   "contents": [{
    ///     "parts": [
    ///       { "text": "..." },
    ///       { "inline_data": { "mime_type": "image/png", "data": "..." } }
    ///     ]
    ///   }]
    /// }
    /// ```
    ///
    /// URL-only images are attached as `file_data` parts since Gemini does
    /// not fetch arbitrary HTTP(S) URLs the same way OpenAI does.
    pub fn build_messages_payload(message: &MultimodalMessage) -> Value {
        let mut parts: Vec<Value> = Vec::with_capacity(1 + message.images.len());
        parts.push(json!({ "text": message.text }));

        for img in &message.images {
            let part = match img {
                ImageInput::Base64 { media_type, data } => json!({
                    "inline_data": {
                        "mime_type": media_type,
                        "data": data,
                    }
                }),
                ImageInput::Url(url) => json!({
                    "file_data": {
                        "mime_type": "image/*",
                        "file_uri": url,
                    }
                }),
            };
            parts.push(part);
        }

        json!({ "contents": [{ "parts": parts }] })
    }
}

#[async_trait]
impl VisionBackend for GeminiVisionBackend {
    fn vision_capability(&self) -> VisionCapability {
        VisionCapability::Full
    }

    fn provider_name(&self) -> &str {
        "gemini"
    }

    async fn ask_with_image(&self, message: &MultimodalMessage) -> ArgentorResult<String> {
        Ok(format!(
            "[gemini-vision-stub] would process {} image(s) with text: {}",
            message.image_count(),
            message.text
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multimodal::MultimodalMessage;

    // ---- Constructors & accessors ----

    #[test]
    fn claude_backend_accessors() {
        let b = ClaudeVisionBackend::new("key-123", "claude-sonnet-4-20250514");
        assert_eq!(b.api_key(), "key-123");
        assert_eq!(b.model_id(), "claude-sonnet-4-20250514");
        assert_eq!(b.api_base_url(), "https://api.anthropic.com");
    }

    #[test]
    fn claude_backend_with_base_url() {
        let b = ClaudeVisionBackend::new("k", "m").with_base_url("http://localhost:9000");
        assert_eq!(b.api_base_url(), "http://localhost:9000");
    }

    #[test]
    fn openai_backend_accessors() {
        let b = OpenAiVisionBackend::new("sk-xxx", "gpt-4o");
        assert_eq!(b.api_key(), "sk-xxx");
        assert_eq!(b.model_id(), "gpt-4o");
        assert_eq!(b.api_base_url(), "https://api.openai.com");
    }

    #[test]
    fn openai_backend_with_base_url() {
        let b = OpenAiVisionBackend::new("k", "gpt-4o").with_base_url("http://x");
        assert_eq!(b.api_base_url(), "http://x");
    }

    #[test]
    fn gemini_backend_accessors() {
        let b = GeminiVisionBackend::new("AIza", "gemini-2.0-flash");
        assert_eq!(b.api_key(), "AIza");
        assert_eq!(b.model_id(), "gemini-2.0-flash");
        assert_eq!(
            b.api_base_url(),
            "https://generativelanguage.googleapis.com"
        );
    }

    #[test]
    fn gemini_backend_with_base_url() {
        let b = GeminiVisionBackend::new("k", "m").with_base_url("http://g");
        assert_eq!(b.api_base_url(), "http://g");
    }

    // ---- Capability declarations ----

    #[test]
    fn claude_capability_full() {
        let b = ClaudeVisionBackend::new("k", "m");
        assert_eq!(b.vision_capability(), VisionCapability::Full);
        assert_eq!(b.provider_name(), "claude");
    }

    #[test]
    fn openai_capability_full() {
        let b = OpenAiVisionBackend::new("k", "m");
        assert_eq!(b.vision_capability(), VisionCapability::Full);
        assert_eq!(b.provider_name(), "openai");
    }

    #[test]
    fn gemini_capability_full() {
        let b = GeminiVisionBackend::new("k", "m");
        assert_eq!(b.vision_capability(), VisionCapability::Full);
        assert_eq!(b.provider_name(), "gemini");
    }

    // ---- Claude payload building ----

    #[test]
    fn claude_payload_text_only() {
        let m = MultimodalMessage::new("hello");
        let p = ClaudeVisionBackend::build_messages_payload(&m);
        assert_eq!(p["role"], "user");
        assert_eq!(p["content"].as_array().unwrap().len(), 1);
        assert_eq!(p["content"][0]["type"], "text");
        assert_eq!(p["content"][0]["text"], "hello");
    }

    #[test]
    fn claude_payload_with_url_image() {
        let m = MultimodalMessage::new("q").with_image_url("https://x/a.png");
        let p = ClaudeVisionBackend::build_messages_payload(&m);
        let content = p["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["type"], "url");
        assert_eq!(content[1]["source"]["url"], "https://x/a.png");
    }

    #[test]
    fn claude_payload_with_base64_image() {
        let m = MultimodalMessage::new("q").with_image_base64("image/png", "AAAA");
        let p = ClaudeVisionBackend::build_messages_payload(&m);
        let content = p["content"].as_array().unwrap();
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["type"], "base64");
        assert_eq!(content[1]["source"]["media_type"], "image/png");
        assert_eq!(content[1]["source"]["data"], "AAAA");
    }

    #[test]
    fn claude_payload_multiple_images() {
        let m = MultimodalMessage::new("q")
            .with_image_url("https://a")
            .with_image_base64("image/jpeg", "BBBB")
            .with_image_url("https://c");
        let p = ClaudeVisionBackend::build_messages_payload(&m);
        let content = p["content"].as_array().unwrap();
        assert_eq!(content.len(), 4);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["source"]["type"], "url");
        assert_eq!(content[2]["source"]["type"], "base64");
        assert_eq!(content[3]["source"]["type"], "url");
    }

    #[test]
    fn claude_payload_text_preserved() {
        let m = MultimodalMessage::new("What is this?").with_image_url("https://x");
        let p = ClaudeVisionBackend::build_messages_payload(&m);
        assert_eq!(p["content"][0]["text"], "What is this?");
    }

    // ---- OpenAI payload building ----

    #[test]
    fn openai_payload_text_only() {
        let m = MultimodalMessage::new("hello");
        let p = OpenAiVisionBackend::build_messages_payload(&m);
        assert_eq!(p["role"], "user");
        assert_eq!(p["content"].as_array().unwrap().len(), 1);
        assert_eq!(p["content"][0]["type"], "text");
        assert_eq!(p["content"][0]["text"], "hello");
    }

    #[test]
    fn openai_payload_with_url_image() {
        let m = MultimodalMessage::new("q").with_image_url("https://x/a.png");
        let p = OpenAiVisionBackend::build_messages_payload(&m);
        let content = p["content"].as_array().unwrap();
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(content[1]["image_url"]["url"], "https://x/a.png");
    }

    #[test]
    fn openai_payload_base64_becomes_data_uri() {
        let m = MultimodalMessage::new("q").with_image_base64("image/png", "ABCDE");
        let p = OpenAiVisionBackend::build_messages_payload(&m);
        let url = p["content"][1]["image_url"]["url"].as_str().unwrap();
        assert_eq!(url, "data:image/png;base64,ABCDE");
    }

    #[test]
    fn openai_payload_base64_jpeg_data_uri() {
        let m = MultimodalMessage::new("q").with_image_base64("image/jpeg", "ZZZZ");
        let p = OpenAiVisionBackend::build_messages_payload(&m);
        let url = p["content"][1]["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
        assert!(url.ends_with("ZZZZ"));
    }

    #[test]
    fn openai_payload_multiple_images() {
        let m = MultimodalMessage::new("q")
            .with_image_url("https://a")
            .with_image_base64("image/png", "XX")
            .with_image_url("https://c");
        let p = OpenAiVisionBackend::build_messages_payload(&m);
        let content = p["content"].as_array().unwrap();
        assert_eq!(content.len(), 4);
        assert_eq!(content[1]["image_url"]["url"], "https://a");
        assert_eq!(content[2]["image_url"]["url"], "data:image/png;base64,XX");
        assert_eq!(content[3]["image_url"]["url"], "https://c");
    }

    // ---- Gemini payload building ----

    #[test]
    fn gemini_payload_text_only() {
        let m = MultimodalMessage::new("hi");
        let p = GeminiVisionBackend::build_messages_payload(&m);
        let parts = p["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["text"], "hi");
    }

    #[test]
    fn gemini_payload_with_base64_image() {
        let m = MultimodalMessage::new("q").with_image_base64("image/png", "AAAA");
        let p = GeminiVisionBackend::build_messages_payload(&m);
        let parts = p["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1]["inline_data"]["mime_type"], "image/png");
        assert_eq!(parts[1]["inline_data"]["data"], "AAAA");
    }

    #[test]
    fn gemini_payload_with_url_uses_file_data() {
        let m = MultimodalMessage::new("q").with_image_url("https://x/y.png");
        let p = GeminiVisionBackend::build_messages_payload(&m);
        let parts = p["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts[1]["file_data"]["file_uri"], "https://x/y.png");
        assert_eq!(parts[1]["file_data"]["mime_type"], "image/*");
    }

    #[test]
    fn gemini_payload_multiple_images() {
        let m = MultimodalMessage::new("compare")
            .with_image_base64("image/png", "AA")
            .with_image_base64("image/jpeg", "BB");
        let p = GeminiVisionBackend::build_messages_payload(&m);
        let parts = p["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0]["text"], "compare");
        assert_eq!(parts[1]["inline_data"]["mime_type"], "image/png");
        assert_eq!(parts[2]["inline_data"]["mime_type"], "image/jpeg");
    }

    #[test]
    fn gemini_payload_mixed_url_and_base64() {
        let m = MultimodalMessage::new("q")
            .with_image_url("https://a")
            .with_image_base64("image/webp", "WW");
        let p = GeminiVisionBackend::build_messages_payload(&m);
        let parts = p["contents"][0]["parts"].as_array().unwrap();
        assert!(parts[1]["file_data"].is_object());
        assert!(parts[2]["inline_data"].is_object());
    }

    #[test]
    fn gemini_payload_root_shape() {
        let m = MultimodalMessage::new("x");
        let p = GeminiVisionBackend::build_messages_payload(&m);
        assert!(p["contents"].is_array());
        assert_eq!(p["contents"].as_array().unwrap().len(), 1);
    }

    // ---- ask_with_image stub behaviour ----

    #[tokio::test]
    async fn claude_ask_with_image_stub() {
        let b = ClaudeVisionBackend::new("k", "claude-sonnet-4");
        let m = MultimodalMessage::new("what?").with_image_url("https://x");
        let out = b.ask_with_image(&m).await.unwrap();
        assert!(out.contains("claude-vision-stub"));
        assert!(out.contains("1 image"));
        assert!(out.contains("what?"));
    }

    #[tokio::test]
    async fn openai_ask_with_image_stub() {
        let b = OpenAiVisionBackend::new("k", "gpt-4o");
        let m = MultimodalMessage::new("describe")
            .with_image_base64("image/png", "AA")
            .with_image_url("https://x");
        let out = b.ask_with_image(&m).await.unwrap();
        assert!(out.contains("openai-vision-stub"));
        assert!(out.contains("2 image"));
    }

    #[tokio::test]
    async fn gemini_ask_with_image_stub() {
        let b = GeminiVisionBackend::new("k", "gemini-2.0-flash");
        let m = MultimodalMessage::new("analyze");
        let out = b.ask_with_image(&m).await.unwrap();
        assert!(out.contains("gemini-vision-stub"));
        assert!(out.contains("0 image"));
        assert!(out.contains("analyze"));
    }

    // ---- Trait-object usage (ensures Send + Sync) ----

    #[test]
    fn vision_backend_is_object_safe() {
        let _backends: Vec<Box<dyn VisionBackend>> = vec![
            Box::new(ClaudeVisionBackend::new("k", "m")),
            Box::new(OpenAiVisionBackend::new("k", "m")),
            Box::new(GeminiVisionBackend::new("k", "m")),
        ];
    }

    #[tokio::test]
    async fn vision_backend_dispatch_dynamic() {
        let backends: Vec<Box<dyn VisionBackend>> = vec![
            Box::new(ClaudeVisionBackend::new("k", "m")),
            Box::new(OpenAiVisionBackend::new("k", "m")),
            Box::new(GeminiVisionBackend::new("k", "m")),
        ];
        let m = MultimodalMessage::new("hi").with_image_url("https://x");
        for b in &backends {
            let out = b.ask_with_image(&m).await.unwrap();
            assert!(out.contains(b.provider_name()));
        }
    }
}
