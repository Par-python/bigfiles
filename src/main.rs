mod walker;
mod classifier;
mod analyzer;
mod renderer;
mod dupes;
mod delete;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "bigfiles", version, about = "Find what's eating your disk")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Directory to scan (default: current dir)
    #[arg(default_value = ".", global = true)]
    path: PathBuf,

    /// Flag files not modified in this many years as stale
    #[arg(short, long, default_value_t = 2, global = true)]
    stale_years: u64,

    /// Skip hidden files and dirs
    #[arg(short = 'H', long, global = true)]
    skip_hidden: bool,

    /// Limit traversal depth (1 = only files in root)
    #[arg(short, long, global = true)]
    depth: Option<usize>,

    /// Output raw JSON (only for default scan)
    #[arg(short, long)]
    json: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Find duplicate files by content hash
    Dupes {
        /// Minimum file size to consider (bytes); ignore tiny files
        #[arg(long, default_value_t = 1024)]
        min_size: u64,
    },
    /// Interactively delete stale files
    Delete,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if !cli.path.exists() {
        eprintln!("bigfiles: path does not exist: {}", cli.path.display());
        return ExitCode::from(2);
    }

    match &cli.command {
        None => run_scan(&cli),
        Some(Command::Dupes { min_size }) => run_dupes(&cli, *min_size),
        Some(Command::Delete) => run_delete(&cli),
    }
}

fn run_scan(cli: &Cli) -> ExitCode {
    let scan = walker::collect(&cli.path, cli.skip_hidden, cli.depth);
    let total: u64 = scan.files.iter().map(|f| f.size).sum();
    let summaries = analyzer::analyze(&scan.files, cli.stale_years);

    if cli.json {
        match serde_json::to_string_pretty(&summaries) {
            Ok(s) => println!("{}", s),
            Err(e) => {
                eprintln!("bigfiles: failed to serialize JSON: {}", e);
                return ExitCode::from(1);
            }
        }
    } else {
        renderer::render(&summaries, total, scan.skipped, &cli.path);
    }
    ExitCode::SUCCESS
}

fn run_dupes(cli: &Cli, min_size: u64) -> ExitCode {
    let scan = walker::collect(&cli.path, cli.skip_hidden, cli.depth);
    let groups = dupes::find(&scan.files, min_size);
    dupes::render(&groups, &cli.path);
    ExitCode::SUCCESS
}

fn run_delete(cli: &Cli) -> ExitCode {
    let scan = walker::collect(&cli.path, cli.skip_hidden, cli.depth);
    match delete::run(&scan.files, cli.stale_years) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bigfiles: delete failed: {}", e);
            ExitCode::from(1)
        }
    }
}
