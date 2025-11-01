use crossterm::{
    event::{self, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs},
};
use std::{
    fs::File,
    io,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

/*
Gaurav Sablok
codeprog@icloud.com
*/

#[derive(Debug, Clone)]
struct VcfRecord {
    chrom: String,
    pos: String,
    id: String,
    ref_: String,
    alt: String,
    qual: String,
    filter: String,
    info: String,
}

#[derive(Default)]
struct App {
    tabs: TabsState,
    files: FileListState,
    vcf: VcfState,
    modal: Option<ModalState>,
}

#[derive(Default)]
struct TabsState {
    index: usize,
    titles: Vec<String>,
}

#[derive(Default)]
struct FileListState {
    items: Vec<PathBuf>,
    selected: Option<usize>,
    filter: String,
}

#[derive(Default)]
struct VcfState {
    records: Vec<VcfRecord>,
    selected: Option<usize>,
    chrom_filter: String,
    ref_filter: String,
    alt_filter: String,
    pos_filter: String, // e.g. "1000-5000" or "12345"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalKind {
    Menu,
    Chrom,
    Ref,
    Alt,
    Pos,
}

#[derive(Default)]
struct ModalState {
    kind: ModalKind,
    input: String,
    menu_selected: usize,
}

impl Default for ModalKind {
    fn default() -> Self {
        ModalKind::Menu
    }
}

impl ModalState {
    fn new_menu() -> Self {
        Self {
            kind: ModalKind::Menu,
            input: String::new(),
            menu_selected: 0,
        }
    }
    fn new_input(kind: ModalKind) -> Self {
        Self {
            kind,
            input: String::new(),
            menu_selected: 0,
        }
    }
}

fn parse_vcf(path: &Path) -> Result<Vec<VcfRecord>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 5 {
            continue;
        }

        records.push(VcfRecord {
            chrom: fields[0].to_string(),
            pos: fields[1].to_string(),
            id: fields[2].to_string(),
            ref_: fields[3].to_string(),
            alt: fields[4].to_string(),
            qual: fields.get(5).unwrap_or(&".").to_string(),
            filter: fields.get(6).unwrap_or(&".").to_string(),
            info: fields.get(7).unwrap_or(&".").to_string(),
        });
    }
    Ok(records)
}

impl App {
    fn new() -> Self {
        let mut app = App::default();
        app.tabs.titles = vec!["Files".to_owned(), "VCF Viewer".to_owned()];
        app.load_vcf_files();
        app
    }

    fn load_vcf_files(&mut self) {
        let mut files = Vec::new();
        for entry in WalkDir::new(".").into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "vcf") {
                files.push(path.to_owned());
            }
        }
        self.files.items = files;
    }

    fn load_selected_vcf(&mut self) {
        if let Some(idx) = self.files.selected {
            let path = &self.files.items[idx];
            self.vcf.records = parse_vcf(path).unwrap_or_default();
            self.vcf.selected = None;
        }
    }

    fn filtered_records(&self) -> Vec<&VcfRecord> {
        let pos_range = parse_pos_range(&self.vcf.pos_filter);

        self.vcf
            .records
            .iter()
            .filter(|r| {
                let chrom = self.vcf.chrom_filter.is_empty()
                    || r.chrom
                        .to_lowercase()
                        .contains(&self.vcf.chrom_filter.to_lowercase());
                let ref_ = self.vcf.ref_filter.is_empty()
                    || r.ref_
                        .to_lowercase()
                        .contains(&self.vcf.ref_filter.to_lowercase());
                let alt = self.vcf.alt_filter.is_empty()
                    || r.alt
                        .to_lowercase()
                        .contains(&self.vcf.alt_filter.to_lowercase());

                let pos_ok = match pos_range {
                    PosRange::None => true,
                    PosRange::Exact(pos) => r.pos == pos.to_string(),
                    PosRange::Range(start, end) => {
                        if let Ok(p) = r.pos.parse::<u64>() {
                            p >= start && p <= end
                        } else {
                            false
                        }
                    }
                };

                chrom && ref_ && alt && pos_ok
            })
            .collect()
    }
}

#[derive(Debug)]
enum PosRange {
    None,
    Exact(u64),
    Range(u64, u64),
}

