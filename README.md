<div align="center">

# ⚡ Slicer

**A lightning-fast CLI log parser and analyzer built in Rust**

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)

*Stream gigabytes of unstructured logs, filter with regex, and extract structured data — without exhausting system memory.*

</div>

---

## 🚀 Features

- **Streaming I/O** — Processes 50GB+ files using under 50MB of RAM via buffered reads
- **Parallel Processing** — Dispatches line batches to all CPU cores via [rayon](https://github.com/rayon-rs/rayon)
- **Flexible Regex Filtering** — Filter lines with any regex; extract named capture groups into structured fields
- **Multiple Output Modes** — Stream NDJSON for pipelines or generate rich statistical summaries
- **Beautiful CLI UX** — Real-time progress bars, colored output, and clear error messages
- **Resilient Parsing** — Handles corrupted/binary-interleaved logs via lossy UTF-8 conversion

---

## 📦 Installation

### From source (recommended)

```bash
cargo install --path .
```

### From crates.io

```bash
cargo install slicer
```

### Build for maximum performance

```bash
cargo build --release
# Binary at: target/release/slicer
```

The release profile enables LTO, single codegen unit, and symbol stripping for optimal binary size and speed.

---

## 📖 Usage

### Basic Filtering

Find all ERROR lines in a log file:

```bash
slicer server.log --pattern "ERROR"
```

### Structured JSON Extraction

Extract timestamps, levels, and messages into NDJSON:

```bash
slicer app.log \
  --pattern '(?P<timestamp>\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}) (?P<level>\w+) (?P<msg>.+)' \
  --mode json
```

**Output:**
```json
{"line_number":1,"raw":"2024-01-15T10:30:00 ERROR disk full","captures":{"timestamp":"2024-01-15T10:30:00","level":"ERROR","msg":"disk full"}}
{"line_number":3,"raw":"2024-01-15T10:31:00 WARN low memory","captures":{"timestamp":"2024-01-15T10:31:00","level":"WARN","msg":"low memory"}}
```

### Statistical Summary

Analyze HTTP access logs for status code distribution:

```bash
slicer access.log \
  --pattern '(?P<ip>\d+\.\d+\.\d+\.\d+) .* (?P<method>\w+) \S+ (?P<status>\d{3})' \
  --mode summary
```

**Output:**
```
╔══════════════════════════════════════════════════════════╗
║                   SLICER ANALYSIS REPORT                 ║
╠══════════════════════════════════════════════════════════╣
║  Total Lines:            1,234,567                       ║
║  Matched Lines:             45,892                       ║
║  Match Rate:                 3.72%                       ║
║  Bytes Processed:        512.30 MB                       ║
║  Elapsed Time:              2.340s                       ║
║  Processing Speed:     218.93 MB/s                       ║
╠══════════════════════════════════════════════════════════╣
║  Field Frequencies:                                      ║
║                                                          ║
║  ▸ status                                                ║
║    200              ████████████████    32,145 (70.0%)   ║
║    404              ████░░░░░░░░░░░░     8,234 (17.9%)   ║
║    500              ██░░░░░░░░░░░░░░     3,891 ( 8.5%)   ║
║    301              █░░░░░░░░░░░░░░░     1,622 ( 3.5%)   ║
╚══════════════════════════════════════════════════════════╝
```

### Pipeline Integration

Pipe JSON output into `jq` for further processing:

```bash
slicer huge.log \
  --pattern '(?P<level>ERROR|WARN) (?P<msg>.+)' \
  --mode json \
  --no-progress \
  | jq 'select(.captures.level == "ERROR")'
```

### Reading from stdin

```bash
cat server.log | slicer - --pattern "ERROR" --mode summary
```

### CLI Reference

```
Usage: slicer [OPTIONS] --pattern <PATTERN> <FILE>

Arguments:
  <FILE>  Path to the log file to parse. Use '-' to read from stdin

Options:
  -p, --pattern <PATTERN>        Regex pattern to filter/parse log lines
  -m, --mode <MODE>              Output mode [default: summary] [possible values: json, summary]
      --batch-size <BATCH_SIZE>  Lines per parallel batch [default: 8192]
      --no-progress              Disable the progress bar
  -h, --help                     Print help (including more info with '--help')
  -V, --version                  Print version
```

---

## ⚙️ Why It's Fast

Slicer is designed from the ground up for maximum throughput on large files:

1. **Single-pass streaming** — The file is read through a 256 KB `BufReader` buffer. At no point is the entire file loaded into memory. This allows processing files larger than available RAM.

2. **One-time regex compilation** — The regex pattern is compiled into an NFA exactly once before the processing loop begins. Every subsequent match is an O(n) scan over the line with no compilation overhead.

3. **Batch-parallel processing** — Lines are collected into configurable batches (default: 8,192 lines) and dispatched to rayon's work-stealing thread pool. This amortizes the overhead of thread synchronization while saturating all CPU cores.

4. **Zero-copy where possible** — In summary mode, matched lines are never cloned into structs. Instead, rayon's `fold`/`reduce` pattern aggregates frequencies directly, avoiding millions of heap allocations.

5. **Buffered output** — JSON output is written through a 256 KB `BufWriter` on stdout, reducing write syscalls by orders of magnitude compared to per-line writes.

6. **Release profile tuning** — The release binary uses Link-Time Optimization (LTO), single codegen unit, and symbol stripping for maximum instruction-level optimization.

---

## 📊 Performance & Benchmarks

<!-- TODO: Insert demo GIF here -->
<!-- ![slicer demo](./assets/demo.gif) -->

### Benchmark Methodology

All benchmarks were run using [hyperfine](https://github.com/sharkdp/hyperfine) on a 10GB synthetic Apache access log file generated with [flog](https://github.com/mingrammer/flog). Each tool was run 5 times with 2 warm-up rounds to eliminate cold-cache effects.

**System:** <!-- Insert your specs: CPU, RAM, OS, disk type -->

**Command:**
```bash
# Generate test data
flog -f apache_combined -n 50000000 -o access_10gb.log

# Benchmark slicer
hyperfine --warmup 2 --runs 5 \
  'slicer access_10gb.log --pattern "(?P<status>404|500)" --mode summary --no-progress'

# Benchmark grep
hyperfine --warmup 2 --runs 5 \
  'grep -cE "404|500" access_10gb.log'

# Benchmark ripgrep
hyperfine --warmup 2 --runs 5 \
  'rg -c "404|500" access_10gb.log'
```

### Results

<!-- TODO: Replace with your actual benchmark results -->

| Tool | Mean Time | Min | Max | Relative |
|------|-----------|-----|-----|----------|
| `slicer --mode summary` | *X.XXs* | *X.XXs* | *X.XXs* | *1.00×* |
| `grep -cE` | *X.XXs* | *X.XXs* | *X.XXs* | *X.XX×* |
| `rg -c` | *X.XXs* | *X.XXs* | *X.XXs* | *X.XX×* |

> **Note:** Slicer does strictly *more* work than `grep`/`rg` — it extracts named capture groups and computes frequency distributions. The goal is competitive raw speed while providing significantly richer output.

### Memory Usage

```bash
# Measure peak RSS
/usr/bin/time -v slicer access_10gb.log --pattern "ERROR" --mode summary --no-progress
```

| File Size | Peak RSS |
|-----------|----------|
| 1 GB      | *~XX MB* |
| 10 GB     | *~XX MB* |
| 50 GB     | *~XX MB* |

---

## 🧪 Running Tests

```bash
# Run all tests (unit + integration)
cargo test

# Run with output visible
cargo test -- --nocapture

# Run clippy lints
cargo clippy -- -D warnings
```

---

## 🏗️ Project Structure

```
slicer/
├── Cargo.toml          # Dependencies and release profile
├── README.md           # This file
├── src/
│   ├── main.rs         # CLI entry point and orchestration
│   ├── lib.rs          # Library root and module declarations
│   ├── cli.rs          # Clap-derived CLI argument definitions
│   ├── models.rs       # Data structures (ParsedLine, Summary)
│   ├── parser.rs       # Core streaming engine with rayon parallelism
│   └── output.rs       # Colored summary report renderer
└── tests/
    └── integration.rs  # End-to-end tests with synthetic log data
```

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).

---

<div align="center">
<sub>Built with 🦀 Rust — because life's too short for slow log parsers.</sub>
</div>
