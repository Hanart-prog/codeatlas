//! Command-line definitions. Commands not built yet are still declared so that
//! `--help` shows the full interface; running one reports its planned phase.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Explain architecture evolution and estimate change impact from a local Git
/// repository's history.
///
/// CodeAtlas runs entirely on this machine. It reads analyzed repositories but
/// never modifies them, and never executes their code, hooks or scripts.
#[derive(Debug, Parser)]
#[command(name = "codeatlas", version)]
pub struct Cli {
    /// TOML configuration file. Repositories cannot supply their own
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Directory for the local database [default: the OS cache directory]
    #[arg(
        long,
        global = true,
        value_name = "DIR",
        env = "CODEATLAS_DATA_DIR",
        hide_env_values = true
    )]
    pub data_dir: Option<PathBuf>,

    /// Log more to stderr (-v debug, -vv trace); RUST_LOG takes precedence
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

/// Output format for commands that report results.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text
    #[default]
    Text,
    /// Machine-readable JSON on stdout
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Index a repository's first-parent history
    Analyze(AnalyzeArgs),
    /// Summarize the latest analysis of a repository
    Overview(OverviewArgs),
    /// [Phase 3, not implemented] Rank files to inspect when changing a file
    Impact(ImpactArgs),
    /// [Phase 4, not implemented] Diff the architecture of two commits
    Compare(CompareArgs),
    /// [Phase 5, not implemented] Evaluate recommendations on later commits
    Backtest(BacktestArgs),
    /// [Phase 6, not implemented] Write a static HTML report
    Report(ReportArgs),
    /// Start the local HTTP API
    Serve(ServeArgs),
    /// [Phase 6, not implemented] Analyze a generated fixture and open the dashboard
    Demo,
}

#[derive(Debug, Args)]
pub struct AnalyzeArgs {
    /// Repository to analyze
    pub repo: PathBuf,
    /// Git reference to analyze
    #[arg(long = "ref", default_value = "HEAD")]
    pub git_ref: String,
    /// Maximum first-parent commits to index
    #[arg(long, default_value_t = atlas_model::DEFAULT_HISTORY_LIMIT)]
    pub history: u32,
    /// Output format
    #[arg(long, value_enum, default_value_t)]
    pub format: OutputFormat,
}

#[derive(Debug, Args)]
pub struct OverviewArgs {
    /// Analyzed repository
    pub repo: PathBuf,
    /// Output format
    #[arg(long, value_enum, default_value_t)]
    pub format: OutputFormat,
}

#[derive(Debug, Args)]
pub struct ImpactArgs {
    /// Analyzed repository
    pub repo: PathBuf,
    /// Historical cutoff; later commits are ignored
    #[arg(long = "ref", default_value = "HEAD")]
    pub git_ref: String,
    /// Changed file, relative to the repository root (repeatable)
    #[arg(long = "file", required = true)]
    pub files: Vec<String>,
    /// Number of recommendations
    #[arg(long, default_value_t = 10)]
    pub top: u32,
    /// Output format
    #[arg(long, value_enum, default_value_t)]
    pub format: OutputFormat,
}

#[derive(Debug, Args)]
pub struct CompareArgs {
    /// Analyzed repository
    pub repo: PathBuf,
    /// Earlier commit
    #[arg(long)]
    pub base: String,
    /// Later commit
    #[arg(long)]
    pub target: String,
    /// Output format
    #[arg(long, value_enum, default_value_t)]
    pub format: OutputFormat,
}

#[derive(Debug, Args)]
pub struct BacktestArgs {
    /// Analyzed repository
    pub repo: PathBuf,
    /// Eligible commits to skip before evaluating
    #[arg(long, default_value_t = 100)]
    pub warmup: u32,
    /// Maximum evaluation cases
    #[arg(long, default_value_t = 200)]
    pub max_cases: u32,
    /// Cutoffs to evaluate, comma-separated
    #[arg(long, value_delimiter = ',', default_value = "5,10")]
    pub top_k: Vec<u32>,
    /// Output format
    #[arg(long, value_enum, default_value_t)]
    pub format: OutputFormat,
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Analyzed repository
    pub repo: PathBuf,
    /// File to write
    #[arg(long, default_value = "./codeatlas-report.html")]
    pub output: PathBuf,
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Repository to register on startup
    pub repo: Option<PathBuf>,
    /// Address to bind; must be loopback [default: 127.0.0.1]
    #[arg(long)]
    pub host: Option<String>,
    /// Port to bind; 0 picks a free port [default: 4317]
    #[arg(long)]
    pub port: Option<u16>,
}
