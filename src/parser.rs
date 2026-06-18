//! Core streaming log processor.
//!
//! The parser reads a file (or stdin) line by line using a buffered reader,
//! collects lines into batches, and dispatches each batch to the rayon thread
//! pool for parallel regex matching.  This design keeps memory bounded by
//! `batch_size × average_line_length` while saturating all available cores.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use rayon::prelude::*;
use regex::Regex;

use crate::cli::OutputMode;
use crate::models::{ParsedLine, Summary};

/// Size of the internal `BufReader` buffer.
///
/// 256 KiB is large enough to amortize syscall overhead on modern kernels
/// without meaningfully inflating RSS.
const BUF_CAPACITY: usize = 256 * 1024;

/// Process a log file (or stdin when `path` is `"-"`).
///
/// Returns a [`Summary`] containing aggregate statistics.  In `Json` mode,
/// matched lines are written to stdout as NDJSON during processing so that
/// downstream consumers can begin parsing immediately.
pub fn process_file(
    path: &Path,
    regex: &Regex,
    mode: &OutputMode,
    batch_size: usize,
    show_progress: bool,
) -> Result<Summary> {
    let (reader, file_size): (Box<dyn Read>, Option<u64>) =
        if path.as_os_str() == "-" {
            (Box::new(io::stdin().lock()), None)
        } else {
            let file = File::open(path)
                .with_context(|| format!("Failed to open file: {}", path.display()))?;
            let size = file.metadata().map(|m| m.len()).ok();
            (Box::new(file), size)
        };

    let buf_reader = BufReader::with_capacity(BUF_CAPACITY, reader);
    let pb = create_progress_bar(file_size, show_progress);

    // Pre-collect named capture group names so we don't re-discover them per line.
    let capture_names: Vec<String> = regex
        .capture_names()
        .flatten()
        .map(String::from)
        .collect();

    let start = Instant::now();
    let mut summary = Summary::new();

    // Reusable batch buffer — `clear()` preserves the heap allocation.
    let mut batch: Vec<(u64, String)> = Vec::with_capacity(batch_size);
    let mut line_number: u64 = 0;

    // Buffer stdout to avoid per-line write syscalls in JSON mode.
    let stdout_handle = io::stdout();
    let mut stdout = BufWriter::with_capacity(BUF_CAPACITY, stdout_handle.lock());

    // Raw byte buffer reused across iterations to avoid per-line allocation
    // when the line is valid UTF-8 (the common case).
    let mut raw_buf: Vec<u8> = Vec::with_capacity(1024);

    for line_result in LineIterator::new(buf_reader, &mut raw_buf) {
        let (line, bytes_read) = line_result.context("Failed to read line from input")?;
        line_number += 1;
        summary.bytes_processed += bytes_read;
        pb.inc(bytes_read);

        batch.push((line_number, line));

        if batch.len() >= batch_size {
            summary.total_lines += batch.len() as u64;
            dispatch_batch(&batch, regex, &capture_names, mode, &mut summary, &mut stdout)?;
            batch.clear();
        }
    }

    // Flush the trailing partial batch.
    if !batch.is_empty() {
        summary.total_lines += batch.len() as u64;
        dispatch_batch(&batch, regex, &capture_names, mode, &mut summary, &mut stdout)?;
    }

    stdout.flush()?;
    pb.finish_and_clear();
    summary.elapsed = start.elapsed();

    Ok(summary)
}

// ---------------------------------------------------------------------------
// Line iterator — reads raw bytes and converts to String with lossy UTF-8
// ---------------------------------------------------------------------------

/// A zero-allocation-per-iteration line reader that reuses a byte buffer.
///
/// Unlike `BufRead::lines()`, this handles non-UTF-8 bytes gracefully
/// via lossy conversion, making it safe for corrupted or binary-interleaved
/// log files.
struct LineIterator<'a, R> {
    reader: R,
    buf: &'a mut Vec<u8>,
}

impl<'a, R: BufRead> LineIterator<'a, R> {
    fn new(reader: R, buf: &'a mut Vec<u8>) -> Self {
        Self { reader, buf }
    }
}

