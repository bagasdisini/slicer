use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use regex::Regex;

use slicer::cli::{Cli, OutputMode};
use slicer::output::print_summary;
use slicer::parser::process_file;

fn main() -> Result<()> {
    let args = Cli::parse();

    // Compile the regex exactly once before entering the hot loop.
    // This is critical: `Regex::new` is expensive (NFA construction),
    // but each subsequent `captures()` call is O(n) in line length.
    let regex = Regex::new(&args.pattern).with_context(|| {
        format!(
            "{} invalid regex pattern: {}",
            "error:".red().bold(),
            args.pattern
        )
    })?;

    // Determine whether to show the progress bar.
    // Disable when --no-progress is set or when stdout is not a terminal
    // (to avoid polluting piped output with ANSI codes on stderr).
    let show_progress = !args.no_progress && atty_stderr();

    let summary = process_file(
        &args.file,
        &regex,
        &args.mode,
        args.batch_size,
        show_progress,
    )?;

    // Always print the summary in summary mode; in JSON mode, the data
    // has already been streamed to stdout.
    if matches!(args.mode, OutputMode::Summary) {
        print_summary(&summary)?;
    }

    Ok(())
}

/// Check if stderr is a terminal (for progress bar rendering decisions).
fn atty_stderr() -> bool {
    // Simple heuristic: try to get terminal size. If it fails, assume
    // we're not in a terminal.  This avoids pulling in the `atty` crate
    // for a single boolean check.
    std::io::IsTerminal::is_terminal(&std::io::stderr())
}
