use crate::format::{bytes as format_bytes, count as format_count};
use crate::walker::FileEntry;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::{execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

struct DirNode {
    path: PathBuf,
    name: String,
    size: u64,
    is_dir: bool,
    children: Vec<usize>,
}

struct Tree {
    nodes: Vec<DirNode>,
    by_path: HashMap<PathBuf, usize>,
    root: usize,
}

impl Tree {
    fn build(root: &Path, files: &[FileEntry]) -> Self {
        let mut nodes: Vec<DirNode> = Vec::new();
        let mut by_path: HashMap<PathBuf, usize> = HashMap::new();

        let root_canon = root.to_path_buf();
        let root_name = root_canon
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| root_canon.display().to_string());
        nodes.push(DirNode {
            path: root_canon.clone(),
            name: root_name,
            size: 0,
            is_dir: true,
            children: Vec::new(),
        });
        by_path.insert(root_canon.clone(), 0);

        for f in files {
            let parent = f.path.parent().unwrap_or(Path::new(""));
            let parent_idx = ensure_dir(&mut nodes, &mut by_path, &root_canon, parent);
            let file_name = f
                .path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let idx = nodes.len();
            nodes.push(DirNode {
                path: f.path.clone(),
                name: file_name,
                size: f.size,
                is_dir: false,
                children: Vec::new(),
            });
            nodes[parent_idx].children.push(idx);
            propagate_size(&mut nodes, &by_path, &root_canon, parent, f.size);
        }

        sort_children_by_size(&mut nodes);

        Tree {
            nodes,
            by_path,
            root: 0,
        }
    }
}

fn sort_children_by_size(nodes: &mut [DirNode]) {
    let n = nodes.len();
    for i in 0..n {
        let mut kids: Vec<usize> = std::mem::take(&mut nodes[i].children);
        kids.sort_by(|&a, &b| nodes[b].size.cmp(&nodes[a].size));
        nodes[i].children = kids;
    }
}

#[allow(clippy::ptr_arg)] // needs Vec::push, can't be &mut [DirNode]
fn ensure_dir(
    nodes: &mut Vec<DirNode>,
    by_path: &mut HashMap<PathBuf, usize>,
    root: &Path,
    dir: &Path,
) -> usize {
    if let Some(&idx) = by_path.get(dir) {
        return idx;
    }
    if dir == root {
        return *by_path.get(root).unwrap();
    }
    let parent = dir.parent().unwrap_or(root);
    let parent_idx = ensure_dir(nodes, by_path, root, parent);
    let name = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let idx = nodes.len();
    nodes.push(DirNode {
        path: dir.to_path_buf(),
        name,
        size: 0,
        is_dir: true,
        children: Vec::new(),
    });
    nodes[parent_idx].children.push(idx);
    by_path.insert(dir.to_path_buf(), idx);
    idx
}

fn propagate_size(
    nodes: &mut [DirNode],
    by_path: &HashMap<PathBuf, usize>,
    root: &Path,
    from: &Path,
    delta: u64,
) {
    let mut cur = from.to_path_buf();
    loop {
        if let Some(&idx) = by_path.get(&cur) {
            nodes[idx].size += delta;
        }
        if cur == root {
            break;
        }
        match cur.parent() {
            Some(p) => cur = p.to_path_buf(),
            None => break,
        }
    }
}

struct AppState {
    tree: Tree,
    current: usize,
    list_state: ListState,
    show_help: bool,
    quit: bool,
}