impl<R: BufRead> Iterator for LineIterator<'_, R> {
    type Item = Result<(String, u64), io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.buf.clear();
        match self.reader.read_until(b'\n', self.buf) {
            Ok(0) => None,
            Ok(n) => {
                // Strip trailing newline characters.
                if self.buf.last() == Some(&b'\n') {
                    self.buf.pop();
                    if self.buf.last() == Some(&b'\r') {
                        self.buf.pop();
                    }
                }
                let line = String::from_utf8_lossy(self.buf).into_owned();
                Some(Ok((line, n as u64)))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

// ---------------------------------------------------------------------------
// Batch dispatch
// ---------------------------------------------------------------------------

/// Process a batch of lines and either write JSON to stdout or accumulate
/// frequency data into `summary`.
fn dispatch_batch(
    batch: &[(u64, String)],
    regex: &Regex,
    capture_names: &[String],
    mode: &OutputMode,
    summary: &mut Summary,
    stdout: &mut impl Write,
) -> Result<()> {
    match mode {
        OutputMode::Json => {
            let matched: Vec<ParsedLine> = process_batch_json(batch, regex, capture_names);
            summary.matched_lines += matched.len() as u64;

            // Accumulate frequencies even in JSON mode for completeness.
            for line in &matched {
                for (name, value) in &line.captures {
                    *summary
                        .field_frequencies
                        .entry(name.clone())
                        .or_default()
                        .entry(value.clone())
                        .or_insert(0) += 1;
                }
            }

            for line in &matched {
                serde_json::to_writer(&mut *stdout, line)?;
                stdout.write_all(b"\n")?;
            }
        }
        OutputMode::Summary => {
            let (matched, freqs) = process_batch_summary(batch, regex, capture_names);
            summary.merge_batch(matched, freqs);
        }
    }
    Ok(())
}

/// Parallel batch processor for **JSON mode** — returns full `ParsedLine` objects.
fn process_batch_json(
    batch: &[(u64, String)],
    regex: &Regex,
    capture_names: &[String],
) -> Vec<ParsedLine> {
    batch
        .par_iter()
        .filter_map(|(line_num, line)| {
            regex.captures(line).map(|caps| {
                let mut captures = HashMap::new();
                for name in capture_names {
                    if let Some(m) = caps.name(name) {
                        captures.insert(name.clone(), m.as_str().to_string());
                    }
                }
                ParsedLine {
                    line_number: *line_num,
                    raw: line.clone(),
                    captures,
                }
            })
        })
        .collect()
}

/// Parallel batch processor for **summary mode** — uses rayon `fold`/`reduce`
/// to aggregate frequencies without allocating `ParsedLine` objects.
///
/// This is significantly cheaper than the JSON path because it avoids cloning
/// every matched line into a struct.
fn process_batch_summary(
    batch: &[(u64, String)],
    regex: &Regex,
    capture_names: &[String],
) -> (u64, HashMap<String, HashMap<String, u64>>) {
    batch
        .par_iter()
        .fold(
            || (0u64, HashMap::<String, HashMap<String, u64>>::new()),
            |(mut count, mut freqs), (_line_num, line)| {
                if let Some(caps) = regex.captures(line) {
                    count += 1;
                    for name in capture_names {
                        if let Some(m) = caps.name(name) {
                            *freqs
                                .entry(name.clone())
                                .or_default()
                                .entry(m.as_str().to_string())
                                .or_insert(0) += 1;
                        }
                    }
                }
                (count, freqs)
            },
        )
        .reduce(
            || (0, HashMap::new()),
            |(c1, mut f1), (c2, f2)| {
                for (field, values) in f2 {
                    let entry = f1.entry(field).or_default();
                    for (value, cnt) in values {
                        *entry.entry(value).or_insert(0) += cnt;
                    }
                }
                (c1 + c2, f1)
            },
        )
}

// ---------------------------------------------------------------------------
// Progress bar
// ---------------------------------------------------------------------------

/// Create an `indicatif` progress bar that renders to stderr.
///
/// If the file size is known, a byte-level progress bar is shown.
/// Otherwise, a spinner indicating lines processed is used.
fn create_progress_bar(file_size: Option<u64>, show: bool) -> ProgressBar {
    if !show {
        return ProgressBar::hidden();
    }

    match file_size {
        Some(size) => {
            let pb = ProgressBar::new(size);
            pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] \
                     {bytes}/{total_bytes} ({bytes_per_sec}) {msg}",
                )
                .unwrap()
                .progress_chars("█▓▒░  "),
            );
            pb
        }
        None => {
            let pb = ProgressBar::new_spinner();
            pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{elapsed_precise}] {bytes} read ({bytes_per_sec}) {msg}",
                )
                .unwrap(),
            );
            pb
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_regex() -> Regex {
        Regex::new(r"(?P<timestamp>\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}) (?P<level>\w+) (?P<msg>.+)")
            .unwrap()
    }

    #[test]
    fn test_process_batch_json_matches() {
        let regex = test_regex();
        let names: Vec<String> = regex.capture_names().flatten().map(String::from).collect();
        let batch = vec![
            (1, "2024-01-15T10:30:00 ERROR disk full".to_string()),
            (2, "this line does not match".to_string()),
            (3, "2024-01-15T10:31:00 WARN low memory".to_string()),
        ];

        let results = process_batch_json(&batch, &regex, &names);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].captures["level"], "ERROR");
        assert_eq!(results[1].captures["level"], "WARN");
    }

    #[test]
    fn test_process_batch_summary_counts() {
        let regex = test_regex();
        let names: Vec<String> = regex.capture_names().flatten().map(String::from).collect();
        let batch = vec![
            (1, "2024-01-15T10:30:00 ERROR disk full".to_string()),
            (2, "noise".to_string()),
            (3, "2024-01-15T10:31:00 ERROR oom".to_string()),
            (4, "2024-01-15T10:32:00 INFO started".to_string()),
        ];

        let (matched, freqs) = process_batch_summary(&batch, &regex, &names);
        assert_eq!(matched, 3);
        assert_eq!(freqs["level"]["ERROR"], 2);
        assert_eq!(freqs["level"]["INFO"], 1);
    }

    #[test]
    fn test_process_batch_no_named_groups() {
        let regex = Regex::new(r"ERROR").unwrap();
        let names: Vec<String> = regex.capture_names().flatten().map(String::from).collect();
        let batch = vec![
            (1, "ERROR something".to_string()),
            (2, "INFO ok".to_string()),
        ];

        let results = process_batch_json(&batch, &regex, &names);
        assert_eq!(results.len(), 1);
        assert!(results[0].captures.is_empty());
    }

    #[test]
    fn test_process_batch_empty() {
        let regex = Regex::new(r"ERROR").unwrap();
        let names: Vec<String> = regex.capture_names().flatten().map(String::from).collect();
        let batch: Vec<(u64, String)> = vec![];

        let (matched, freqs) = process_batch_summary(&batch, &regex, &names);
        assert_eq!(matched, 0);
        assert!(freqs.is_empty());
    }

    #[test]
    fn test_line_iterator_basic() {
        let data = b"line one\nline two\nline three\n";
        let reader = BufReader::new(&data[..]);
        let mut buf = Vec::new();
        let lines: Vec<(String, u64)> = LineIterator::new(reader, &mut buf)
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].0, "line one");
        assert_eq!(lines[1].0, "line two");
        assert_eq!(lines[2].0, "line three");
    }

    #[test]
    fn test_line_iterator_crlf() {
        let data = b"hello\r\nworld\r\n";
        let reader = BufReader::new(&data[..]);
        let mut buf = Vec::new();
        let lines: Vec<(String, u64)> = LineIterator::new(reader, &mut buf)
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "hello");
        assert_eq!(lines[1].0, "world");
    }

    #[test]
    fn test_line_iterator_no_trailing_newline() {
        let data = b"no newline at end";
        let reader = BufReader::new(&data[..]);
        let mut buf = Vec::new();
        let lines: Vec<(String, u64)> = LineIterator::new(reader, &mut buf)
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, "no newline at end");
    }
}
