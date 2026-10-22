use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod sample;
mod classify;
mod report;
mod bench;

/// browser-process-budget (bpb) — measure Chromium process overhead on Windows.
#[derive(Parser, Debug)]
#[command(name = "bpb", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Snapshot Chrome/Edge processes and working set (read-only).
    Sample {
        /// Browser family to match: chrome | edge | auto
        #[arg(long, default_value = "chrome")]
        browser: String,
        /// Output path (JSON). Default: stdout
        #[arg(long)]
        out: Option<PathBuf>,
        /// Include session id when available (RDS).
        #[arg(long, default_value_t = true)]
        session: bool,
    },
    /// Classify a previous sample JSON into Browser/Renderer/GPU/Utility/Extension.
    Classify {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Render Markdown + JSON summary from classified samples.
    Report {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        out_dir: PathBuf,
        /// Redact hostnames and usernames in the report.
        #[arg(long, default_value_t = true)]
        redact: bool,
    },
    /// Run a named office workload script then sample (requires scripts/).
    Bench {
        /// Scenario id, e.g. office-basic
        #[arg(long)]
        scenario: String,
        #[arg(long, default_value = "chrome")]
        browser: String,
        #[arg(long)]
        out_dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Sample { browser, out, session } => {
            sample::run(&browser, out.as_deref(), session)
        }
        Commands::Classify { input, out } => classify::run(&input, out.as_deref()),
        Commands::Report {
            input,
            out_dir,
            redact,
        } => report::run(&input, &out_dir, redact),
        Commands::Bench {
            scenario,
            browser,
            out_dir,
        } => bench::run(&scenario, &browser, &out_dir),
    }
}
