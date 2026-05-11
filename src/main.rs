mod delete;

use bigfiles::walker::{ScanResult, WalkOptions};
use bigfiles::{analyzer, dupes, renderer, walker, INTERRUPTED};
use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const EXIT_SUCCESS: u8 = 0;
const EXIT_RUNTIME_ERROR: u8 = 1;
const EXIT_USAGE_ERROR: u8 = 2;

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

    /// Exclude files/dirs matching this glob (repeatable). Example: --exclude 'node_modules' --exclude '*.log'
    #[arg(short = 'e', long = "exclude", global = true, value_name = "GLOB")]
    excludes: Vec<String>,

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

        /// Interactively delete duplicate copies (keep one per group)
        #[arg(long)]
        delete: bool,
    },
    /// Interactively delete stale files
    Delete,
}

fn main() -> ExitCode {
    let interrupt_flag = Arc::new(AtomicBool::new(false));
    INTERRUPTED.set(interrupt_flag.clone()).ok();
    {
        let flag = interrupt_flag.clone();
        let _ = ctrlc::set_handler(move || {
            flag.store(true, Ordering::SeqCst);
        });
    }

    let cli = Cli::parse();

    if !cli.path.exists() {
        eprintln!("bigfiles: path does not exist: {}", cli.path.display());
        return ExitCode::from(EXIT_USAGE_ERROR);
    }

    if cli.command.is_some() && (cli.top.is_some() || cli.json) {
        eprintln!(
            "bigfiles: --top and --json only apply to the default scan; ignoring for this subcommand"
        );
    }

    match &cli.command {
        None => run_scan(&cli),
        Some(Command::Dupes { min_size, delete }) => run_dupes(&cli, *min_size, *delete),
        Some(Command::Delete) => run_delete(&cli),
    }
}

fn scan_with_progress(cli: &Cli, show_progress: bool) -> ScanResult {
    if !show_progress || !std::io::stderr().is_terminal() {
        return walker::collect(&cli.path, walk_opts(cli));
    }
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} scanning {pos} files...")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.set_draw_target(indicatif::ProgressDrawTarget::stderr());
    pb.enable_steady_tick(Duration::from_millis(100));
    let result = walker::collect_with_progress(&cli.path, walk_opts(cli), {
        let pb = pb.clone();
        move |n| pb.set_position(n as u64)
    });
    pb.finish_and_clear();
    result
}

fn walk_opts(cli: &Cli) -> WalkOptions {
    WalkOptions {
        skip_hidden: cli.skip_hidden,
        max_depth: cli.depth,
        respect_ignore: !cli.no_ignore,
        exclude_globs: cli.excludes.clone(),
    }
}

#[cfg(unix)]
fn setup_pager(cli: &Cli) {
    if cli.no_pager {
        return;
    }
    pager::Pager::with_default_pager("less -FRX").setup();
}

#[cfg(not(unix))]
fn setup_pager(_cli: &Cli) {}

fn run_scan(cli: &Cli) -> ExitCode {
    let scan = scan_with_progress(cli, !cli.json);
    let total: u64 = scan.files.iter().map(|f| f.size).sum();
    let summaries = analyzer::analyze(&scan.files, cli.stale_years);

    if cli.json {
        let envelope = serde_json::json!({
            "version": 1,
            "root": cli.path.display().to_string(),
            "total_size": total,
            "skipped": scan.skipped,
            "categories": summaries,
        });
        match serde_json::to_string_pretty(&envelope) {
            Ok(s) => println!("{}", s),
            Err(e) => {
                eprintln!("bigfiles: failed to serialize JSON: {}", e);
                return ExitCode::from(EXIT_RUNTIME_ERROR);
            }
        }
    } else {
        setup_pager(cli);
        renderer::render(&summaries, total, scan.skipped, &cli.path);
        if let Some(n) = cli.top {
            renderer::render_top(&scan.files, n);
        }
    }
    ExitCode::from(EXIT_SUCCESS)
}

fn run_dupes(cli: &Cli, min_size: u64, delete: bool) -> ExitCode {
    let scan = scan_with_progress(cli, true);
    let groups = dupes::find(&scan.files, min_size);
    if delete {
        match dupes::delete_interactive(&groups, &cli.path) {
            Ok(()) => ExitCode::from(EXIT_SUCCESS),
            Err(e) => {
                eprintln!("bigfiles: dupes delete failed: {}", e);
                ExitCode::from(EXIT_RUNTIME_ERROR)
            }
        }
    } else {
        setup_pager(cli);
        dupes::render(&groups, &cli.path);
        ExitCode::from(EXIT_SUCCESS)
    }
}

fn run_delete(cli: &Cli) -> ExitCode {
    let scan = scan_with_progress(cli, true);
    match delete::run(&scan.files, cli.stale_years) {
        Ok(()) => ExitCode::from(EXIT_SUCCESS),
        Err(e) => {
            eprintln!("bigfiles: delete failed: {}", e);
            ExitCode::from(EXIT_RUNTIME_ERROR)
        }
    }
}
