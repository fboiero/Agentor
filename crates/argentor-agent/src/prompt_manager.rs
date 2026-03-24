//! Versioned prompt template management system.
//!
//! Manages prompt templates with variable substitution, versioning, A/B testing
//! variants, conditional blocks, iteration, and multi-step prompt chains.
//!
//! # Main types
//!
//! - [`PromptManager`] — Thread-safe manager for versioned prompt templates.
//! - [`PromptTemplate`] — A versioned template with variable placeholders.
//! - [`TemplateVariable`] — Describes an expected variable (type, default, required).
//! - [`PromptChain`] — Compose multiple templates into a sequential pipeline.
//! - [`VarType`] — Supported variable types for validation.
//! - [`TemplateSummary`] — Lightweight summary returned by listing operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// VarType
// ---------------------------------------------------------------------------

/// Supported variable types for template variables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarType {
    /// A string value.
    String,
    /// An integer number.
    Integer,
    /// A floating-point number.
    Float,
    /// A boolean value.
    Boolean,
    /// A JSON value.
    Json,
    /// A list of values (rendered comma-separated or via `{{#each}}`).
    List,
}

// ---------------------------------------------------------------------------
// TemplateVariable
// ---------------------------------------------------------------------------

/// Describes a single variable expected by a prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVariable {
    /// Variable name (without curly braces).
    pub name: String,
    /// Expected type.
    pub var_type: VarType,
    /// Whether this variable must be supplied at render time.
    pub required: bool,
    /// Default value used when the variable is not supplied.
    pub default: Option<String>,
    /// Human-readable description of the variable's purpose.
    pub description: Option<String>,
}

impl TemplateVariable {
    /// Create a required string variable.
    pub fn required(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            var_type: VarType::String,
            required: true,
            default: None,
            description: None,
        }
    }

    /// Create an optional string variable with a default value.
    pub fn optional(name: impl Into<String>, default: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            var_type: VarType::String,
            required: false,
            default: Some(default.into()),
            description: None,
        }
    }

    /// Set the variable type.
    #[must_use]
    pub fn with_type(mut self, var_type: VarType) -> Self {
        self.var_type = var_type;
        self
    }

    /// Set the description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

// ---------------------------------------------------------------------------
// PromptTemplate
// ---------------------------------------------------------------------------

/// A versioned prompt template with variable placeholders and A/B variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Unique template identifier (e.g. `"sales_qualifier_v1"`).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of the template's purpose.
    pub description: String,
    /// Template content with `{{variable}}` placeholders.
    pub content: String,
    /// Expected variables.
    pub variables: Vec<TemplateVariable>,
    /// Monotonically increasing version number.
    pub version: u32,
    /// When this version was created.
    pub created_at: DateTime<Utc>,
    /// When this version was last updated.
    pub updated_at: DateTime<Utc>,
    /// Categorisation tags.
    pub tags: Vec<String>,
    /// Named content variants for A/B testing (e.g. `"formal"` vs `"casual"`).
    pub variants: HashMap<String, String>,
    /// Arbitrary key-value metadata.
    pub metadata: HashMap<String, String>,
}

impl PromptTemplate {
    /// Create a new template at version 1.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            content: content.into(),
            variables: Vec::new(),
            version: 1,
            created_at: now,
            updated_at: now,
            tags: Vec::new(),
            variants: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Set the description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Add a variable.
    #[must_use]
    pub fn with_variable(mut self, var: TemplateVariable) -> Self {
        self.variables.push(var);
        self
    }

    /// Add multiple variables.
    #[must_use]
    pub fn with_variables(mut self, vars: Vec<TemplateVariable>) -> Self {
        self.variables.extend(vars);
        self
    }

    /// Add a tag.
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Add a named variant.
    #[must_use]
    pub fn with_variant(mut self, name: impl Into<String>, content: impl Into<String>) -> Self {
        self.variants.insert(name.into(), content.into());
        self
    }

    /// Add a metadata entry.
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

// ---------------------------------------------------------------------------
// TemplateSummary
// ---------------------------------------------------------------------------

/// Lightweight summary of a template (returned by listing operations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSummary {
    /// Template id.
    pub id: String,
    /// Template name.
    pub name: String,
    /// Current version.
    pub version: u32,
    /// Tags.
    pub tags: Vec<String>,
    /// Number of variants available.
    pub variant_count: usize,
}

