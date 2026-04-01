//! Structured output parsing and validation for LLM responses.
//!
//! Extracts structured data (JSON, key-value pairs, lists, code blocks)
//! from free-text LLM responses using configurable extraction patterns.
//!
//! # Main types
//!
//! - [`StructuredOutputParser`] — Extracts structured data from LLM text.
//! - [`OutputSchema`] — Defines expected fields with types and constraints.
//! - [`ExtractedOutput`] — The parsed result with validation status.
//! - [`ExtractionPattern`] — Strategy for locating structured content.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// FieldType
// ---------------------------------------------------------------------------

/// Expected type for a field in the output schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// A string value.
    String,
    /// An integer number.
    Integer,
    /// A floating-point number.
    Number,
    /// A boolean value.
    Boolean,
    /// A JSON array.
    Array,
    /// A JSON object.
    Object,
    /// Any valid JSON value.
    Any,
}

// ---------------------------------------------------------------------------
// FieldDefinition
// ---------------------------------------------------------------------------

/// Definition of a single expected field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    /// Name of the field.
    pub name: String,
    /// Expected type.
    pub field_type: FieldType,
    /// Whether this field must be present.
    pub required: bool,
    /// Optional description (used to generate prompts).
    pub description: Option<String>,
    /// Optional default value if the field is missing.
    pub default: Option<serde_json::Value>,
}

impl FieldDefinition {
    /// Create a new required field definition.
    pub fn required(name: impl Into<String>, field_type: FieldType) -> Self {
        Self {
            name: name.into(),
            field_type,
            required: true,
            description: None,
            default: None,
        }
    }

    /// Create a new optional field with a default value.
    pub fn optional(
        name: impl Into<String>,
        field_type: FieldType,
        default: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            field_type,
            required: false,
            description: None,
            default: Some(default),
        }
    }

    /// Add a description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

// ---------------------------------------------------------------------------
// OutputSchema
// ---------------------------------------------------------------------------

/// Defines the expected structure of an LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSchema {
    /// Name for this schema.
    pub name: String,
    /// Expected fields.
    pub fields: Vec<FieldDefinition>,
}

impl OutputSchema {
    /// Create a new schema with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fields: Vec::new(),
        }
    }

    /// Add a field definition.
    pub fn with_field(mut self, field: FieldDefinition) -> Self {
        self.fields.push(field);
        self
    }

    /// Generate a JSON schema string for prompt injection.
    pub fn to_json_schema(&self) -> String {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for field in &self.fields {
            let type_str = match field.field_type {
                FieldType::String => "string",
                FieldType::Integer => "integer",
                FieldType::Number => "number",
                FieldType::Boolean => "boolean",
                FieldType::Array => "array",
                FieldType::Object => "object",
                FieldType::Any => "any",
            };

            let mut prop = serde_json::Map::new();
            prop.insert(
                "type".to_string(),
                serde_json::Value::String(type_str.to_string()),
            );
            if let Some(desc) = &field.description {
                prop.insert(
                    "description".to_string(),
                    serde_json::Value::String(desc.clone()),
                );
            }
            properties.insert(field.name.clone(), serde_json::Value::Object(prop));

            if field.required {
                required.push(serde_json::Value::String(field.name.clone()));
            }
        }

        let schema = serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
        });

        serde_json::to_string_pretty(&schema).unwrap_or_default()
    }

    /// Generate a prompt instruction asking the LLM to respond in this schema.
    pub fn to_prompt_instruction(&self) -> String {
        let schema = self.to_json_schema();
        format!(
            "Respond with a JSON object matching this schema:\n```json\n{schema}\n```\nDo not include any text outside the JSON object."
        )
    }
}

// ---------------------------------------------------------------------------
// ExtractionPattern
// ---------------------------------------------------------------------------

/// Strategy for locating structured content within LLM output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionPattern {
    /// Extract the first JSON object found in the text.
    JsonBlock,
    /// Extract JSON from a markdown code block (```json ... ```).
    MarkdownCodeBlock,
    /// Extract key-value pairs in "Key: Value" format.
    KeyValuePairs,
    /// Extract a numbered or bulleted list.
    List,
    /// Try all patterns in order and return the first success.
    Auto,
}

// ---------------------------------------------------------------------------
// ValidationError
// ---------------------------------------------------------------------------

