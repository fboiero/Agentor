//! Agent card builder and discovery helpers.
//!
//! Provides a fluent API for constructing [`AgentCard`] instances without
//! having to manually populate every field.
//!
//! # Example
//!
//! ```rust
//! use argentor_a2a::discovery::AgentCardBuilder;
//!
//! let card = AgentCardBuilder::new("MyAgent", "http://localhost:3000")
//!     .description("An intelligent assistant")
//!     .version("1.0.0")
//!     .streaming(true)
//!     .add_skill("summarize", "Summarize", "Summarize documents")
//!     .add_input_mode("text/plain")
//!     .add_output_mode("text/plain")
//!     .build();
//!
//! assert_eq!(card.name, "MyAgent");
//! assert!(card.capabilities.streaming);
//! assert_eq!(card.skills.len(), 1);
//! ```

use crate::protocol::*;

/// Fluent builder for constructing [`AgentCard`] instances.
pub struct AgentCardBuilder {
    name: String,
    description: String,
    url: String,
    version: String,
    provider: Option<AgentProvider>,
    capabilities: AgentCapabilities,
    skills: Vec<AgentSkill>,
    default_input_modes: Vec<String>,
    default_output_modes: Vec<String>,
    authentication: Option<AuthenticationInfo>,
}

impl AgentCardBuilder {
    /// Create a new builder with the required name and URL.
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            url: url.into(),
            version: "0.1.0".to_string(),
            provider: None,
            capabilities: AgentCapabilities::default(),
            skills: Vec::new(),
            default_input_modes: Vec::new(),
            default_output_modes: Vec::new(),
            authentication: None,
        }
    }

    /// Set the agent description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the agent version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set the provider organization information.
    pub fn provider(mut self, organization: impl Into<String>, url: Option<String>) -> Self {
        self.provider = Some(AgentProvider {
            organization: organization.into(),
            url,
        });
        self
    }

    /// Enable or disable streaming support.
    pub fn streaming(mut self, enabled: bool) -> Self {
        self.capabilities.streaming = enabled;
        self
    }

    /// Enable or disable push notifications.
    pub fn push_notifications(mut self, enabled: bool) -> Self {
        self.capabilities.push_notifications = enabled;
        self
    }

    /// Enable or disable state transition history tracking.
    pub fn state_transition_history(mut self, enabled: bool) -> Self {
        self.capabilities.state_transition_history = enabled;
        self
    }

    /// Set the full capabilities object.
    pub fn capabilities(mut self, capabilities: AgentCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Add a skill to the agent card.
    pub fn add_skill(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.skills.push(AgentSkill {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            tags: Vec::new(),
            examples: Vec::new(),
        });
        self
    }

    /// Add a fully configured skill to the agent card.
    pub fn add_skill_full(mut self, skill: AgentSkill) -> Self {
        self.skills.push(skill);
        self
    }

    /// Add an accepted input content type.
    pub fn add_input_mode(mut self, mode: impl Into<String>) -> Self {
        self.default_input_modes.push(mode.into());
        self
    }

    /// Add a produced output content type.
    pub fn add_output_mode(mut self, mode: impl Into<String>) -> Self {
        self.default_output_modes.push(mode.into());
        self
    }

    /// Set the authentication requirements.
    pub fn authentication(mut self, auth: AuthenticationInfo) -> Self {
        self.authentication = Some(auth);
        self
    }

    /// Add a single authentication scheme.
    pub fn add_auth_scheme(
        mut self,
        scheme: impl Into<String>,
        service_url: Option<String>,
    ) -> Self {
        let auth = self
            .authentication
            .get_or_insert_with(AuthenticationInfo::default);
        auth.schemes.push(AuthScheme {
            scheme: scheme.into(),
            service_url,
        });
        self
    }

    /// Build the [`AgentCard`].
    pub fn build(self) -> AgentCard {
        AgentCard {
            name: self.name,
            description: self.description,
            url: self.url,
            version: self.version,
            provider: self.provider,
            capabilities: self.capabilities,
            skills: self.skills,
            default_input_modes: self.default_input_modes,
            default_output_modes: self.default_output_modes,
            authentication: self.authentication,
        }
    }
}