impl From<&PromptTemplate> for TemplateSummary {
    fn from(t: &PromptTemplate) -> Self {
        Self {
            id: t.id.clone(),
            name: t.name.clone(),
            version: t.version,
            tags: t.tags.clone(),
            variant_count: t.variants.len(),
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering engine (free functions)
// ---------------------------------------------------------------------------

/// Render a template string by substituting variables.
///
/// Supports:
/// - `{{variable}}` — simple substitution
/// - `{{variable|default:value}}` — default if not provided
/// - `{{variable|upper}}` — uppercase
/// - `{{variable|lower}}` — lowercase
/// - `{{#if variable}}...{{/if}}` — conditional blocks
/// - `{{#each items}}...{{/each}}` — iteration (comma-separated list)
fn render_template(content: &str, vars: &HashMap<String, String>) -> String {
    let mut result = content.to_string();

    // --- Pass 1: {{#each items}}...{{/each}} ---
    result = render_each_blocks(&result, vars);

    // --- Pass 2: {{#if variable}}...{{/if}} ---
    result = render_if_blocks(&result, vars);

    // --- Pass 3: {{variable|filter}} and {{variable}} ---
    result = render_variables(&result, vars);

    result
}

/// Render `{{#each items}}...{{/each}}` blocks.
fn render_each_blocks(content: &str, vars: &HashMap<String, String>) -> String {
    let mut result = content.to_string();
    // Non-greedy match for each blocks.
    let each_re = regex::Regex::new(r"\{\{#each\s+(\w+)\}\}([\s\S]*?)\{\{/each\}\}").unwrap();

    while let Some(caps) = each_re.captures(&result) {
        let full_match = caps.get(0).unwrap();
        let var_name = &caps[1];
        let body = &caps[2];

        let replacement = if let Some(val) = vars.get(var_name) {
            // Split by commas and render body for each item.
            val.split(',')
                .map(|item| body.replace("{{item}}", item.trim()))
                .collect::<Vec<_>>()
                .join("")
        } else {
            String::new()
        };

        result = format!(
            "{}{}{}",
            &result[..full_match.start()],
            replacement,
            &result[full_match.end()..]
        );
    }

    result
}

/// Render `{{#if variable}}...{{/if}}` blocks.
fn render_if_blocks(content: &str, vars: &HashMap<String, String>) -> String {
    let mut result = content.to_string();
    let if_re = regex::Regex::new(r"\{\{#if\s+(\w+)\}\}([\s\S]*?)\{\{/if\}\}").unwrap();

    while let Some(caps) = if_re.captures(&result) {
        let full_match = caps.get(0).unwrap();
        let var_name = &caps[1];
        let body = &caps[2];

        let is_truthy = vars
            .get(var_name)
            .map(|v| !v.is_empty() && v != "false" && v != "0")
            .unwrap_or(false);

        let replacement = if is_truthy {
            body.to_string()
        } else {
            String::new()
        };

        result = format!(
            "{}{}{}",
            &result[..full_match.start()],
            replacement,
            &result[full_match.end()..]
        );
    }

    result
}

/// Render `{{variable}}`, `{{variable|default:val}}`, `{{variable|upper}}`, `{{variable|lower}}`.
fn render_variables(content: &str, vars: &HashMap<String, String>) -> String {
    let var_re =
        regex::Regex::new(r"\{\{(\w+)(?:\|(\w+)(?::([^}]*))?)?\}\}").unwrap();

    var_re
        .replace_all(content, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let filter = caps.get(2).map(|m| m.as_str());
            let filter_arg = caps.get(3).map(|m| m.as_str());

            let raw_value = vars.get(var_name).cloned();

            match filter {
                Some("default") => {
                    let default_val = filter_arg.unwrap_or("");
                    raw_value.unwrap_or_else(|| default_val.to_string())
                }
                Some("upper") => raw_value
                    .unwrap_or_default()
                    .to_uppercase(),
                Some("lower") => raw_value
                    .unwrap_or_default()
                    .to_lowercase(),
                _ => raw_value.unwrap_or_default(),
            }
        })
        .to_string()
}

// ---------------------------------------------------------------------------
// PromptManager (thread-safe)
// ---------------------------------------------------------------------------

/// Internal state guarded by `RwLock`.
#[derive(Debug, Default)]
struct ManagerInner {
    /// Current (latest) version of each template, keyed by template id.
    templates: HashMap<String, PromptTemplate>,
    /// Full version history, keyed by template id.
    history: HashMap<String, Vec<PromptTemplate>>,
    /// Active variant per template id (if A/B testing is enabled).
    active_variants: HashMap<String, String>,
}

/// Thread-safe manager for versioned prompt templates.
///
/// All public methods acquire the internal `RwLock` and are safe to call from
/// multiple threads or async tasks.
#[derive(Debug, Clone)]
pub struct PromptManager {
    inner: Arc<RwLock<ManagerInner>>,
}

impl Default for PromptManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptManager {
    /// Create a new, empty prompt manager.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ManagerInner::default())),
        }
    }

    /// Register (or update) a template. If a template with the same id already
    /// exists, the old version is archived and the new version number is
    /// incremented automatically.
    pub fn register_template(&self, mut template: PromptTemplate) {
        let mut inner = self.inner.write().unwrap();

        if let Some(existing) = inner.templates.get(&template.id).cloned() {
            // Archive the current version.
            template.version = existing.version + 1;
            template.updated_at = Utc::now();
            inner
                .history
                .entry(template.id.clone())
                .or_default()
                .push(existing);
        }

        inner.templates.insert(template.id.clone(), template);
    }