fn parse_pos_range(input: &str) -> PosRange {
    let s = input.trim();
    if s.is_empty() {
        return PosRange::None;
    }

    if let Ok(pos) = s.parse::<u64>() {
        return PosRange::Exact(pos);
    }

    if let Some((start_str, end_str)) = s.split_once('-') {
        let start = start_str.trim().parse::<u64>();
        let end = end_str.trim().parse::<u64>();
        if let (Ok(start), Ok(end)) = (start, end) {
            if start <= end {
                return PosRange::Range(start, end);
            }
        }
    }

    PosRange::None
}

fn ui(f: &mut ratatui::Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(f.area());

    let titles: Vec<_> = app.tabs.titles.iter().cloned().map(Line::from).collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("VCF TUI"))
        .select(app.tabs.index)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    match app.tabs.index {
        0 => render_file_tab(f, app, chunks[1]),
        1 => render_vcf_tab(f, app, chunks[1]),
        _ => {}
    }

    if let Some(modal) = &app.modal {
        render_modal(f, modal, app);
    }
}

fn render_file_tab(f: &mut ratatui::Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(area);

    let filter = Paragraph::new(format!("Filter: {}", app.files.filter))
        .block(Block::default().borders(Borders::ALL).title("File Filter"))
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(filter, chunks[0]);

    let items: Vec<ListItem> = app
        .files
        .items
        .iter()
        .filter(|p| {
            app.files.filter.is_empty()
                || p.to_str()
                    .unwrap()
                    .to_lowercase()
                    .contains(&app.files.filter.to_lowercase())
        })
        .enumerate()
        .map(|(i, path)| {
            let name = path.file_name().unwrap().to_string_lossy();
            let style = if Some(i) == app.files.selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(name, style)))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("VCF Files (Up/Down move, Enter open)"),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_widget(list, chunks[1]);
}

fn render_vcf_tab(f: &mut ratatui::Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(area);

    let filter_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(chunks[0]);

    let chrom = Paragraph::new(format!("CHROM: {}", app.vcf.chrom_filter))
        .block(Block::default().borders(Borders::ALL).title("Filter"))
        .style(Style::default().fg(Color::Green));
    f.render_widget(chrom, filter_chunks[0]);

    let ref_ = Paragraph::new(format!("REF: {}", app.vcf.ref_filter))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Green));
    f.render_widget(ref_, filter_chunks[1]);

    let alt = Paragraph::new(format!("ALT: {}", app.vcf.alt_filter))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Green));
    f.render_widget(alt, filter_chunks[2]);

    let pos = Paragraph::new(format!("POS: {}", app.vcf.pos_filter))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Green));
    f.render_widget(pos, filter_chunks[3]);

    let filtered = app.filtered_records();
    let mut list_state = ListState::default();
    list_state.select(app.vcf.selected);

    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let line = format!("{}:{} {}>{}", r.chrom, r.pos, r.ref_, r.alt);
            let style = if Some(i) == app.vcf.selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(line, style)))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Variants (Up/Down, f = filter menu)"),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    f.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn render_modal(f: &mut ratatui::Frame, modal: &ModalState, _app: &App) {
    let area = centered_rect(60, 30, f.area());
    f.render_widget(Clear, area);

    match modal.kind {
        ModalKind::Menu => {
            let items = vec!["CHROM", "REF", "ALT", "POS", "Clear all", "Cancel"];
            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, txt)| {
                    let style = if i == modal.menu_selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(Span::styled(*txt, style)))
                })
                .collect();

            let list = List::new(list_items)
                .block(
                    Block::default()
                        .title("Filter Menu (Up/Down, Enter)")
                        .borders(Borders::ALL),
                )
                .highlight_style(Style::default().bg(Color::DarkGray));

            let mut state = ListState::default();
            state.select(Some(modal.menu_selected));
            f.render_stateful_widget(list, area, &mut state);
        }
        ModalKind::Chrom | ModalKind::Ref | ModalKind::Alt | ModalKind::Pos => {
            let title = match modal.kind {
                ModalKind::Chrom => "CHROM filter (Esc cancel, Enter accept)",
                ModalKind::Ref => "REF filter (Esc cancel, Enter accept)",
                ModalKind::Alt => "ALT filter (Esc cancel, Enter accept)",
                ModalKind::Pos => "POS filter: 12345 or 1000-5000 (Esc cancel, Enter accept)",
                _ => unreachable!(),
            };
            let input = Paragraph::new(modal.input.as_str())
                .style(Style::default().fg(Color::Cyan))
                .block(Block::default().title(title).borders(Borders::ALL));
            f.render_widget(input, area);
        }
    }
}

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    app.files.selected = if app.files.items.is_empty() {
        None
    } else {
        Some(0)
    };
    if app.files.selected.is_some() {
        app.load_selected_vcf();
    }

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if app.modal.is_some() {
                handle_modal_key(&mut app, key);
                continue;
            }

            match app.tabs.index {
                0 => handle_files_tab(&mut app, key),
                1 => handle_vcf_tab(&mut app, key),
                _ => {}
            }
        }
    }
}

