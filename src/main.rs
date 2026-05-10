mod analyzer;
mod classifier;
mod delete;
mod dupes;
mod renderer;
mod walker;

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use walker::WalkOptions;

fn help_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .usage(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .literal(AnsiColor::Green.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Magenta.on_default())
        .valid(AnsiColor::Green.on_default())
        .invalid(AnsiColor::Red.on_default() | Effects::BOLD)
        .error(AnsiColor::Red.on_default() | Effects::BOLD)
}

#[derive(Parser)]
#[command(
    name = "bigfiles",
    version,
    about = "Find what's eating your disk",
    styles = help_styles()
)]
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

    /// Do not respect .gitignore / .ignore files
    #[arg(long, global = true)]
    no_ignore: bool,

    /// Do not auto-page output through $PAGER (default: less -FRX)
    #[arg(long, global = true)]
    no_pager: bool,

    /// Show the N largest individual files per category (default scan only)
    #[arg(short, long)]
    top: Option<usize>,

    /// Output raw JSON (default scan only)
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

fn walk_opts(cli: &Cli) -> WalkOptions {
    WalkOptions {
        skip_hidden: cli.skip_hidden,
        max_depth: cli.depth,
        respect_ignore: !cli.no_ignore,
    }
}

fn setup_pager(cli: &Cli) {
    if cli.no_pager {
        return;
    }
    pager::Pager::with_default_pager("less -FRX").setup();
}

fn run_scan(cli: &Cli) -> ExitCode {
    let scan = walker::collect(&cli.path, walk_opts(cli));
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
        setup_pager(cli);
        renderer::render(&summaries, total, scan.skipped, &cli.path);
        if let Some(n) = cli.top {
            renderer::render_top(&scan.files, n);
        }
    }
    ExitCode::SUCCESS
}

fn run_dupes(cli: &Cli, min_size: u64) -> ExitCode {
    let scan = walker::collect(&cli.path, walk_opts(cli));
    let groups = dupes::find(&scan.files, min_size);
    setup_pager(cli);
    dupes::render(&groups, &cli.path);
    ExitCode::SUCCESS
}

fn run_delete(cli: &Cli) -> ExitCode {
    let scan = walker::collect(&cli.path, walk_opts(cli));
    match delete::run(&scan.files, cli.stale_years) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bigfiles: delete failed: {}", e);
            ExitCode::from(1)
        }
    }
}
