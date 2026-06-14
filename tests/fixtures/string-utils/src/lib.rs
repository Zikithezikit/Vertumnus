//! A string utility library for testing Vertumnus.
//!
//! This crate provides basic string manipulation functions and
//! a simple text processor struct.

/// Reverses a string.
///
/// # Examples
///
/// ```
/// assert_eq!(string_utils::reverse("hello"), "olleh");
/// ```
pub fn reverse(s: &str) -> String {
    s.chars().rev().collect()
}

/// Counts the number of words in a string.
pub fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Checks if a string is a palindrome.
///
/// Returns `true` if the string reads the same forwards and backwards
/// (ignoring case and whitespace).
pub fn is_palindrome(s: &str) -> bool {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    cleaned == cleaned.chars().rev().collect::<String>()
}

/// Truncates a string to the given length, appending "..." if truncated.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// A simple text processor that can transform strings.
pub struct TextProcessor {
    /// The prefix to add to each processed string
    pub prefix: String,
    /// Whether to convert to uppercase
    pub uppercase: bool,
}

impl TextProcessor {
    /// Create a new `TextProcessor`.
    pub fn new(prefix: String, uppercase: bool) -> Self {
        TextProcessor { prefix, uppercase }
    }

    /// Process a string by adding the prefix and optionally uppercasing.
    pub fn process(&self, input: &str) -> String {
        let mut result = self.prefix.clone();
        let content = if self.uppercase {
            input.to_uppercase()
        } else {
            input.to_string()
        };
        result.push_str(&content);
        result
    }

    /// Returns a greeting message.
    pub fn greet(&self, name: &str) -> String {
        format!("{}{}", self.prefix, name)
    }
}

/// Status of a text processing operation.
#[derive(Debug, PartialEq)]
pub enum ProcessStatus {
    Success,
    EmptyInput,
    TooLong,
}

impl ProcessStatus {
    /// Check if the status indicates success.
    pub fn is_ok(&self) -> bool {
        matches!(self, ProcessStatus::Success)
    }

    /// Get a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            ProcessStatus::Success => "Processing completed successfully",
            ProcessStatus::EmptyInput => "Input was empty",
            ProcessStatus::TooLong => "Input exceeded maximum length",
        }
    }
}