/// Create a minimal agent card from basic information.
///
/// This is a convenience function for creating simple agent cards without
/// the full builder API.
pub fn minimal_agent_card(name: &str, url: &str, description: &str) -> AgentCard {
    AgentCardBuilder::new(name, url)
        .description(description)
        .add_input_mode("text/plain")
        .add_output_mode("text/plain")
        .build()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_minimal() {
        let card = AgentCardBuilder::new("TestAgent", "http://localhost:3000").build();
        assert_eq!(card.name, "TestAgent");
        assert_eq!(card.url, "http://localhost:3000");
        assert_eq!(card.version, "0.1.0");
        assert!(card.description.is_empty());
        assert!(card.skills.is_empty());
        assert!(!card.capabilities.streaming);
    }

    #[test]
    fn test_builder_full() {
        let card = AgentCardBuilder::new("FullAgent", "https://agent.example.com")
            .description("A fully configured agent")
            .version("2.0.0")
            .provider("Argentor", Some("https://argentor.dev".to_string()))
            .streaming(true)
            .push_notifications(true)
            .state_transition_history(true)
            .add_skill("summarize", "Summarize", "Summarize a document")
            .add_skill("translate", "Translate", "Translate text between languages")
            .add_input_mode("text/plain")
            .add_input_mode("application/json")
            .add_output_mode("text/plain")
            .add_auth_scheme("Bearer", Some("https://auth.example.com/token".to_string()))
            .build();

        assert_eq!(card.name, "FullAgent");
        assert_eq!(card.description, "A fully configured agent");
        assert_eq!(card.version, "2.0.0");
        assert!(card.capabilities.streaming);
        assert!(card.capabilities.push_notifications);
        assert!(card.capabilities.state_transition_history);
        assert_eq!(card.skills.len(), 2);
        assert_eq!(card.skills[0].id, "summarize");
        assert_eq!(card.skills[1].id, "translate");
        assert_eq!(card.default_input_modes.len(), 2);
        assert_eq!(card.default_output_modes.len(), 1);
        assert!(card.authentication.is_some());
        assert_eq!(card.authentication.unwrap().schemes.len(), 1);

        let provider = card.provider.unwrap();
        assert_eq!(provider.organization, "Argentor");
        assert_eq!(provider.url, Some("https://argentor.dev".to_string()));
    }

    #[test]
    fn test_builder_add_skill_full() {
        let skill = AgentSkill {
            id: "custom".to_string(),
            name: "Custom Skill".to_string(),
            description: "A custom skill".to_string(),
            tags: vec!["custom".to_string(), "test".to_string()],
            examples: vec!["Do something custom".to_string()],
        };

        let card = AgentCardBuilder::new("Agent", "http://localhost")
            .add_skill_full(skill)
            .build();

        assert_eq!(card.skills.len(), 1);
        assert_eq!(card.skills[0].tags.len(), 2);
        assert_eq!(card.skills[0].examples.len(), 1);
    }

    #[test]
    fn test_builder_capabilities_object() {
        let caps = AgentCapabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: true,
        };

        let card = AgentCardBuilder::new("Agent", "http://localhost")
            .capabilities(caps)
            .build();

        assert!(card.capabilities.streaming);
        assert!(!card.capabilities.push_notifications);
        assert!(card.capabilities.state_transition_history);
    }

    #[test]
    fn test_builder_multiple_auth_schemes() {
        let card = AgentCardBuilder::new("SecureAgent", "http://localhost")
            .add_auth_scheme("Bearer", Some("https://auth.example.com".to_string()))
            .add_auth_scheme("ApiKey", None)
            .build();

        let auth = card.authentication.unwrap();
        assert_eq!(auth.schemes.len(), 2);
        assert_eq!(auth.schemes[0].scheme, "Bearer");
        assert_eq!(auth.schemes[1].scheme, "ApiKey");
        assert!(auth.schemes[1].service_url.is_none());
    }

    #[test]
    fn test_minimal_agent_card() {
        let card = minimal_agent_card("SimpleAgent", "http://localhost:8080", "A simple agent");
        assert_eq!(card.name, "SimpleAgent");
        assert_eq!(card.url, "http://localhost:8080");
        assert_eq!(card.description, "A simple agent");
        assert_eq!(card.default_input_modes, vec!["text/plain"]);
        assert_eq!(card.default_output_modes, vec!["text/plain"]);
    }

    #[test]
    fn test_builder_serialization_roundtrip() {
        let card = AgentCardBuilder::new("RoundtripAgent", "http://localhost:3000")
            .description("Testing serialization")
            .version("1.0.0")
            .streaming(true)
            .add_skill("echo", "Echo", "Echo input")
            .add_input_mode("text/plain")
            .add_output_mode("text/plain")
            .build();

        let json = serde_json::to_string(&card).unwrap();
        let parsed: AgentCard = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "RoundtripAgent");
        assert_eq!(parsed.version, "1.0.0");
        assert!(parsed.capabilities.streaming);
        assert_eq!(parsed.skills.len(), 1);
    }

    #[test]
    fn test_builder_chaining_order_independence() {
        let card1 = AgentCardBuilder::new("A", "http://a")
            .description("desc")
            .version("1.0")
            .streaming(true)
            .build();

        let card2 = AgentCardBuilder::new("A", "http://a")
            .streaming(true)
            .version("1.0")
            .description("desc")
            .build();

        assert_eq!(card1.name, card2.name);
        assert_eq!(card1.description, card2.description);
        assert_eq!(card1.version, card2.version);
        assert_eq!(card1.capabilities.streaming, card2.capabilities.streaming);
    }
}
