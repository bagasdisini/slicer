//! Rich terminal output for summary reports.
//!
//! The summary renderer uses ANSI colors and Unicode box-drawing characters
//! to produce a visually appealing report.  All output goes to **stderr** so
//! it never contaminates piped JSON streams.

use std::io::{self, Write};

use colored::Colorize;

use crate::models::Summary;

/// Format a byte count into a human-readable string (e.g., "1.23 GB").
fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MB", b / MIB)
    } else if b >= KIB {
        format!("{:.2} KB", b / KIB)
    } else {
        format!("{} B", bytes)
    }
}

/// Format a number with thousands separators (e.g., 1_234_567 → "1,234,567").
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

/// Render a horizontal bar of a given ratio (0.0–1.0) within `width` characters.
fn render_bar(ratio: f64, width: usize) -> String {
    let filled = (ratio * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Print the summary report to stderr with colors and structure.
pub fn print_summary(summary: &Summary) -> io::Result<()> {
    let mut out = io::stderr().lock();

    let match_rate = if summary.total_lines > 0 {
        (summary.matched_lines as f64 / summary.total_lines as f64) * 100.0
    } else {
        0.0
    };

    let speed = if summary.elapsed.as_secs_f64() > 0.0 {
        summary.bytes_processed as f64 / summary.elapsed.as_secs_f64()
    } else {
        0.0
    };

    writeln!(out)?;
    writeln!(
        out,
        "{}",
        "╔══════════════════════════════════════════════════════════╗"
            .cyan()
    )?;
    writeln!(
        out,
        "{}",
        "║            ⚡  SLICER ANALYSIS REPORT  ⚡              ║"
            .cyan()
            .bold()
    )?;
    writeln!(
        out,
        "{}",
        "╠══════════════════════════════════════════════════════════╣"
            .cyan()
    )?;

    writeln!(
        out,
        "║  {} {:>20}        ║",
        "Total Lines:".white().bold(),
        format_number(summary.total_lines).green()
    )?;
    writeln!(
        out,
        "║  {} {:>18}        ║",
        "Matched Lines:".white().bold(),
        format_number(summary.matched_lines).yellow()
    )?;
    writeln!(
        out,
        "║  {} {:>21}        ║",
        "Match Rate:".white().bold(),
        format!("{:.2}%", match_rate).magenta()
    )?;
    writeln!(
        out,
        "║  {} {:>16}        ║",
        "Bytes Processed:".white().bold(),
        format_bytes(summary.bytes_processed).blue()
    )?;
    writeln!(
        out,
        "║  {} {:>18}        ║",
        "Elapsed Time:".white().bold(),
        format!("{:.3}s", summary.elapsed.as_secs_f64())
            .cyan()
    )?;
    writeln!(
        out,
        "║  {} {:>14}        ║",
        "Processing Speed:".white().bold(),
        format!("{}/s", format_bytes(speed as u64)).green().bold()
    )?;

    if !summary.field_frequencies.is_empty() {
        writeln!(
            out,
            "{}",
            "╠══════════════════════════════════════════════════════════╣"
                .cyan()
        )?;
        writeln!(
            out,
            "║  {}                                        ║",
            "Field Frequencies:".white().bold()
        )?;

        // Sort fields alphabetically for deterministic output.
        let mut fields: Vec<_> = summary.field_frequencies.iter().collect();
        fields.sort_by_key(|(a, _)| *a);

        for (field, values) in &fields {
            writeln!(
                out,
                "║                                                          ║"
            )?;
            writeln!(
                out,
                "║  {} {}",
                "▸".cyan(),
                field.yellow().bold()
            )?;

            // Sort by count descending, show top 15.
            let mut entries: Vec<_> = values.iter().collect();
            entries.sort_by(|a, b| b.1.cmp(a.1));

            let total: u64 = entries.iter().map(|(_, c)| **c).sum();
            let shown = entries.len().min(15);

            for (value, count) in entries.iter().take(shown) {
                let pct = if total > 0 {
                    (**count as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                let bar = render_bar(**count as f64 / total as f64, 16);
                writeln!(
                    out,
                    "║    {:<16} {} {:>8} ({:.1}%)",
                    value.white(),
                    bar.cyan(),
                    format_number(**count).green(),
                    pct
                )?;
            }

            if entries.len() > 15 {
                writeln!(
                    out,
                    "║    {} {} {}",
                    "...".dimmed(),
                    "and".dimmed(),
                    format!("{} more", entries.len() - 15).dimmed()
                )?;
            }
        }
    }

    writeln!(
        out,
        "{}",
        "╚══════════════════════════════════════════════════════════╝"
            .cyan()
    )?;
    writeln!(out)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1_048_576), "1.00 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.00 GB");
        assert_eq!(format_bytes(2_500_000_000), "2.33 GB");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(1_234_567), "1,234,567");
    }

    #[test]
    fn test_render_bar() {
        let bar = render_bar(0.5, 10);
        assert_eq!(bar.chars().count(), 10);
        assert!(bar.contains('█'));
        assert!(bar.contains('░'));
    }
}
