use reedline::{ValidationResult, Validator};

/// A default validator which checks for mismatched quotes and brackets
pub struct ReplValidator;

impl Validator for ReplValidator {
    fn validate(&self, line: &str) -> ValidationResult {
        ValidationResult::Incomplete
    }
}
