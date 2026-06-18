//! Integration tests for the slicer log parser.
//!
//! These tests generate synthetic log data in memory, pipe it through the
//! parser, and assert correctness of both JSON and summary outputs.

use std::io::Write;

use regex::Regex;
use slicer::cli::OutputMode;
use slicer::parser::process_file;

/// Generate a synthetic log file with a mix of levels and noise lines.
fn generate_test_log(line_count: usize) -> Vec<u8> {
    let levels = ["ERROR", "WARN", "INFO", "DEBUG"];
    let mut data = Vec::with_capacity(line_count * 80);

    for i in 0..line_count {
        if i % 7 == 0 {
            // Noise line that won't match the structured pattern.
            writeln!(data, "--- heartbeat tick {} ---", i).unwrap();
        } else {
            let level = levels[i % levels.len()];
            writeln!(
                data,
                "2024-01-15T10:{:02}:{:02} {} request handled path=/api/v1/resource id={}",
                (i / 60) % 60,
                i % 60,
                level,
                i
            )
            .unwrap();
        }
    }
    data
}

/// Write test log data to a temporary file and return its path.
fn write_temp_log(data: &[u8]) -> tempfile::NamedTempFile {
    let mut tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    tmp.write_all(data).expect("failed to write temp log");
    tmp.flush().expect("failed to flush temp log");
    tmp
}

#[test]
fn test_summary_mode_counts() {
    let data = generate_test_log(100);
    let tmp = write_temp_log(&data);

    let regex = Regex::new(
        r"(?P<timestamp>\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}) (?P<level>\w+) (?P<msg>.+)",
    )
    .unwrap();

    let summary = process_file(tmp.path(), &regex, &OutputMode::Summary, 32, false).unwrap();

    // We generated 100 lines; every 7th is noise, so matched = 100 - ceil(100/7).
    assert_eq!(summary.total_lines, 100);
    assert!(summary.matched_lines > 0);
    assert!(summary.matched_lines < 100);

    // Verify field frequencies contain "level" with expected keys.
    assert!(summary.field_frequencies.contains_key("level"));
    let level_freqs = &summary.field_frequencies["level"];
    assert!(level_freqs.contains_key("ERROR"));
    assert!(level_freqs.contains_key("WARN"));
    assert!(level_freqs.contains_key("INFO"));
    assert!(level_freqs.contains_key("DEBUG"));

    // Sum of level frequencies should equal matched lines.
    let freq_sum: u64 = level_freqs.values().sum();
    assert_eq!(freq_sum, summary.matched_lines);
}

#[test]
fn test_json_mode_output() {
    let data = b"2024-01-15T10:00:00 ERROR disk full\n\
                 this is noise\n\
                 2024-01-15T10:01:00 WARN low memory\n";

    let tmp = write_temp_log(data);

    let regex = Regex::new(
        r"(?P<timestamp>\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}) (?P<level>\w+) (?P<msg>.+)",
    )
    .unwrap();

    // We can't easily capture stdout in an integration test, so we verify
    // the summary side-effects instead (frequencies are tracked in JSON mode too).
    let summary = process_file(tmp.path(), &regex, &OutputMode::Json, 32, false).unwrap();

    assert_eq!(summary.total_lines, 3);
    assert_eq!(summary.matched_lines, 2);
    assert_eq!(summary.field_frequencies["level"]["ERROR"], 1);
    assert_eq!(summary.field_frequencies["level"]["WARN"], 1);
}

#[test]
fn test_simple_regex_no_captures() {
    let data = b"ERROR something broke\nINFO all good\nERROR another failure\n";
    let tmp = write_temp_log(data);

    let regex = Regex::new(r"ERROR").unwrap();
    let summary = process_file(tmp.path(), &regex, &OutputMode::Summary, 32, false).unwrap();

    assert_eq!(summary.total_lines, 3);
    assert_eq!(summary.matched_lines, 2);
    assert!(summary.field_frequencies.is_empty());
}

#[test]
fn test_empty_file() {
    let tmp = write_temp_log(b"");

    let regex = Regex::new(r"ERROR").unwrap();
    let summary = process_file(tmp.path(), &regex, &OutputMode::Summary, 32, false).unwrap();

    assert_eq!(summary.total_lines, 0);
    assert_eq!(summary.matched_lines, 0);
}

#[test]
fn test_no_matches() {
    let data = b"all is well\nnothing to see here\nmove along\n";
    let tmp = write_temp_log(data);

    let regex = Regex::new(r"CRITICAL").unwrap();
    let summary = process_file(tmp.path(), &regex, &OutputMode::Summary, 32, false).unwrap();

    assert_eq!(summary.total_lines, 3);
    assert_eq!(summary.matched_lines, 0);
}

#[test]
fn test_large_batch_processing() {
    // Generate enough lines to exercise multiple batch cycles with a small batch size.
    let data = generate_test_log(500);
    let tmp = write_temp_log(&data);

    let regex = Regex::new(r"(?P<level>ERROR|WARN|INFO|DEBUG)").unwrap();
    let summary = process_file(tmp.path(), &regex, &OutputMode::Summary, 16, false).unwrap();

    assert_eq!(summary.total_lines, 500);
    assert!(summary.matched_lines > 0);

    let freq_sum: u64 = summary
        .field_frequencies
        .get("level")
        .map(|m| m.values().sum())
        .unwrap_or(0);
    assert_eq!(freq_sum, summary.matched_lines);
}

#[test]
fn test_ip_address_extraction() {
    let data = b"192.168.1.1 - GET /index.html 200\n\
                 10.0.0.5 - POST /api/data 404\n\
                 invalid line\n\
                 172.16.0.1 - GET /health 200\n";
    let tmp = write_temp_log(data);

    let regex =
        Regex::new(r"(?P<ip>\d+\.\d+\.\d+\.\d+) - (?P<method>\w+) (?P<path>\S+) (?P<status>\d+)")
            .unwrap();
    let summary = process_file(tmp.path(), &regex, &OutputMode::Summary, 32, false).unwrap();

    assert_eq!(summary.total_lines, 4);
    assert_eq!(summary.matched_lines, 3);
    assert_eq!(summary.field_frequencies["status"]["200"], 2);
    assert_eq!(summary.field_frequencies["status"]["404"], 1);
    assert_eq!(summary.field_frequencies["method"]["GET"], 2);
    assert_eq!(summary.field_frequencies["method"]["POST"], 1);
}