    /// Render the latest version of a template, substituting the given variables.
    /// If an active variant is set for this template, the variant content is used.
    pub fn render(
        &self,
        template_id: &str,
        variables: &HashMap<String, String>,
    ) -> Result<String, PromptError> {
        let inner = self.inner.read().unwrap();
        let tmpl = inner
            .templates
            .get(template_id)
            .ok_or_else(|| PromptError::TemplateNotFound(template_id.to_string()))?;

        // Check required variables.
        for var in &tmpl.variables {
            if var.required && !variables.contains_key(&var.name) && var.default.is_none() {
                return Err(PromptError::MissingVariable(var.name.clone()));
            }
        }

        // Build effective variables: defaults first, then caller overrides.
        let mut effective = HashMap::new();
        for var in &tmpl.variables {
            if let Some(default) = &var.default {
                effective.insert(var.name.clone(), default.clone());
            }
        }
        for (k, v) in variables {
            effective.insert(k.clone(), v.clone());
        }

        // Pick content: active variant or base content.
        let content = if let Some(variant_name) = inner.active_variants.get(template_id) {
            tmpl.variants
                .get(variant_name)
                .unwrap_or(&tmpl.content)
        } else {
            &tmpl.content
        };

        Ok(render_template(content, &effective))
    }

    /// Render a specific historical version of a template.
    pub fn render_version(
        &self,
        template_id: &str,
        version: u32,
        variables: &HashMap<String, String>,
    ) -> Result<String, PromptError> {
        let inner = self.inner.read().unwrap();

        // Check if the current version matches.
        if let Some(current) = inner.templates.get(template_id) {
            if current.version == version {
                drop(inner);
                return self.render(template_id, variables);
            }
        }

        // Search history.
        let history = inner
            .history
            .get(template_id)
            .ok_or_else(|| PromptError::VersionNotFound(template_id.to_string(), version))?;

        let tmpl = history
            .iter()
            .find(|t| t.version == version)
            .ok_or_else(|| PromptError::VersionNotFound(template_id.to_string(), version))?;

        let mut effective = HashMap::new();
        for var in &tmpl.variables {
            if let Some(default) = &var.default {
                effective.insert(var.name.clone(), default.clone());
            }
        }
        for (k, v) in variables {
            effective.insert(k.clone(), v.clone());
        }

        Ok(render_template(&tmpl.content, &effective))
    }

    /// List all registered templates.
    pub fn list_templates(&self) -> Vec<TemplateSummary> {
        let inner = self.inner.read().unwrap();
        inner.templates.values().map(TemplateSummary::from).collect()
    }

    /// Get the current version of a template by id.
    pub fn get_template(&self, id: &str) -> Option<PromptTemplate> {
        let inner = self.inner.read().unwrap();
        inner.templates.get(id).cloned()
    }

    /// Get the full version history of a template.
    pub fn get_history(&self, id: &str) -> Vec<PromptTemplate> {
        let inner = self.inner.read().unwrap();
        inner.history.get(id).cloned().unwrap_or_default()
    }

    /// Set the active variant for a template (used in A/B testing).
    /// Pass `None` to clear the active variant and revert to base content.
    pub fn set_active_variant(
        &self,
        template_id: &str,
        variant: Option<&str>,
    ) -> Result<(), PromptError> {
        let mut inner = self.inner.write().unwrap();
        let tmpl = inner
            .templates
            .get(template_id)
            .ok_or_else(|| PromptError::TemplateNotFound(template_id.to_string()))?;

        if let Some(v) = variant {
            if !tmpl.variants.contains_key(v) {
                return Err(PromptError::VariantNotFound(
                    template_id.to_string(),
                    v.to_string(),
                ));
            }
            inner
                .active_variants
                .insert(template_id.to_string(), v.to_string());
        } else {
            inner.active_variants.remove(template_id);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PromptChain
// ---------------------------------------------------------------------------

/// A step in a prompt chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStep {
    /// Template id to render.
    pub template_id: String,
    /// Maps output names from previous steps to variable names in this template.
    /// Key = variable name expected by the template, Value = key in the running
    /// context (or a literal prefixed with `"literal:"`).
    pub variable_mapping: HashMap<String, String>,
}

/// Compose multiple templates into a sequential pipeline.
///
/// The output of each step is stored under the key `"step_N_output"` (0-indexed)
/// and can be referenced by subsequent steps through variable mappings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptChain {
    /// Human-readable name for the chain.
    pub name: String,
    /// Ordered list of steps.
    pub steps: Vec<ChainStep>,
}

impl PromptChain {
    /// Create a new, empty chain.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    /// Append a step to the chain.
    pub fn add_step(
        &mut self,
        template_id: impl Into<String>,
        variable_mapping: HashMap<String, String>,
    ) {
        self.steps.push(ChainStep {
            template_id: template_id.into(),
            variable_mapping,
        });
    }

