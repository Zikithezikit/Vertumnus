//! A data structures library for testing Vertumnus.
//!
//! This crate exercises collection types (Vec, HashMap, HashSet),
//! tuples, and nested generics to test the type mapper and generator.

use std::collections::{HashMap, HashSet};

/// Returns the sum of a list of integers.
pub fn sum_list(values: Vec<i64>) -> i64 {
    values.iter().sum()
}

/// Returns a sorted list of unique values.
pub fn unique_sorted(values: Vec<i64>) -> Vec<i64> {
    let mut sorted = values.clone();
    sorted.sort();
    sorted.dedup();
    sorted
}

/// Counts word frequencies in a string.
pub fn word_frequencies(text: &str) -> HashMap<String, usize> {
    let mut freq = HashMap::new();
    for word in text.split_whitespace() {
        *freq.entry(word.to_string()).or_insert(0) += 1;
    }
    freq
}

/// Computes a set of unique words from a text.
pub fn unique_words(text: &str) -> HashSet<String> {
    text.split_whitespace().map(|w| w.to_string()).collect()
}

/// Looks up a value in a map, returning a default if missing.
pub fn lookup_or_default(map: HashMap<String, i64>, key: String, default: i64) -> i64 {
    map.get(&key).copied().unwrap_or(default)
}

/// Merges two maps, with the second map overwriting the first.
pub fn merge_maps(a: HashMap<String, i64>, b: HashMap<String, i64>) -> HashMap<String, i64> {
    let mut result = a;
    for (k, v) in b {
        result.insert(k, v);
    }
    result
}

/// Finds the intersection of two sets.
pub fn intersect_sets(a: HashSet<i64>, b: HashSet<i64>) -> Vec<i64> {
    let mut result: Vec<i64> = a.intersection(&b).copied().collect();
    result.sort();
    result
}

/// Returns the first and last elements of a list, if non-empty.
pub fn first_and_last(values: Vec<i64>) -> Option<(i64, i64)> {
    let first = values.first()?;
    let last = values.last()?;
    Some((*first, *last))
}

/// Splits a tuple pair into two lists.
pub fn unzip_pairs(pairs: Vec<(String, i64)>) -> (Vec<String>, Vec<i64>) {
    pairs.into_iter().unzip()
}

/// Converts a list of options, filtering out None.
pub fn flatten_options(values: Vec<Option<i64>>) -> Vec<i64> {
    values.into_iter().flatten().collect()
}

/// A generic result type for validation operations.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    EmptyInput,
    TooLong { max: usize, actual: usize },
    InvalidCharacter(char),
}

/// Validates that a string meets certain criteria.
pub fn validate_string(s: &str, max_len: usize) -> Result<String, ValidationError> {
    if s.is_empty() {
        return Err(ValidationError::EmptyInput);
    }
    if s.len() > max_len {
        return Err(ValidationError::TooLong {
            max: max_len,
            actual: s.len(),
        });
    }
    for c in s.chars() {
        if !c.is_alphanumeric() && c != ' ' {
            return Err(ValidationError::InvalidCharacter(c));
        }
    }
    Ok(s.to_string())
}

/// A container for a collection of named values.
pub struct DataStore {
    /// The name of this store
    pub name: String,
    /// The stored values
    pub values: Vec<i64>,
    /// Named entries
    pub entries: HashMap<String, f64>,
}

impl DataStore {
    /// Create a new DataStore.
    pub fn new(name: String) -> Self {
        DataStore {
            name,
            values: Vec::new(),
            entries: HashMap::new(),
        }
    }

    /// Add a value to the store.
    pub fn add_value(&mut self, val: i64) {
        self.values.push(val);
    }

    /// Add a named entry.
    pub fn add_entry(&mut self, key: String, val: f64) {
        self.entries.insert(key, val);
    }

    /// Compute the sum of all values.
    pub fn total(&self) -> i64 {
        self.values.iter().sum()
    }

    /// Returns the average of values, or None if empty.
    pub fn average(&self) -> Option<f64> {
        if self.values.is_empty() {
            None
        } else {
            let sum: i64 = self.values.iter().sum();
            Some(sum as f64 / self.values.len() as f64)
        }
    }

    /// Merge another DataStore into this one.
    pub fn merge(&mut self, other: &DataStore) {
        self.values.extend_from_slice(&other.values);
        for (k, v) in &other.entries {
            self.entries.insert(k.clone(), *v);
        }
    }
}

/// A counter that can be incremented and queried.
pub struct Counter {
    /// The current count
    pub count: i64,
    /// A history of all values the counter has held
    pub history: Vec<i64>,
}

impl Counter {
    /// Create a new Counter starting at 0.
    pub fn new() -> Self {
        Counter {
            count: 0,
            history: vec![0],
        }
    }

    /// Increment the counter by `amount`.
    pub fn increment(&mut self, amount: i64) -> i64 {
        self.count += amount;
        self.history.push(self.count);
        self.count
    }

    /// Reset the counter to 0.
    pub fn reset(&mut self) {
        self.count = 0;
        self.history.push(0);
    }

    /// Get the full history of count values.
    pub fn get_history(&self) -> Vec<i64> {
        self.history.clone()
    }
}

/// Status of a data processing operation.
#[derive(Debug, Clone, PartialEq)]
pub enum OpStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

impl OpStatus {
    /// Returns true if the operation is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, OpStatus::Completed | OpStatus::Failed(_))
    }

    /// Returns a human-readable label.
    pub fn label(&self) -> &str {
        match self {
            OpStatus::Pending => "Pending",
            OpStatus::Running => "Running",
            OpStatus::Completed => "Completed",
            OpStatus::Failed(_) => "Failed",
        }
    }
}

/// Simple value class — a C-like enum with no data variants.
#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    Red,
    Green,
    Blue,
    Yellow,
}

impl Color {
    /// Convert the color to a hex-like numeric code.
    pub fn code(&self) -> i64 {
        match self {
            Color::Red => 0xFF0000,
            Color::Green => 0x00FF00,
            Color::Blue => 0x0000FF,
            Color::Yellow => 0xFFFF00,
        }
    }
}
