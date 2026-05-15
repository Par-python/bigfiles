mod delete;

use bigfiles::analyzer::SortKey;
use bigfiles::format::Units;
use bigfiles::walker::{ScanResult, WalkOptions};
use bigfiles::{analyzer, dupes, format, renderer, walker, INTERRUPTED};
use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand, ValueEnum};
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

    /// Byte unit style: default (1024-based, KB/MB), iec (1024, KiB/MiB), si (1000, KB/MB)
    #[arg(long, global = true, value_name = "STYLE", default_value = "default")]
    units: String,

    /// Color output: auto (default), always, never. Respects NO_COLOR env var.
    #[arg(long, global = true, value_name = "WHEN", default_value = "auto")]
    color: String,

    /// Show the N largest individual files per category (default scan only)
    #[arg(short, long)]
    top: Option<usize>,

    /// Output raw JSON (default scan only)
    #[arg(short, long)]
    json: bool,

    /// Sort categories by: size, count, stale-size, stale-count, name (default scan only)
    #[arg(long, value_enum, default_value_t = SortKeyArg::Size)]
    sort: SortKeyArg,

    /// Reverse the sort order (default scan only)
    #[arg(long)]
    reverse: bool,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
#[clap(rename_all = "kebab-case")]
enum SortKeyArg {
    Size,
    Count,
    StaleSize,
    StaleCount,
    Name,
}

impl From<SortKeyArg> for SortKey {
    fn from(v: SortKeyArg) -> Self {
        match v {
            SortKeyArg::Size => SortKey::Size,
            SortKeyArg::Count => SortKey::Count,
            SortKeyArg::StaleSize => SortKey::StaleSize,
            SortKeyArg::StaleCount => SortKey::StaleCount,
            SortKeyArg::Name => SortKey::Name,
        }
    }
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

        /// Skip the persistent hash cache (read and write disabled for this run)
        #[arg(long)]
        no_cache: bool,

        /// Delete the persistent hash cache before running
        #[arg(long)]
        clear_cache: bool,
    },
    /// Interactively delete stale files
    Delete,
    /// Interactive directory browser (ncdu-style)
    Tui,
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

    let units = match cli.units.as_str() {
        "iec" => Units::Iec,
        "si" => Units::Si,
        "default" => Units::Default,
        other => {
            eprintln!(
                "bigfiles: invalid --units {:?} (expected: default, iec, si)",
                other
            );
            return ExitCode::from(EXIT_USAGE_ERROR);
        }
    };
    format::set_units(units);

    apply_color_choice(&cli.color);

    if !cli.path.exists() {
        eprintln!("bigfiles: path does not exist: {}", cli.path.display());
        return ExitCode::from(EXIT_USAGE_ERROR);
    }

    let non_default_sort = !matches!(cli.sort, SortKeyArg::Size) || cli.reverse;
    if cli.command.is_some() && (cli.top.is_some() || cli.json || non_default_sort) {
        eprintln!(
            "bigfiles: --top, --json, --sort, --reverse only apply to the default scan; ignoring for this subcommand"
        );
    }

    match &cli.command {
        None => run_scan(&cli),
        Some(Command::Dupes {
            min_size,
            delete,
            no_cache,
            clear_cache,
        }) => run_dupes(&cli, *min_size, *delete, *no_cache, *clear_cache),
        Some(Command::Delete) => run_delete(&cli),
        Some(Command::Tui) => run_tui(&cli),
    }
}

fn scan_with_progress(cli: &Cli, show_progress: bool) -> ScanResult {
    if !show_progress || !std::io::stderr().is_terminal() {
        return walker::collect(&cli.path, walk_opts(cli));
    }
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.set_draw_target(indicatif::ProgressDrawTarget::stderr());
    pb.enable_steady_tick(Duration::from_millis(100));
    let result = walker::collect_with_progress(&cli.path, walk_opts(cli), {
        let pb = pb.clone();
        move |tick| {
            let dir = tick.current_dir.display().to_string();
            let trimmed = if dir.len() > 60 {
                let start = dir.len() - 57;
                format!("…{}", &dir[start..])
            } else {
                dir
            };
            pb.set_message(format!(
                "scanned {} files, {} — {}",
                bigfiles::format::count(tick.file_count),
                bigfiles::format::bytes(tick.total_bytes),
                trimmed
            ));
        }
    });
    pb.finish_and_clear();
    result
}

fn apply_color_choice(when: &str) {
    use owo_colors::set_override;
    if std::env::var_os("NO_COLOR").is_some() {
        set_override(false);
        return;
    }
    match when {
        "always" => set_override(true),
        "never" => set_override(false),
        _ => {}
    }
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
    let mut summaries = analyzer::analyze(&scan.files, cli.stale_years);
    analyzer::sort_summaries(&mut summaries, cli.sort.into(), cli.reverse);

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

fn run_dupes(
    cli: &Cli,
    min_size: u64,
    delete: bool,
    no_cache: bool,
    clear_cache: bool,
) -> ExitCode {
    if clear_cache {
        if let Err(e) = bigfiles::cache::clear() {
            eprintln!("bigfiles: failed to clear hash cache: {}", e);
        }
    }
    let scan = scan_with_progress(cli, true);
    let cache = if no_cache {
        bigfiles::cache::HashCache::empty()
    } else {
        bigfiles::cache::HashCache::load()
    };
    let groups = dupes::find_with_cache(&scan.files, min_size, &cache);
    if !no_cache {
        if let Err(e) = cache.save() {
            eprintln!("bigfiles: failed to save hash cache: {}", e);
        }
    }
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

fn run_tui(cli: &Cli) -> ExitCode {
    let scan = scan_with_progress(cli, true);
    match bigfiles::tui::run(&cli.path, &scan.files) {
        Ok(()) => ExitCode::from(EXIT_SUCCESS),
        Err(e) => {
            eprintln!("bigfiles: tui failed: {}", e);
            ExitCode::from(EXIT_RUNTIME_ERROR)
        }
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