/// An error found during output validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Name of the field with the error.
    pub field: String,
    /// Description of the error.
    pub message: String,
}

// ---------------------------------------------------------------------------
// ExtractedOutput
// ---------------------------------------------------------------------------

/// The result of structured output extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedOutput {
    /// The extracted data as a JSON value.
    pub data: serde_json::Value,
    /// Whether the output fully validates against the schema.
    pub is_valid: bool,
    /// Validation errors found.
    pub errors: Vec<ValidationError>,
    /// Which extraction pattern succeeded.
    pub pattern_used: String,
    /// Fields that were filled with default values.
    pub defaulted_fields: Vec<String>,
}

// ---------------------------------------------------------------------------
// StructuredOutputParser
// ---------------------------------------------------------------------------

/// Extracts and validates structured data from LLM text output.
pub struct StructuredOutputParser {
    schema: OutputSchema,
    pattern: ExtractionPattern,
}

impl StructuredOutputParser {
    /// Create a new parser with the given schema and extraction pattern.
    pub fn new(schema: OutputSchema, pattern: ExtractionPattern) -> Self {
        Self { schema, pattern }
    }

    /// Parse the LLM output text and extract structured data.
    pub fn parse(&self, text: &str) -> ExtractedOutput {
        match &self.pattern {
            ExtractionPattern::JsonBlock => self.extract_json_block(text),
            ExtractionPattern::MarkdownCodeBlock => self.extract_markdown_code_block(text),
            ExtractionPattern::KeyValuePairs => self.extract_key_value_pairs(text),
            ExtractionPattern::List => self.extract_list(text),
            ExtractionPattern::Auto => self.extract_auto(text),
        }
    }

    /// Try all extraction patterns in order.
    fn extract_auto(&self, text: &str) -> ExtractedOutput {
        // Try markdown code block first (most structured)
        let result = self.extract_markdown_code_block(text);
        if result.is_valid {
            return result;
        }

        // Try raw JSON block
        let result = self.extract_json_block(text);
        if result.is_valid {
            return result;
        }

        // Try key-value pairs
        let result = self.extract_key_value_pairs(text);
        if result.is_valid {
            return result;
        }

        // Fall back to list
        self.extract_list(text)
    }

    /// Extract JSON from markdown code block.
    fn extract_markdown_code_block(&self, text: &str) -> ExtractedOutput {
        // Look for ```json ... ``` or ``` ... ```
        let json_str = extract_code_block(text);

        match json_str {
            Some(json) => self.parse_and_validate(&json, "markdown_code_block"),
            None => self.empty_result("markdown_code_block"),
        }
    }

    /// Extract the first JSON object found in text.
    fn extract_json_block(&self, text: &str) -> ExtractedOutput {
        let json_str = extract_first_json_object(text);

        match json_str {
            Some(json) => self.parse_and_validate(&json, "json_block"),
            None => self.empty_result("json_block"),
        }
    }

