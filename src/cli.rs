use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Slicer — A lightning-fast CLI log parser and analyzer.
///
/// Streams gigabytes of unstructured log files, filters them with regex,
/// and outputs structured JSON or statistical summaries without exhausting
/// system memory.
#[derive(Parser, Debug)]
#[command(
    name = "slicer",
    version,
    about = "⚡ Lightning-fast log parser and analyzer",
    long_about = "Slicer streams gigabytes of log files through a compiled regex engine,\n\
                  extracts named capture groups into structured JSON, and produces\n\
                  rich statistical summaries — all with under 50MB of RAM."
)]
pub struct Cli {
    /// Path to the log file to parse. Use '-' to read from stdin.
    pub file: PathBuf,

    /// Regular expression pattern to filter/parse log lines.
    ///
    /// Supports named capture groups using (?P<name>...) syntax.
    /// Named groups are extracted into JSON fields and tracked in summaries.
    #[arg(short, long)]
    pub pattern: String,

    /// Output mode: 'json' emits one JSON object per matched line,
    /// 'summary' prints an aggregate statistical report.
    #[arg(short = 'm', long, default_value = "summary", value_enum)]
    pub mode: OutputMode,

    /// Number of lines to collect before dispatching to the rayon thread pool.
    /// Larger batches amortize parallelism overhead; smaller batches reduce
    /// peak memory. The default of 8192 balances both concerns.
    #[arg(long, default_value = "8192")]
    pub batch_size: usize,

    /// Disable the progress bar (useful when piping output).
    #[arg(long)]
    pub no_progress: bool,
}

/// Output mode selector.
#[derive(Debug, Clone, ValueEnum)]
pub enum OutputMode {
    /// Emit one JSON object per matched line (NDJSON / JSON Lines).
    Json,
    /// Print an aggregate statistical summary after processing.
    Summary,
}
