use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;

/// A single matched log line with extracted named capture groups.
///
/// When the regex contains named groups like `(?P<level>\w+)`, each match
/// populates the `captures` map with `{ "level": "ERROR" }` etc.
/// Lines without named groups still match but produce an empty map.
#[derive(Debug, Clone, Serialize)]
pub struct ParsedLine {
    /// 1-based line number in the source file.
    pub line_number: u64,
    /// The raw text of the matched line.
    pub raw: String,
    /// Named capture group values extracted from the line.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub captures: HashMap<String, String>,
}

/// Aggregated statistics accumulated across the entire file.
///
/// Field frequencies track how often each distinct value appears for every
/// named capture group, enabling reports like "HTTP 404 appeared 12,345 times".
#[derive(Debug)]
pub struct Summary {
    /// Total number of lines read from the file.
    pub total_lines: u64,
    /// Number of lines that matched the regex.
    pub matched_lines: u64,
    /// Total bytes read from the file.
    pub bytes_processed: u64,
    /// Per-field value frequencies: field_name → { value → count }.
    pub field_frequencies: HashMap<String, HashMap<String, u64>>,
    /// Wall-clock time for the entire processing run.
    pub elapsed: Duration,
}

impl Summary {
    /// Create a zeroed summary.
    pub fn new() -> Self {
        Self {
            total_lines: 0,
            matched_lines: 0,
            bytes_processed: 0,
            field_frequencies: HashMap::new(),
            elapsed: Duration::ZERO,
        }
    }

    /// Merge a batch result into this summary.
    pub fn merge_batch(&mut self, matched: u64, freqs: HashMap<String, HashMap<String, u64>>) {
        self.matched_lines += matched;
        for (field, values) in freqs {
            let entry = self.field_frequencies.entry(field).or_default();
            for (value, count) in values {
                *entry.entry(value).or_insert(0) += count;
            }
        }
    }
}

impl Default for Summary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_merge() {
        let mut summary = Summary::new();

        let mut freqs1 = HashMap::new();
        let mut level_freq = HashMap::new();
        level_freq.insert("ERROR".to_string(), 5);
        level_freq.insert("WARN".to_string(), 3);
        freqs1.insert("level".to_string(), level_freq);

        summary.merge_batch(8, freqs1);
        assert_eq!(summary.matched_lines, 8);
        assert_eq!(summary.field_frequencies["level"]["ERROR"], 5);
        assert_eq!(summary.field_frequencies["level"]["WARN"], 3);

        let mut freqs2 = HashMap::new();
        let mut level_freq2 = HashMap::new();
        level_freq2.insert("ERROR".to_string(), 2);
        level_freq2.insert("INFO".to_string(), 10);
        freqs2.insert("level".to_string(), level_freq2);

        summary.merge_batch(12, freqs2);
        assert_eq!(summary.matched_lines, 20);
        assert_eq!(summary.field_frequencies["level"]["ERROR"], 7);
        assert_eq!(summary.field_frequencies["level"]["INFO"], 10);
        assert_eq!(summary.field_frequencies["level"]["WARN"], 3);
    }

    #[test]
    fn test_parsed_line_serialization() {
        let line = ParsedLine {
            line_number: 42,
            raw: "2024-01-15 ERROR something broke".to_string(),
            captures: {
                let mut m = HashMap::new();
                m.insert("level".to_string(), "ERROR".to_string());
                m
            },
        };
        let json = serde_json::to_string(&line).unwrap();
        assert!(json.contains("\"line_number\":42"));
        assert!(json.contains("\"level\":\"ERROR\""));
    }

    #[test]
    fn test_parsed_line_empty_captures_skipped() {
        let line = ParsedLine {
            line_number: 1,
            raw: "hello".to_string(),
            captures: HashMap::new(),
        };
        let json = serde_json::to_string(&line).unwrap();
        assert!(!json.contains("captures"));
    }
}