impl AppState {
    fn new(tree: Tree) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        let current = tree.root;
        Self {
            tree,
            current,
            list_state,
            show_help: false,
            quit: false,
        }
    }

    fn children(&self) -> &[usize] {
        &self.tree.nodes[self.current].children
    }

    fn move_down(&mut self) {
        let n = self.children().len();
        if n == 0 {
            return;
        }
        let next = self
            .list_state
            .selected()
            .map(|i| (i + 1).min(n - 1))
            .unwrap_or(0);
        self.list_state.select(Some(next));
    }

    fn move_up(&mut self) {
        if self.children().is_empty() {
            return;
        }
        let next = self
            .list_state
            .selected()
            .map(|i| i.saturating_sub(1))
            .unwrap_or(0);
        self.list_state.select(Some(next));
    }

    fn descend(&mut self) {
        let Some(i) = self.list_state.selected() else {
            return;
        };
        let Some(&child_idx) = self.children().get(i) else {
            return;
        };
        if self.tree.nodes[child_idx].is_dir && !self.tree.nodes[child_idx].children.is_empty() {
            self.current = child_idx;
            self.list_state.select(Some(0));
        }
    }

    fn ascend(&mut self) {
        if self.current == self.tree.root {
            return;
        }
        let cur_path = self.tree.nodes[self.current].path.clone();
        if let Some(parent) = cur_path.parent() {
            if let Some(&p_idx) = self.tree.by_path.get(parent) {
                let target = self.current;
                self.current = p_idx;
                let sel = self.tree.nodes[p_idx]
                    .children
                    .iter()
                    .position(|&c| c == target)
                    .unwrap_or(0);
                self.list_state.select(Some(sel));
            }
        }
    }
}

pub fn run(root: &Path, files: &[FileEntry]) -> io::Result<()> {
    let tree = Tree::build(root, files);
    let mut state = AppState::new(tree);

    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;

    let result = (|| -> io::Result<()> {
        while !state.quit {
            term.draw(|f| draw(f, &mut state))?;
            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    handle_key(&mut state, key.code, key.modifiers);
                }
            }
            if crate::interrupted() {
                state.quit = true;
            }
        }
        Ok(())
    })();

    terminal::disable_raw_mode()?;
    execute!(term.backend_mut(), terminal::LeaveAlternateScreen)?;
    result
}

fn handle_key(state: &mut AppState, code: KeyCode, mods: KeyModifiers) {
    if state.show_help {
        state.show_help = false;
        return;
    }
    match code {
        KeyCode::Char('q') | KeyCode::Esc => state.quit = true,
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => state.quit = true,
        KeyCode::Char('?') | KeyCode::Char('h') => state.show_help = true,
        KeyCode::Down | KeyCode::Char('j') => state.move_down(),
        KeyCode::Up | KeyCode::Char('k') => state.move_up(),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => state.descend(),
        KeyCode::Left | KeyCode::Backspace => state.ascend(),
        _ => {}
    }
}

fn draw(f: &mut ratatui::Frame<'_>, state: &mut AppState) {
    let area = f.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    let cur = &state.tree.nodes[state.current];
    let header = Paragraph::new(Line::from(vec![
        Span::styled("  bigfiles ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            format_bytes(cur.size),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            cur.path.display().to_string(),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(header, layout[0]);

    let kids = state.children();
    let max_size = kids
        .iter()
        .map(|&i| state.tree.nodes[i].size)
        .max()
        .unwrap_or(0);

    let items: Vec<ListItem> = kids
        .iter()
        .map(|&i| {
            let n = &state.tree.nodes[i];
            let bar_units = if max_size > 0 {
                (n.size as f64 / max_size as f64 * 20.0) as usize
            } else {
                0
            };
            let bar = "█".repeat(bar_units);
            let suffix = if n.is_dir {
                format!("  ({} items)", format_count(n.children.len()))
            } else {
                String::new()
            };
            let name_style = if n.is_dir {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{:>10} ", format_bytes(n.size)),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!(" {:<20} ", bar),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(n.name.clone(), name_style),
                Span::styled(suffix, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ▶ ");
    f.render_stateful_widget(list, layout[1], &mut state.list_state);

    let footer_text = if state.show_help {
        "↑/↓ or j/k: move   ↵/→: open   ←/⌫: up   q: quit   ?: toggle help".to_string()
    } else {
        "↑/↓ move   ↵ open   ← up   q quit   ? help".to_string()
    };
    let footer = Paragraph::new(Span::styled(
        format!("  {}", footer_text),
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(footer, layout[2]);
}