    /// Extract key-value pairs.
    fn extract_key_value_pairs(&self, text: &str) -> ExtractedOutput {
        let mut map = serde_json::Map::new();

        for line in text.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_lowercase().replace(' ', "_");
                let value = value.trim();
                // Try to parse as JSON value, fallback to string
                let json_val = serde_json::from_str(value)
                    .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
                map.insert(key, json_val);
            }
        }

        if map.is_empty() {
            return self.empty_result("key_value_pairs");
        }

        let data = serde_json::Value::Object(map);
        self.validate(data, "key_value_pairs")
    }

    /// Extract a list from text.
    fn extract_list(&self, text: &str) -> ExtractedOutput {
        let mut items = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            // Match numbered (1. item) or bulleted (- item, * item) lists
            let content = if let Some(rest) = line.strip_prefix("- ") {
                Some(rest)
            } else if let Some(rest) = line.strip_prefix("* ") {
                Some(rest)
            } else if line.len() > 2 && line.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                // Match "1. item" or "1) item"
                line.find(". ")
                    .map(|i| &line[i + 2..])
                    .or_else(|| line.find(") ").map(|i| &line[i + 2..]))
            } else {
                None
            };

            if let Some(content) = content {
                items.push(serde_json::Value::String(content.trim().to_string()));
            }
        }

        if items.is_empty() {
            return self.empty_result("list");
        }

        let data = serde_json::json!({ "items": items });
        self.validate(data, "list")
    }

    /// Parse a JSON string and validate against the schema.
    fn parse_and_validate(&self, json_str: &str, pattern: &str) -> ExtractedOutput {
        match serde_json::from_str::<serde_json::Value>(json_str) {
            Ok(data) => self.validate(data, pattern),
            Err(_) => self.empty_result(pattern),
        }
    }

    /// Validate extracted data against the schema and apply defaults.
    fn validate(&self, mut data: serde_json::Value, pattern: &str) -> ExtractedOutput {
        let mut errors = Vec::new();
        let mut defaulted_fields = Vec::new();

        if let Some(obj) = data.as_object_mut() {
            for field in &self.schema.fields {
                match obj.get(&field.name) {
                    Some(value) => {
                        if !type_matches(value, &field.field_type) {
                            errors.push(ValidationError {
                                field: field.name.clone(),
                                message: format!(
                                    "Expected type {:?}, got {}",
                                    field.field_type,
                                    value_type_name(value)
                                ),
                            });
                        }
                    }
                    None => {
                        if field.required {
                            if let Some(default) = &field.default {
                                obj.insert(field.name.clone(), default.clone());
                                defaulted_fields.push(field.name.clone());
                            } else {
                                errors.push(ValidationError {
                                    field: field.name.clone(),
                                    message: "Required field missing".to_string(),
                                });
                            }
                        } else if let Some(default) = &field.default {
                            obj.insert(field.name.clone(), default.clone());
                            defaulted_fields.push(field.name.clone());
                        }
                    }
                }
            }
        }

        ExtractedOutput {
            data,
            is_valid: errors.is_empty(),
            errors,
            pattern_used: pattern.to_string(),
            defaulted_fields,
        }
    }

    /// Return an empty/invalid extraction result.
    fn empty_result(&self, pattern: &str) -> ExtractedOutput {
        ExtractedOutput {
            data: serde_json::Value::Null,
            is_valid: false,
            errors: vec![ValidationError {
                field: "_root".to_string(),
                message: format!("No structured content found using {pattern} pattern"),
            }],
            pattern_used: pattern.to_string(),
            defaulted_fields: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract content from a markdown code block.
fn extract_code_block(text: &str) -> Option<String> {
    let start_markers = ["```json\n", "```json\r\n", "```\n", "```\r\n"];

    for marker in &start_markers {
        if let Some(start) = text.find(marker) {
            let content_start = start + marker.len();
            if let Some(end) = text[content_start..].find("```") {
                return Some(text[content_start..content_start + end].trim().to_string());
            }
        }
    }
    None
}

/// Extract the first JSON object from text.
fn extract_first_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in text[start..].char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start..start + i + 1].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Check if a JSON value matches the expected field type.
fn type_matches(value: &serde_json::Value, expected: &FieldType) -> bool {
    match expected {
        FieldType::String => value.is_string(),
        FieldType::Integer => value.is_i64() || value.is_u64(),
        FieldType::Number => value.is_number(),
        FieldType::Boolean => value.is_boolean(),
        FieldType::Array => value.is_array(),
        FieldType::Object => value.is_object(),
        FieldType::Any => true,
    }
}

/// Human-readable type name for a JSON value.
fn value_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_schema() -> OutputSchema {
        OutputSchema::new("test")
            .with_field(FieldDefinition::required("name", FieldType::String))
            .with_field(FieldDefinition::required("age", FieldType::Integer))
            .with_field(FieldDefinition::optional(
                "active",
                FieldType::Boolean,
                serde_json::Value::Bool(true),
            ))
    }

    // 1. Parse JSON from markdown code block
    #[test]
    fn test_markdown_code_block() {
        let parser =
            StructuredOutputParser::new(test_schema(), ExtractionPattern::MarkdownCodeBlock);
        let text = r#"Here is the result:
```json
{"name": "Alice", "age": 30}
```"#;
        let result = parser.parse(text);
        assert!(result.is_valid);
        assert_eq!(result.data["name"], "Alice");
        assert_eq!(result.data["age"], 30);
    }

    // 2. Parse raw JSON block
    #[test]
    fn test_json_block() {
        let parser = StructuredOutputParser::new(test_schema(), ExtractionPattern::JsonBlock);
        let text = r#"The output is {"name": "Bob", "age": 25} and some trailing text."#;
        let result = parser.parse(text);
        assert!(result.is_valid);
        assert_eq!(result.data["name"], "Bob");
    }

    // 3. Missing required field
    #[test]
    fn test_missing_required_field() {
        let parser = StructuredOutputParser::new(test_schema(), ExtractionPattern::JsonBlock);
        let text = r#"{"name": "Eve"}"#;
        let result = parser.parse(text);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.field == "age"));
    }

    // 4. Default value applied for optional field
    #[test]
    fn test_default_value() {
        let parser = StructuredOutputParser::new(test_schema(), ExtractionPattern::JsonBlock);
        let text = r#"{"name": "Charlie", "age": 40}"#;
        let result = parser.parse(text);
        assert!(result.is_valid);
        assert_eq!(result.data["active"], true);
        assert!(result.defaulted_fields.contains(&"active".to_string()));
    }

    // 5. Type mismatch error
    #[test]
    fn test_type_mismatch() {
        let parser = StructuredOutputParser::new(test_schema(), ExtractionPattern::JsonBlock);
        let text = r#"{"name": "Dana", "age": "not a number"}"#;
        let result = parser.parse(text);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.field == "age"));
    }

    // 6. Key-value extraction
    #[test]
    fn test_key_value_extraction() {
        let schema = OutputSchema::new("kv")
            .with_field(FieldDefinition::required("status", FieldType::String));
        let parser = StructuredOutputParser::new(schema, ExtractionPattern::KeyValuePairs);
        let text = "Status: success\nDuration: 42\nMessage: all good";
        let result = parser.parse(text);
        assert!(result.is_valid);
        assert_eq!(result.data["status"], "success");
    }

    // 7. List extraction
    #[test]
    fn test_list_extraction() {
        let schema = OutputSchema::new("list")
            .with_field(FieldDefinition::required("items", FieldType::Array));
        let parser = StructuredOutputParser::new(schema, ExtractionPattern::List);
        let text = "Here are the items:\n- Apple\n- Banana\n- Cherry";
        let result = parser.parse(text);
        assert!(result.is_valid);
        let items = result.data["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], "Apple");
    }

    // 8. List with numbered items
    #[test]
    fn test_numbered_list() {
        let schema = OutputSchema::new("list")
            .with_field(FieldDefinition::required("items", FieldType::Array));
        let parser = StructuredOutputParser::new(schema, ExtractionPattern::List);
        let text = "Steps:\n1. First step\n2. Second step\n3. Third step";
        let result = parser.parse(text);
        assert!(result.is_valid);
        let items = result.data["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
    }

    // 9. Auto pattern falls through
    #[test]
    fn test_auto_pattern_json() {
        let parser = StructuredOutputParser::new(test_schema(), ExtractionPattern::Auto);
        let text = r#"```json
{"name": "Auto", "age": 10}
```"#;
        let result = parser.parse(text);
        assert!(result.is_valid);
        assert_eq!(result.pattern_used, "markdown_code_block");
    }

    // 10. Auto pattern falls back to JSON block
    #[test]
    fn test_auto_pattern_fallback() {
        let parser = StructuredOutputParser::new(test_schema(), ExtractionPattern::Auto);
        let text = r#"Result: {"name": "Fallback", "age": 20}"#;
        let result = parser.parse(text);
        assert!(result.is_valid);
        assert_eq!(result.pattern_used, "json_block");
    }

    // 11. No structured content found
    #[test]
    fn test_no_content_found() {
        let parser = StructuredOutputParser::new(test_schema(), ExtractionPattern::JsonBlock);
        let text = "Just some plain text without any JSON.";
        let result = parser.parse(text);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    // 12. Nested JSON objects
    #[test]
    fn test_nested_json() {
        let schema = OutputSchema::new("nested")
            .with_field(FieldDefinition::required("data", FieldType::Object));
        let parser = StructuredOutputParser::new(schema, ExtractionPattern::JsonBlock);
        let text = r#"{"data": {"key": "value", "nested": {"deep": true}}}"#;
        let result = parser.parse(text);
        assert!(result.is_valid);
        assert!(result.data["data"]["nested"]["deep"].as_bool().unwrap());
    }

    // 13. Schema to JSON schema
    #[test]
    fn test_to_json_schema() {
        let schema = test_schema();
        let json_schema = schema.to_json_schema();
        let parsed: serde_json::Value = serde_json::from_str(&json_schema).unwrap();
        assert_eq!(parsed["type"], "object");
        assert!(parsed["properties"]["name"].is_object());
        let required = parsed["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::Value::String("name".to_string())));
    }

    // 14. Prompt instruction generation
    #[test]
    fn test_prompt_instruction() {
        let schema = test_schema();
        let prompt = schema.to_prompt_instruction();
        assert!(prompt.contains("Respond with a JSON object"));
        assert!(prompt.contains("\"name\""));
    }

    // 15. FieldType::Any accepts everything
    #[test]
    fn test_any_type() {
        let schema =
            OutputSchema::new("any").with_field(FieldDefinition::required("value", FieldType::Any));
        let parser = StructuredOutputParser::new(schema, ExtractionPattern::JsonBlock);

        for text in &[
            r#"{"value": "string"}"#,
            r#"{"value": 42}"#,
            r#"{"value": true}"#,
            r#"{"value": [1, 2]}"#,
        ] {
            let result = parser.parse(text);
            assert!(result.is_valid, "Failed for: {text}");
        }
    }

    // 16. Code block without json language tag
    #[test]
    fn test_code_block_no_lang() {
        let parser =
            StructuredOutputParser::new(test_schema(), ExtractionPattern::MarkdownCodeBlock);
        let text = "```\n{\"name\": \"NoLang\", \"age\": 5}\n```";
        let result = parser.parse(text);
        assert!(result.is_valid);
        assert_eq!(result.data["name"], "NoLang");
    }

    // 17. JSON with escaped quotes
    #[test]
    fn test_json_escaped_quotes() {
        let schema = OutputSchema::new("escaped")
            .with_field(FieldDefinition::required("text", FieldType::String));
        let parser = StructuredOutputParser::new(schema, ExtractionPattern::JsonBlock);
        let text = r#"Output: {"text": "He said \"hello\""}"#;
        let result = parser.parse(text);
        assert!(result.is_valid);
    }

    // 18. Multiple JSON objects — extracts first
    #[test]
    fn test_multiple_json_first() {
        let parser = StructuredOutputParser::new(test_schema(), ExtractionPattern::JsonBlock);
        let text = r#"{"name": "First", "age": 1} and {"name": "Second", "age": 2}"#;
        let result = parser.parse(text);
        assert!(result.is_valid);
        assert_eq!(result.data["name"], "First");
    }

    // 19. ExtractedOutput serializable
    #[test]
    fn test_extracted_output_serializable() {
        let parser = StructuredOutputParser::new(test_schema(), ExtractionPattern::JsonBlock);
        let text = r#"{"name": "Serializable", "age": 99}"#;
        let result = parser.parse(text);
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"is_valid\":true"));
    }

    // 20. OutputSchema serializable
    #[test]
    fn test_schema_serializable() {
        let schema = test_schema();
        let json = serde_json::to_string(&schema).unwrap();
        let restored: OutputSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "test");
        assert_eq!(restored.fields.len(), 3);
    }

    // 21. Key-value with numeric values
    #[test]
    fn test_kv_numeric_values() {
        let schema = OutputSchema::new("kv_num")
            .with_field(FieldDefinition::required("count", FieldType::Number));
        let parser = StructuredOutputParser::new(schema, ExtractionPattern::KeyValuePairs);
        let text = "Count: 42";
        let result = parser.parse(text);
        assert!(result.is_valid);
        assert_eq!(result.data["count"], 42);
    }

    // 22. Empty schema validates any object
    #[test]
    fn test_empty_schema() {
        let schema = OutputSchema::new("empty");
        let parser = StructuredOutputParser::new(schema, ExtractionPattern::JsonBlock);
        let text = r#"{"any": "data"}"#;
        let result = parser.parse(text);
        assert!(result.is_valid);
    }

    // 23. List with asterisk bullets
    #[test]
    fn test_asterisk_list() {
        let schema = OutputSchema::new("list")
            .with_field(FieldDefinition::required("items", FieldType::Array));
        let parser = StructuredOutputParser::new(schema, ExtractionPattern::List);
        let text = "* Item one\n* Item two";
        let result = parser.parse(text);
        assert!(result.is_valid);
        let items = result.data["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
    }

    // 24. Field with description
    #[test]
    fn test_field_with_description() {
        let field = FieldDefinition::required("name", FieldType::String)
            .with_description("The user's full name");
        assert_eq!(field.description.unwrap(), "The user's full name");
    }
}
