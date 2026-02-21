/// Input sanitizer to prevent log poisoning, injection attacks, and malformed input.
pub struct Sanitizer {
    max_message_length: usize,
}

impl Default for Sanitizer {
    fn default() -> Self {
        Self {
            max_message_length: 100_000,
        }
    }
}

impl Sanitizer {
    pub fn new(max_message_length: usize) -> Self {
        Self { max_message_length }
    }

    /// Sanitize a string: strip control characters, validate UTF-8, enforce length limits.
    pub fn sanitize(&self, input: &str) -> SanitizeResult {
        if input.len() > self.max_message_length {
            return SanitizeResult::Rejected("Input exceeds maximum length".to_string());
        }

        let cleaned: String = input
            .chars()
            .filter(|c| {
                // Allow printable characters, newlines, tabs
                !c.is_control() || *c == '\n' || *c == '\t' || *c == '\r'
            })
            .collect();

        if cleaned.is_empty() && !input.is_empty() {
            return SanitizeResult::Rejected("Input contains only control characters".to_string());
        }

        if cleaned != input {
            SanitizeResult::Cleaned(cleaned)
        } else {
            SanitizeResult::Clean(cleaned)
        }
    }

    /// Sanitize HTTP headers to prevent log poisoning.
    pub fn sanitize_header(&self, value: &str) -> String {
        value
            .chars()
            .filter(|c| c.is_ascii_graphic() || *c == ' ')
            .take(1000)
            .collect()
    }
}

#[derive(Debug, PartialEq)]
pub enum SanitizeResult {
    /// Input was already clean.
    Clean(String),
    /// Input was cleaned (control characters removed).
    Cleaned(String),
    /// Input was rejected entirely.
    Rejected(String),
}

impl SanitizeResult {
    pub fn is_rejected(&self) -> bool {
        matches!(self, SanitizeResult::Rejected(_))
    }

    pub fn into_string(self) -> Option<String> {
        match self {
            SanitizeResult::Clean(s) | SanitizeResult::Cleaned(s) => Some(s),
            SanitizeResult::Rejected(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_input() {
        let s = Sanitizer::default();
        let result = s.sanitize("Hello world\nNew line");
        assert!(matches!(result, SanitizeResult::Clean(_)));
    }

    #[test]
    fn test_control_chars_stripped() {
        let s = Sanitizer::default();
        let input = "Hello\x00\x01\x02World";
        let result = s.sanitize(input);
        assert_eq!(result, SanitizeResult::Cleaned("HelloWorld".to_string()));
    }

    #[test]
    fn test_length_rejection() {
        let s = Sanitizer::new(10);
        let result = s.sanitize("This is too long for the limit");
        assert!(result.is_rejected());
    }

    #[test]
    fn test_header_sanitization() {
        let s = Sanitizer::default();
        let header = "normal-value\x00\x1b[31minjected\x1b[0m";
        let clean = s.sanitize_header(header);
        assert!(!clean.contains('\x00'));
        assert!(!clean.contains('\x1b'));
    }
}