fn handle_files_tab(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('q') => std::process::exit(0),
        KeyCode::Down => {
            if let Some(sel) = app.files.selected {
                if sel + 1 < app.files.items.len() {
                    app.files.selected = Some(sel + 1);
                }
            }
        }
        KeyCode::Up => {
            if let Some(sel) = app.files.selected {
                if sel > 0 {
                    app.files.selected = Some(sel - 1);
                }
            }
        }
        KeyCode::Enter => {
            app.load_selected_vcf();
            app.tabs.index = 1;
        }
        KeyCode::Char(c) => {
            app.files.filter.push(c);
        }
        KeyCode::Backspace => {
            app.files.filter.pop();
        }
        KeyCode::Tab => {
            app.tabs.index = (app.tabs.index + 1) % app.tabs.titles.len();
        }
        _ => {}
    }
}

fn handle_vcf_tab(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.tabs.index = 0;
        }
        KeyCode::Down => {
            let filtered = app.filtered_records();
            if let Some(sel) = app.vcf.selected {
                if sel + 1 < filtered.len() {
                    app.vcf.selected = Some(sel + 1);
                }
            } else if !filtered.is_empty() {
                app.vcf.selected = Some(0);
            }
        }
        KeyCode::Up => {
            if let Some(sel) = app.vcf.selected {
                if sel > 0 {
                    app.vcf.selected = Some(sel - 1);
                }
            }
        }
        KeyCode::Char('f') => {
            app.modal = Some(ModalState::new_menu());
        }
        KeyCode::Tab => {
            app.tabs.index = (app.tabs.index + 1) % app.tabs.titles.len();
        }
        _ => {}
    }
}

fn handle_modal_key(app: &mut App, key: crossterm::event::KeyEvent) {
    let modal = app.modal.as_mut().unwrap();

    match modal.kind {
        ModalKind::Menu => match key.code {
            KeyCode::Up => {
                if modal.menu_selected > 0 {
                    modal.menu_selected -= 1;
                }
            }
            KeyCode::Down => {
                if modal.menu_selected < 5 {
                    modal.menu_selected += 1;
                }
            }
            KeyCode::Enter => match modal.menu_selected {
                0 => app.modal = Some(ModalState::new_input(ModalKind::Chrom)),
                1 => app.modal = Some(ModalState::new_input(ModalKind::Ref)),
                2 => app.modal = Some(ModalState::new_input(ModalKind::Alt)),
                3 => app.modal = Some(ModalState::new_input(ModalKind::Pos)),
                4 => {
                    app.vcf.chrom_filter.clear();
                    app.vcf.ref_filter.clear();
                    app.vcf.alt_filter.clear();
                    app.vcf.pos_filter.clear();
                    app.modal = None;
                }
                5 => app.modal = None,
                _ => {}
            },
            KeyCode::Esc => app.modal = None,
            _ => {}
        },
        ModalKind::Chrom | ModalKind::Ref | ModalKind::Alt | ModalKind::Pos => match key.code {
            KeyCode::Char(c) => {
                modal.input.push(c);
            }
            KeyCode::Backspace => {
                modal.input.pop();
            }
            KeyCode::Enter => {
                let txt = modal.input.trim().to_string();
                match modal.kind {
                    ModalKind::Chrom => app.vcf.chrom_filter = txt,
                    ModalKind::Ref => app.vcf.ref_filter = txt,
                    ModalKind::Alt => app.vcf.alt_filter = txt,
                    ModalKind::Pos => app.vcf.pos_filter = txt,
                    _ => {}
                }
                app.modal = None;
            }
            KeyCode::Esc => app.modal = None,
            _ => {}
        },
    }
}