    /// Render the full chain. Each step's rendered output is stored in the
    /// running context as `"step_N_output"` and can be consumed by later steps.
    pub fn render(
        &self,
        manager: &PromptManager,
        initial_vars: &HashMap<String, String>,
    ) -> Result<Vec<String>, PromptError> {
        let mut context: HashMap<String, String> = initial_vars.clone();
        let mut outputs = Vec::new();

        for (idx, step) in self.steps.iter().enumerate() {
            // Build variables for this step by applying the mapping.
            let mut step_vars = HashMap::new();
            for (template_var, source) in &step.variable_mapping {
                if let Some(literal) = source.strip_prefix("literal:") {
                    step_vars.insert(template_var.clone(), literal.to_string());
                } else if let Some(val) = context.get(source) {
                    step_vars.insert(template_var.clone(), val.clone());
                }
            }

            let rendered = manager.render(&step.template_id, &step_vars)?;
            context.insert(format!("step_{idx}_output"), rendered.clone());
            outputs.push(rendered);
        }

        Ok(outputs)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during template operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum PromptError {
    /// Template with the given id was not found.
    #[error("template not found: {0}")]
    TemplateNotFound(String),

    /// Requested version does not exist.
    #[error("version {1} not found for template {0}")]
    VersionNotFound(String, u32),

    /// A required variable was not supplied and has no default.
    #[error("missing required variable: {0}")]
    MissingVariable(String),

    /// The requested variant does not exist on the template.
    #[error("variant '{1}' not found on template {0}")]
    VariantNotFound(String, String),
}

// ---------------------------------------------------------------------------
// Pre-built templates for XcapitSFF
// ---------------------------------------------------------------------------

/// Create the `sales_qualifier_v1` template.
pub fn sales_qualifier_v1() -> PromptTemplate {
    PromptTemplate::new(
        "sales_qualifier_v1",
        "Sales Qualifier",
        "You are a sales qualification assistant for the {{region|upper}} region.\n\n\
         Evaluate the following lead:\n\
         - Company: {{company}}\n\
         - Contact title: {{title}}\n\
         - Lead score: {{score}}\n\
         - Affinity index: {{affinity|default:unknown}}\n\n\
         {{#if score}}Based on the score of {{score}}, classify this lead as hot, warm, or cold.{{/if}}\n\n\
         Provide a concise qualification summary and recommended next steps.",
    )
    .with_description("Qualifies inbound leads based on company, region, title, score, and affinity.")
    .with_variables(vec![
        TemplateVariable::required("company").with_description("Company name"),
        TemplateVariable::required("region").with_description("Geographic region"),
        TemplateVariable::required("title").with_description("Contact's job title"),
        TemplateVariable::required("score").with_type(VarType::Integer).with_description("Lead score (0-100)"),
        TemplateVariable::optional("affinity", "unknown").with_type(VarType::Float).with_description("Affinity index"),
    ])
    .with_tag("sales")
    .with_tag("xcapit-sff")
    .with_metadata("domain", "sales")
}

/// Create the `outreach_composer_v1` template.
pub fn outreach_composer_v1() -> PromptTemplate {
    PromptTemplate::new(
        "outreach_composer_v1",
        "Outreach Composer",
        "Compose a {{channel|lower}} outreach message for:\n\n\
         Company: {{company}}\n\
         Contact: {{contact}}\n\
         Region: {{region}}\n\
         Score class: {{score_class}}\n\n\
         {{#if score_class}}Tailor the tone and urgency to a {{score_class}} lead.{{/if}}\n\n\
         Keep it under 200 words.",
    )
    .with_description("Generates personalised outreach messages across channels.")
    .with_variables(vec![
        TemplateVariable::required("company"),
        TemplateVariable::required("contact"),
        TemplateVariable::required("channel").with_description("Communication channel (email, linkedin, phone)"),
        TemplateVariable::required("region"),
        TemplateVariable::required("score_class").with_description("hot, warm, or cold"),
    ])
    .with_variant(
        "formal",
        "Dear {{contact}},\n\nI am writing on behalf of our {{region}} team regarding {{company}}.\n\
         As a {{score_class}} prospect, we believe there is significant opportunity for collaboration.\n\n\
         Channel: {{channel|upper}}\n\nBest regards.",
    )
    .with_variant(
        "casual",
        "Hey {{contact}}!\n\nQuick note about {{company}} — you're flagged as {{score_class}} in {{region}}.\n\
         Let's connect on {{channel|lower}}!\n\nCheers!",
    )
    .with_tag("outreach")
    .with_tag("xcapit-sff")
}

/// Create the `support_responder_v1` template.
pub fn support_responder_v1() -> PromptTemplate {
    PromptTemplate::new(
        "support_responder_v1",
        "Support Responder",
        "You are a customer support agent.\n\n\
         Ticket category: {{ticket_category}}\n\
         Priority: {{priority|upper}}\n\
         Customer message:\n{{customer_message}}\n\n\
         {{#if kb_context}}Relevant knowledge base context:\n{{kb_context}}\n\n{{/if}}\
         Draft a helpful, empathetic response that addresses the customer's concern.\n\
         If the priority is HIGH or CRITICAL, escalate appropriately.",
    )
    .with_description("Generates support responses using ticket context and KB articles.")
    .with_variables(vec![
        TemplateVariable::required("ticket_category").with_description("Category of the support ticket"),
        TemplateVariable::required("priority").with_description("Ticket priority (low, medium, high, critical)"),
        TemplateVariable::required("customer_message").with_description("The customer's original message"),
        TemplateVariable::optional("kb_context", "").with_description("Relevant knowledge base excerpts"),
    ])
    .with_tag("support")
    .with_tag("xcapit-sff")
}

/// Create the `ticket_router_v1` template.
pub fn ticket_router_v1() -> PromptTemplate {
    PromptTemplate::new(
        "ticket_router_v1",
        "Ticket Router",
        "Analyze the following customer message and classify it.\n\n\
         Message:\n{{message}}\n\n\
         Respond with a JSON object containing:\n\
         - \"category\": one of [billing, technical, account, general]\n\
         - \"priority\": one of [low, medium, high, critical]\n\
         - \"summary\": a one-sentence summary\n\
         - \"suggested_team\": the team that should handle this",
    )
    .with_description("Routes incoming messages to the correct support category and priority.")
    .with_variables(vec![
        TemplateVariable::required("message").with_description("Raw customer message to classify"),
    ])
    .with_tag("routing")
    .with_tag("xcapit-sff")
}

/// Register all pre-built XcapitSFF templates in the given manager.
pub fn register_xcapit_templates(manager: &PromptManager) {
    manager.register_template(sales_qualifier_v1());
    manager.register_template(outreach_composer_v1());
    manager.register_template(support_responder_v1());
    manager.register_template(ticket_router_v1());
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- Helpers --

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn basic_template() -> PromptTemplate {
        PromptTemplate::new("test1", "Test Template", "Hello {{name}}, welcome to {{place}}!")
            .with_variable(TemplateVariable::required("name"))
            .with_variable(TemplateVariable::required("place"))
    }

    // -----------------------------------------------------------------------
    // 1. Basic variable substitution
    // -----------------------------------------------------------------------
    #[test]
    fn test_simple_substitution() {
        let mgr = PromptManager::new();
        mgr.register_template(basic_template());
        let result = mgr
            .render("test1", &vars(&[("name", "Alice"), ("place", "Wonderland")]))
            .unwrap();
        assert_eq!(result, "Hello Alice, welcome to Wonderland!");
    }

    // -----------------------------------------------------------------------
    // 2. Missing required variable returns error
    // -----------------------------------------------------------------------
    #[test]
    fn test_missing_required_variable() {
        let mgr = PromptManager::new();
        mgr.register_template(basic_template());
        let err = mgr.render("test1", &vars(&[("name", "Alice")])).unwrap_err();
        assert_eq!(err, PromptError::MissingVariable("place".to_string()));
    }

    // -----------------------------------------------------------------------
    // 3. Default filter in template content
    // -----------------------------------------------------------------------
    #[test]
    fn test_default_filter() {
        let tmpl = PromptTemplate::new("d1", "Default", "Hello {{name|default:World}}!");
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);
        let result = mgr.render("d1", &HashMap::new()).unwrap();
        assert_eq!(result, "Hello World!");
    }

    // -----------------------------------------------------------------------
    // 4. Upper filter
    // -----------------------------------------------------------------------
    #[test]
    fn test_upper_filter() {
        let tmpl = PromptTemplate::new("u1", "Upper", "Region: {{region|upper}}");
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);
        let result = mgr.render("u1", &vars(&[("region", "latam")])).unwrap();
        assert_eq!(result, "Region: LATAM");
    }

