mod walker;
mod classifier;
mod analyzer;
mod renderer;

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "bigfiles", about = "Find what's eating your disk")]
struct Args {
    /// Directory to scan (default: current dir)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Flag files not modified in this many years as stale
    #[arg(short, long, default_value_t = 2)]
    stale_years: u64,

    /// Skip hidden files and dirs
    #[arg(short = 'H', long)]
    skip_hidden: bool,

    /// Output raw JSON
    #[arg(short, long)]
    json: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    if !args.path.exists() {
        eprintln!("bigfiles: path does not exist: {}", args.path.display());
        return ExitCode::from(2);
    }

    let scan = walker::collect(&args.path, args.skip_hidden);
    let total: u64 = scan.files.iter().map(|f| f.size).sum();
    let summaries = analyzer::analyze(&scan.files, args.stale_years);

    if args.json {
        match serde_json::to_string_pretty(&summaries) {
            Ok(s) => println!("{}", s),
            Err(e) => {
                eprintln!("bigfiles: failed to serialize JSON: {}", e);
                return ExitCode::from(1);
            }
        }
    } else {
        renderer::render(&summaries, total, scan.skipped, &args.path);
    }
    ExitCode::SUCCESS
}
