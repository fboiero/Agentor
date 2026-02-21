use agentor_core::{AgentorResult, ToolCall, ToolResult};
use agentor_security::Capability;
use agentor_skills::skill::{Skill, SkillDescriptor};
use async_trait::async_trait;
use std::time::Duration;
use tracing::info;

const MAX_RESPONSE_SIZE: usize = 5 * 1024 * 1024; // 5MB

/// HTTP fetch skill. Makes GET/POST requests to allowed hosts.
pub struct HttpFetchSkill {
    descriptor: SkillDescriptor,
    client: reqwest::Client,
}

impl HttpFetchSkill {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            descriptor: SkillDescriptor {
                name: "http_fetch".to_string(),
                description: "Fetch content from a URL via HTTP GET or POST.".to_string(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to fetch"
                        },
                        "method": {
                            "type": "string",
                            "enum": ["GET", "POST"],
                            "description": "HTTP method (default: GET)"
                        },
                        "headers": {
                            "type": "object",
                            "description": "Optional HTTP headers as key-value pairs"
                        },
                        "body": {
                            "type": "string",
                            "description": "Optional request body (for POST)"
                        }
                    },
                    "required": ["url"]
                }),
                required_capabilities: vec![Capability::NetworkAccess {
                    allowed_hosts: vec![], // Configured at runtime
                }],
            },
            client,
        }
    }
}

impl Default for HttpFetchSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for HttpFetchSkill {
    fn descriptor(&self) -> &SkillDescriptor {
        &self.descriptor
    }

    async fn execute(&self, call: ToolCall) -> AgentorResult<ToolResult> {
        let url = call.arguments["url"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        if url.is_empty() {
            return Ok(ToolResult::error(&call.id, "Empty URL"));
        }

        // Validate URL
        let parsed_url = match reqwest::Url::parse(&url) {
            Ok(u) => u,
            Err(e) => {
                return Ok(ToolResult::error(
                    &call.id,
                    format!("Invalid URL '{}': {}", url, e),
                ));
            }
        };

        // Block internal/private networks (SSRF prevention)
        if let Some(host) = parsed_url.host_str() {
            if is_private_host(host) {
                return Ok(ToolResult::error(
                    &call.id,
                    format!(
                        "Access denied: '{}' resolves to a private/internal address",
                        host
                    ),
                ));
            }
        }

        // Only allow http/https
        match parsed_url.scheme() {
            "http" | "https" => {}
            scheme => {
                return Ok(ToolResult::error(
                    &call.id,
                    format!("Unsupported scheme '{}'. Only http/https allowed.", scheme),
                ));
            }
        }

        let method = call.arguments["method"]
            .as_str()
            .unwrap_or("GET")
            .to_uppercase();

        info!(url = %url, method = %method, "HTTP fetch");

        let mut request = match method.as_str() {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            _ => {
                return Ok(ToolResult::error(
                    &call.id,
                    format!("Unsupported method '{}'. Use GET or POST.", method),
                ));
            }
        };

        // Add custom headers
        if let Some(headers) = call.arguments["headers"].as_object() {
            for (key, value) in headers {
                if let Some(v) = value.as_str() {
                    request = request.header(key.as_str(), v);
                }
            }
        }

        // Add body for POST
        if method == "POST" {
            if let Some(body) = call.arguments["body"].as_str() {
                request = request.body(body.to_string());
            }
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::error(
                    &call.id,
                    format!("HTTP request failed: {}", e),
                ));
            }
        };

        let status = response.status().as_u16();
        let headers: serde_json::Map<String, serde_json::Value> = response
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|val| (k.to_string(), serde_json::Value::String(val.to_string())))
            })
            .collect();

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body_bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return Ok(ToolResult::error(
                    &call.id,
                    format!("Failed to read response body: {}", e),
                ));
            }
        };

        if body_bytes.len() > MAX_RESPONSE_SIZE {
            return Ok(ToolResult::error(
                &call.id,
                format!(
                    "Response too large: {} bytes (max: {} bytes)",
                    body_bytes.len(),
                    MAX_RESPONSE_SIZE
                ),
            ));
        }

        let body = String::from_utf8_lossy(&body_bytes);

        let result = serde_json::json!({
            "status": status,
            "headers": headers,
            "content_type": content_type,
            "body": body,
            "size": body_bytes.len(),
        });

        if (200..400).contains(&status) {
            Ok(ToolResult::success(&call.id, result.to_string()))
        } else {
            Ok(ToolResult::error(&call.id, result.to_string()))
        }
    }
}

/// Check if a host resolves to a private/internal network address (SSRF prevention).
fn is_private_host(host: &str) -> bool {
    let private_patterns = [
        "localhost",
        "127.",
        "10.",
        "172.16.",
        "172.17.",
        "172.18.",
        "172.19.",
        "172.20.",
        "172.21.",
        "172.22.",
        "172.23.",
        "172.24.",
        "172.25.",
        "172.26.",
        "172.27.",
        "172.28.",
        "172.29.",
        "172.30.",
        "172.31.",
        "192.168.",
        "169.254.",
        "0.0.0.0",
        "[::1]",
        "metadata.google",
        "metadata.aws",
    ];

    let host_lower = host.to_lowercase();
    private_patterns
        .iter()
        .any(|p| host_lower.starts_with(p) || host_lower == *p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_private_host_detection() {
        assert!(is_private_host("localhost"));
        assert!(is_private_host("127.0.0.1"));
        assert!(is_private_host("192.168.1.1"));
        assert!(is_private_host("10.0.0.1"));
        assert!(is_private_host("169.254.169.254"));
        assert!(is_private_host("metadata.google.internal"));
        assert!(!is_private_host("google.com"));
        assert!(!is_private_host("api.anthropic.com"));
    }

    #[tokio::test]
    async fn test_http_fetch_invalid_url() {
        let skill = HttpFetchSkill::new();
        let call = ToolCall {
            id: "test_1".to_string(),
            name: "http_fetch".to_string(),
            arguments: serde_json::json!({"url": "not a url"}),
        };
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_http_fetch_blocks_ssrf() {
        let skill = HttpFetchSkill::new();
        let call = ToolCall {
            id: "test_2".to_string(),
            name: "http_fetch".to_string(),
            arguments: serde_json::json!({"url": "http://169.254.169.254/latest/meta-data/"}),
        };
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("private"));
    }

    #[tokio::test]
    async fn test_http_fetch_blocks_localhost() {
        let skill = HttpFetchSkill::new();
        let call = ToolCall {
            id: "test_3".to_string(),
            name: "http_fetch".to_string(),
            arguments: serde_json::json!({"url": "http://localhost:8080/admin"}),
        };
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_http_fetch_blocks_bad_scheme() {
        let skill = HttpFetchSkill::new();
        let call = ToolCall {
            id: "test_4".to_string(),
            name: "http_fetch".to_string(),
            arguments: serde_json::json!({"url": "file:///etc/passwd"}),
        };
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
    }
}