    // -----------------------------------------------------------------------
    // 5. Lower filter
    // -----------------------------------------------------------------------
    #[test]
    fn test_lower_filter() {
        let tmpl = PromptTemplate::new("l1", "Lower", "Channel: {{channel|lower}}");
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);
        let result = mgr.render("l1", &vars(&[("channel", "EMAIL")])).unwrap();
        assert_eq!(result, "Channel: email");
    }

    // -----------------------------------------------------------------------
    // 6. Conditional block (truthy)
    // -----------------------------------------------------------------------
    #[test]
    fn test_if_block_truthy() {
        let tmpl = PromptTemplate::new("if1", "IfTest", "Start{{#if flag}} VISIBLE{{/if}} End");
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);
        let result = mgr.render("if1", &vars(&[("flag", "yes")])).unwrap();
        assert_eq!(result, "Start VISIBLE End");
    }

    // -----------------------------------------------------------------------
    // 7. Conditional block (falsy — missing)
    // -----------------------------------------------------------------------
    #[test]
    fn test_if_block_missing() {
        let tmpl = PromptTemplate::new("if2", "IfTest2", "Start{{#if flag}} HIDDEN{{/if}} End");
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);
        let result = mgr.render("if2", &HashMap::new()).unwrap();
        assert_eq!(result, "Start End");
    }

    // -----------------------------------------------------------------------
    // 8. Conditional block (falsy — "false")
    // -----------------------------------------------------------------------
    #[test]
    fn test_if_block_false_string() {
        let tmpl = PromptTemplate::new("if3", "IfTest3", "A{{#if x}} B{{/if}} C");
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);
        let result = mgr.render("if3", &vars(&[("x", "false")])).unwrap();
        assert_eq!(result, "A C");
    }

    // -----------------------------------------------------------------------
    // 9. Each block
    // -----------------------------------------------------------------------
    #[test]
    fn test_each_block() {
        let tmpl = PromptTemplate::new(
            "each1",
            "EachTest",
            "Items:{{#each items}} [{{item}}]{{/each}}",
        );
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);
        let result = mgr
            .render("each1", &vars(&[("items", "a,b,c")]))
            .unwrap();
        assert_eq!(result, "Items: [a] [b] [c]");
    }

    // -----------------------------------------------------------------------
    // 10. Each block with empty list
    // -----------------------------------------------------------------------
    #[test]
    fn test_each_block_missing() {
        let tmpl = PromptTemplate::new("each2", "EachTest2", "Items:{{#each items}} [{{item}}]{{/each}} done");
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);
        let result = mgr.render("each2", &HashMap::new()).unwrap();
        assert_eq!(result, "Items: done");
    }

    // -----------------------------------------------------------------------
    // 11. Template not found
    // -----------------------------------------------------------------------
    #[test]
    fn test_template_not_found() {
        let mgr = PromptManager::new();
        let err = mgr.render("nonexistent", &HashMap::new()).unwrap_err();
        assert_eq!(
            err,
            PromptError::TemplateNotFound("nonexistent".to_string())
        );
    }

    // -----------------------------------------------------------------------
    // 12. Versioning — register twice bumps version
    // -----------------------------------------------------------------------
    #[test]
    fn test_versioning() {
        let mgr = PromptManager::new();
        mgr.register_template(basic_template());
        let v1 = mgr.get_template("test1").unwrap();
        assert_eq!(v1.version, 1);

        let updated = PromptTemplate::new("test1", "Test v2", "Hi {{name}} at {{place}}!")
            .with_variable(TemplateVariable::required("name"))
            .with_variable(TemplateVariable::required("place"));
        mgr.register_template(updated);

        let v2 = mgr.get_template("test1").unwrap();
        assert_eq!(v2.version, 2);
        assert_eq!(v2.name, "Test v2");
    }

    // -----------------------------------------------------------------------
    // 13. History — old versions are preserved
    // -----------------------------------------------------------------------
    #[test]
    fn test_history() {
        let mgr = PromptManager::new();
        mgr.register_template(basic_template());
        let updated = PromptTemplate::new("test1", "Test v2", "v2 {{name}} {{place}}")
            .with_variable(TemplateVariable::required("name"))
            .with_variable(TemplateVariable::required("place"));
        mgr.register_template(updated);

        let history = mgr.get_history("test1");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].version, 1);
    }

    // -----------------------------------------------------------------------
    // 14. Render specific version
    // -----------------------------------------------------------------------
    #[test]
    fn test_render_version() {
        let mgr = PromptManager::new();
        mgr.register_template(
            PromptTemplate::new("rv", "RV", "v1: {{x}}")
                .with_variable(TemplateVariable::required("x")),
        );
        mgr.register_template(
            PromptTemplate::new("rv", "RV", "v2: {{x}}")
                .with_variable(TemplateVariable::required("x")),
        );

        let r1 = mgr.render_version("rv", 1, &vars(&[("x", "hello")])).unwrap();
        assert_eq!(r1, "v1: hello");

        let r2 = mgr.render_version("rv", 2, &vars(&[("x", "hello")])).unwrap();
        assert_eq!(r2, "v2: hello");
    }

    // -----------------------------------------------------------------------
    // 15. Render version not found
    // -----------------------------------------------------------------------
    #[test]
    fn test_render_version_not_found() {
        let mgr = PromptManager::new();
        mgr.register_template(basic_template());
        let err = mgr
            .render_version("test1", 99, &HashMap::new())
            .unwrap_err();
        assert!(matches!(err, PromptError::VersionNotFound(_, 99)));
    }

    // -----------------------------------------------------------------------
    // 16. List templates
    // -----------------------------------------------------------------------
    #[test]
    fn test_list_templates() {
        let mgr = PromptManager::new();
        mgr.register_template(basic_template());
        mgr.register_template(PromptTemplate::new("t2", "T2", "content"));
        let list = mgr.list_templates();
        assert_eq!(list.len(), 2);
    }

    // -----------------------------------------------------------------------
    // 17. Get template returns None for missing
    // -----------------------------------------------------------------------
    #[test]
    fn test_get_template_none() {
        let mgr = PromptManager::new();
        assert!(mgr.get_template("missing").is_none());
    }

    // -----------------------------------------------------------------------
    // 18. A/B variant — set and render
    // -----------------------------------------------------------------------
    #[test]
    fn test_ab_variant() {
        let tmpl = PromptTemplate::new("ab", "AB", "base: {{x}}")
            .with_variant("formal", "Dear {{x}}")
            .with_variant("casual", "Hey {{x}}!")
            .with_variable(TemplateVariable::required("x"));
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);

        // Base content.
        let r = mgr.render("ab", &vars(&[("x", "Bob")])).unwrap();
        assert_eq!(r, "base: Bob");

        // Switch to formal.
        mgr.set_active_variant("ab", Some("formal")).unwrap();
        let r = mgr.render("ab", &vars(&[("x", "Bob")])).unwrap();
        assert_eq!(r, "Dear Bob");

        // Switch to casual.
        mgr.set_active_variant("ab", Some("casual")).unwrap();
        let r = mgr.render("ab", &vars(&[("x", "Bob")])).unwrap();
        assert_eq!(r, "Hey Bob!");

        // Clear variant — back to base.
        mgr.set_active_variant("ab", None).unwrap();
        let r = mgr.render("ab", &vars(&[("x", "Bob")])).unwrap();
        assert_eq!(r, "base: Bob");
    }

    // -----------------------------------------------------------------------
    // 19. Set variant — not found error
    // -----------------------------------------------------------------------
    #[test]
    fn test_variant_not_found() {
        let tmpl = PromptTemplate::new("vn", "VN", "content");
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);
        let err = mgr.set_active_variant("vn", Some("nope")).unwrap_err();
        assert!(matches!(err, PromptError::VariantNotFound(_, _)));
    }

    // -----------------------------------------------------------------------
    // 20. Default variable value from TemplateVariable
    // -----------------------------------------------------------------------
    #[test]
    fn test_variable_default() {
        let tmpl = PromptTemplate::new("vd", "VD", "Hello {{name}}")
            .with_variable(TemplateVariable::optional("name", "World"));
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);
        let result = mgr.render("vd", &HashMap::new()).unwrap();
        assert_eq!(result, "Hello World");
    }

    // -----------------------------------------------------------------------
    // 21. PromptChain — basic two-step
    // -----------------------------------------------------------------------
    #[test]
    fn test_chain_basic() {
        let mgr = PromptManager::new();
        mgr.register_template(
            PromptTemplate::new("step_a", "A", "Classified: {{message}}")
                .with_variable(TemplateVariable::required("message")),
        );
        mgr.register_template(
            PromptTemplate::new("step_b", "B", "Response to: {{input}}")
                .with_variable(TemplateVariable::required("input")),
        );

        let mut chain = PromptChain::new("test_chain");
        chain.add_step("step_a", vars(&[("message", "message")]));
        chain.add_step("step_b", vars(&[("input", "step_0_output")]));

        let outputs = chain
            .render(&mgr, &vars(&[("message", "help me")]))
            .unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0], "Classified: help me");
        assert_eq!(outputs[1], "Response to: Classified: help me");
    }

    // -----------------------------------------------------------------------
    // 22. PromptChain — literal mapping
    // -----------------------------------------------------------------------
    #[test]
    fn test_chain_literal() {
        let mgr = PromptManager::new();
        mgr.register_template(
            PromptTemplate::new("lit", "Lit", "Value: {{x}}")
                .with_variable(TemplateVariable::required("x")),
        );

        let mut chain = PromptChain::new("lit_chain");
        chain.add_step("lit", vars(&[("x", "literal:42")]));

        let outputs = chain.render(&mgr, &HashMap::new()).unwrap();
        assert_eq!(outputs[0], "Value: 42");
    }

    // -----------------------------------------------------------------------
    // 23. PromptChain — error propagation
    // -----------------------------------------------------------------------
    #[test]
    fn test_chain_error_propagation() {
        let mgr = PromptManager::new();
        let mut chain = PromptChain::new("bad");
        chain.add_step("missing_template", HashMap::new());
        let err = chain.render(&mgr, &HashMap::new()).unwrap_err();
        assert!(matches!(err, PromptError::TemplateNotFound(_)));
    }

    // -----------------------------------------------------------------------
    // 24. Pre-built: sales_qualifier_v1
    // -----------------------------------------------------------------------
    #[test]
    fn test_prebuilt_sales_qualifier() {
        let mgr = PromptManager::new();
        register_xcapit_templates(&mgr);
        let result = mgr
            .render(
                "sales_qualifier_v1",
                &vars(&[
                    ("company", "Acme Corp"),
                    ("region", "latam"),
                    ("title", "CTO"),
                    ("score", "85"),
                    ("affinity", "0.92"),
                ]),
            )
            .unwrap();
        assert!(result.contains("LATAM"));
        assert!(result.contains("Acme Corp"));
        assert!(result.contains("85"));
    }

    // -----------------------------------------------------------------------
    // 25. Pre-built: outreach_composer_v1
    // -----------------------------------------------------------------------
    #[test]
    fn test_prebuilt_outreach_composer() {
        let mgr = PromptManager::new();
        register_xcapit_templates(&mgr);
        let result = mgr
            .render(
                "outreach_composer_v1",
                &vars(&[
                    ("company", "FinCo"),
                    ("contact", "Jane"),
                    ("channel", "EMAIL"),
                    ("region", "EMEA"),
                    ("score_class", "hot"),
                ]),
            )
            .unwrap();
        assert!(result.contains("email")); // channel|lower
        assert!(result.contains("hot"));
    }

    // -----------------------------------------------------------------------
    // 26. Pre-built: support_responder_v1
    // -----------------------------------------------------------------------
    #[test]
    fn test_prebuilt_support_responder() {
        let mgr = PromptManager::new();
        register_xcapit_templates(&mgr);
        let result = mgr
            .render(
                "support_responder_v1",
                &vars(&[
                    ("ticket_category", "billing"),
                    ("priority", "high"),
                    ("customer_message", "I was double charged!"),
                    ("kb_context", "Refund policy: 30 days."),
                ]),
            )
            .unwrap();
        assert!(result.contains("HIGH")); // priority|upper
        assert!(result.contains("Refund policy"));
    }

    // -----------------------------------------------------------------------
    // 27. Pre-built: ticket_router_v1
    // -----------------------------------------------------------------------
    #[test]
    fn test_prebuilt_ticket_router() {
        let mgr = PromptManager::new();
        register_xcapit_templates(&mgr);
        let result = mgr
            .render(
                "ticket_router_v1",
                &vars(&[("message", "My payment failed and I need help")]),
            )
            .unwrap();
        assert!(result.contains("My payment failed"));
        assert!(result.contains("category"));
    }

    // -----------------------------------------------------------------------
    // 28. Thread safety — clone manager across threads
    // -----------------------------------------------------------------------
    #[test]
    fn test_thread_safety() {
        let mgr = PromptManager::new();
        mgr.register_template(
            PromptTemplate::new("ts", "TS", "Hello {{name}}")
                .with_variable(TemplateVariable::required("name")),
        );

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let mgr = mgr.clone();
                std::thread::spawn(move || {
                    let v = vars(&[("name", &format!("thread-{i}"))]);
                    mgr.render("ts", &v).unwrap()
                })
            })
            .collect();

        for (i, h) in handles.into_iter().enumerate() {
            let r = h.join().unwrap();
            assert_eq!(r, format!("Hello thread-{i}"));
        }
    }

    // -----------------------------------------------------------------------
    // 29. Combined filters and blocks
    // -----------------------------------------------------------------------
    #[test]
    fn test_combined_features() {
        let tmpl = PromptTemplate::new(
            "combo",
            "Combo",
            "{{greeting|upper}} {{name|default:Guest}}! {{#if vip}}VIP access granted.{{/if}}",
        );
        let mgr = PromptManager::new();
        mgr.register_template(tmpl);

        let result = mgr
            .render("combo", &vars(&[("greeting", "hello"), ("vip", "true")]))
            .unwrap();
        assert_eq!(result, "HELLO Guest! VIP access granted.");
    }

    // -----------------------------------------------------------------------
    // 30. TemplateSummary conversion
    // -----------------------------------------------------------------------
    #[test]
    fn test_template_summary() {
        let tmpl = PromptTemplate::new("sum", "Summary Test", "content")
            .with_tag("a")
            .with_tag("b")
            .with_variant("v1", "alt content");
        let summary = TemplateSummary::from(&tmpl);
        assert_eq!(summary.id, "sum");
        assert_eq!(summary.tags.len(), 2);
        assert_eq!(summary.variant_count, 1);
    }

    // -----------------------------------------------------------------------
    // 31. Default manager (Default trait)
    // -----------------------------------------------------------------------
    #[test]
    fn test_default_manager() {
        let mgr = PromptManager::default();
        assert!(mgr.list_templates().is_empty());
    }

    // -----------------------------------------------------------------------
    // 32. Get history for unknown template returns empty vec
    // -----------------------------------------------------------------------
    #[test]
    fn test_history_empty() {
        let mgr = PromptManager::new();
        assert!(mgr.get_history("nope").is_empty());
    }
}
