use std::{
    collections::HashMap,
    fs,
    io::{self, stdout},
    net::{Ipv4Addr, Ipv6Addr},
    process::Command,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
};

const REFRESH_EVERY: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
struct PortEntry {
    protocol: String,
    state: String,
    local: String,
    inode: u64,
    pid: Option<u32>,
    process: String,
    command: String,
}

struct App {
    entries: Vec<PortEntry>,
    selected: usize,
    filter: String,
    filtering: bool,
    confirm_kill: bool,
    status: String,
    last_refresh: Instant,
}

impl App {
    fn new() -> Self {
        let mut app = Self {
            entries: Vec::new(),
            selected: 0,
            filter: String::new(),
            filtering: false,
            confirm_kill: false,
            status: String::from("Loading sockets…"),
            last_refresh: Instant::now() - REFRESH_EVERY,
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        match collect_ports() {
            Ok(mut entries) => {
                entries.sort_by(|a, b| {
                    a.local
                        .cmp(&b.local)
                        .then(a.protocol.cmp(&b.protocol))
                        .then(a.pid.cmp(&b.pid))
                });
                self.entries = entries;
                self.selected = self.selected.min(self.filtered_len().saturating_sub(1));
                self.status = format!("{} sockets · refreshed just now", self.entries.len());
            }
            Err(error) => self.status = format!("Could not read sockets: {error}"),
        }
        self.last_refresh = Instant::now();
    }

    fn filtered_entries(&self) -> Vec<&PortEntry> {
        let needle = self.filter.to_lowercase();
        self.entries
            .iter()
            .filter(|entry| {
                needle.is_empty()
                    || entry.protocol.to_lowercase().contains(&needle)
                    || entry.state.to_lowercase().contains(&needle)
                    || entry.local.to_lowercase().contains(&needle)
                    || entry
                        .pid
                        .map(|pid| pid.to_string())
                        .unwrap_or_default()
                        .contains(&needle)
                    || entry.process.to_lowercase().contains(&needle)
                    || entry.command.to_lowercase().contains(&needle)
            })
            .collect()
    }

    fn filtered_len(&self) -> usize {
        self.filtered_entries().len()
    }

    fn selected_entry(&self) -> Option<&PortEntry> {
        self.filtered_entries().get(self.selected).copied()
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.filtered_len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = ((self.selected as isize + delta).rem_euclid(len as isize)) as usize;
    }

    fn kill_selected(&mut self) {
        let Some(entry) = self.selected_entry() else {
            self.status = "Nothing selected".into();
            return;
        };
        let Some(pid) = entry.pid else {
            self.status = "The socket owner did not expose a PID".into();
            return;
        };
        match Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
        {
            Ok(status) if status.success() => self.status = format!("Sent SIGTERM to {}", pid),
            Ok(status) => self.status = format!("kill exited with {status}"),
            Err(error) => self.status = format!("Could not kill {pid}: {error}"),
        }
        self.confirm_kill = false;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = setup_terminal()?;
    let result = run(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(out))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();
    loop {
        terminal.draw(|frame| draw(frame, &app))?;
        if event::poll(Duration::from_millis(120))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && handle_key(&mut app, key) {
                    break;
                }
            }
        }
        if app.last_refresh.elapsed() >= REFRESH_EVERY && !app.filtering && !app.confirm_kill {
            app.refresh();
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if app.confirm_kill {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => app.kill_selected(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.confirm_kill = false,
            _ => {}
        }
        return false;
    }

    if app.filtering {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => app.filtering = false,
            KeyCode::Backspace => {
                app.filter.pop();
                app.selected = 0;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.filter.push(c);
                app.selected = 0;
            }
            _ => {}
        }
        return false;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return true,
        KeyCode::Char('/') => app.filtering = true,
        KeyCode::Char('r') | KeyCode::Char('R') => app.refresh(),
        KeyCode::Char('x') | KeyCode::Char('X') => {
            if app.selected_entry().is_some() {
                app.confirm_kill = true;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
        KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
        KeyCode::PageUp => app.move_selection(-8),
        KeyCode::PageDown => app.move_selection(8),
        _ => {}
    }
    false
}

fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);

    let title = Line::from(vec![
        Span::styled(
            " PORTWATCH ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  live socket monitor",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let filter = if app.filtering {
        format!("Search: {}▌", app.filter)
    } else if app.filter.is_empty() {
        "Press / to filter".to_string()
    } else {
        format!("Filter: {}", app.filter)
    };
    let header = Paragraph::new(vec![
        title,
        Line::from(Span::styled(filter, Style::default().fg(Color::Yellow))),
    ])
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(header, chunks[0]);

    let rows = app.filtered_entries().into_iter().map(|entry| {
        let pid = entry
            .pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "—".into());
        let state_style = if entry.state == "LISTEN" {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Blue)
        };
        Row::new(vec![
            Cell::from(entry.protocol.clone()).style(Style::default().fg(Color::Magenta)),
            Cell::from(entry.state.clone()).style(state_style),
            Cell::from(entry.local.clone()),
            Cell::from(pid).style(Style::default().fg(Color::Yellow)),
            Cell::from(entry.process.clone()).style(Style::default().fg(Color::Cyan)),
            Cell::from(entry.command.clone()).style(Style::default().fg(Color::Gray)),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(28),
            Constraint::Length(8),
            Constraint::Length(18),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(["PROTO", "STATE", "LOCAL", "PID", "PROCESS", "COMMAND"])
            .style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(1),
    )
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(35, 45, 60))
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▌ ")
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" sockets "),
    );
    let mut state = TableState::default();
    state.select((!app.filtered_entries().is_empty()).then_some(app.selected));
    frame.render_stateful_widget(table, chunks[1], &mut state);

    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            " ↑↓/j k ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("select  "),
        Span::styled(
            "/",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" search  "),
        Span::styled(
            "r",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" refresh  "),
        Span::styled(
            "x",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" kill  "),
        Span::styled(
            "q",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" quit   "),
        Span::styled(&app.status, Style::default().fg(Color::DarkGray)),
    ]));
    frame.render_widget(footer, chunks[2]);

    if app.confirm_kill {
        let popup = centered_rect(60, 20, area);
        let target = app
            .selected_entry()
            .map(|entry| format!("{}:{}", entry.process, entry.local))
            .unwrap_or_default();
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Kill process?",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(format!("{target}")),
                Line::from("Press y to terminate · n to cancel"),
            ])
            .alignment(ratatui::layout::Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .title(" confirm "),
            ),
            popup,
        );
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn collect_ports() -> Result<Vec<PortEntry>, String> {
    let sources = [
        ("TCP", "/proc/net/tcp", true),
        ("TCP", "/proc/net/tcp6", true),
        ("UDP", "/proc/net/udp", false),
        ("UDP", "/proc/net/udp6", false),
    ];
    let mut entries = Vec::new();
    let mut readable_sources = 0;
    for (protocol, path, tcp) in sources {
        match fs::read_to_string(path) {
            Ok(contents) => {
                readable_sources += 1;
                entries.extend(
                    contents
                        .lines()
                        .skip(1)
                        .filter_map(|line| parse_proc_net_line(line, protocol, tcp)),
                );
            }
            Err(_) => {}
        }
    }
    if readable_sources == 0 {
        return Err("could not read /proc/net socket tables".into());
    }
    let owners = socket_owners();
    for entry in &mut entries {
        if let Some((pid, process, command)) = owners.get(&entry.inode) {
            entry.pid = Some(*pid);
            entry.process = process.clone();
            entry.command = command.clone();
        }
        if entry.command.is_empty() {
            entry.command = entry.process.clone();
        }
    }
    Ok(entries)
}

fn parse_proc_net_line(line: &str, protocol: &str, tcp: bool) -> Option<PortEntry> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 10 {
        return None;
    }
    let state = fields[3];
    if tcp && state != "0A" {
        return None;
    }
    let (local, port) = decode_endpoint(fields[1])?;
    if port == 0 {
        return None;
    }
    let inode = fields[9].parse().ok()?;
    Some(PortEntry {
        protocol: protocol.into(),
        state: socket_state(state, tcp),
        local: format!("{local}:{port}"),
        inode,
        pid: None,
        process: "unknown".into(),
        command: String::new(),
    })
}

fn socket_state(code: &str, tcp: bool) -> String {
    match (tcp, code) {
        (true, "0A") => "LISTEN".into(),
        (true, "01") => "ESTABLISHED".into(),
        (false, "07") => "UNCONN".into(),
        _ => code.into(),
    }
}

fn decode_endpoint(value: &str) -> Option<(String, u16)> {
    let (address_hex, port_hex) = value.rsplit_once(':')?;
    let port = u16::from_str_radix(port_hex, 16).ok()?;
    if address_hex.len() == 8 {
        let address = u32::from_str_radix(address_hex, 16).ok()?.to_le_bytes();
        Some((Ipv4Addr::from(address).to_string(), port))
    } else if address_hex.len() == 32 {
        let mut address = [0u8; 16];
        for (index, chunk) in address_hex.as_bytes().chunks(8).enumerate() {
            let word = u32::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
            address[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        Some((format!("[{}]", Ipv6Addr::from(address)), port))
    } else {
        None
    }
}

fn socket_owners() -> HashMap<u64, (u32, String, String)> {
    let mut owners = HashMap::new();
    let Ok(processes) = fs::read_dir("/proc") else {
        return owners;
    };
    for process in processes.flatten() {
        let name = process.file_name();
        let Some(pid) = name.to_str().and_then(|name| name.parse::<u32>().ok()) else {
            continue;
        };
        let comm = fs::read_to_string(format!("/proc/{pid}/comm"))
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .to_string();
        let command = fs::read(format!("/proc/{pid}/cmdline"))
            .map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .replace('\0', " ")
                    .trim()
                    .to_string()
            })
            .unwrap_or_else(|_| comm.clone());
        let Ok(descriptors) = fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        for descriptor in descriptors.flatten() {
            let Ok(link) = fs::read_link(descriptor.path()) else {
                continue;
            };
            let Some(link) = link.to_str() else {
                continue;
            };
            let Some(inode) = link
                .strip_prefix("socket:[")
                .and_then(|value| value.strip_suffix(']'))
            else {
                continue;
            };
            if let Ok(inode) = inode.parse::<u64>() {
                owners
                    .entry(inode)
                    .or_insert((pid, comm.clone(), command.clone()));
            }
        }
    }
    owners
}

#[cfg(test)]
mod tests {
    use super::{decode_endpoint, parse_proc_net_line};

    #[test]
    fn decodes_ipv4_proc_address() {
        assert_eq!(
            decode_endpoint("0100007F:1F90"),
            Some(("127.0.0.1".into(), 8080))
        );
    }

    #[test]
    fn keeps_only_tcp_listeners() {
        let line = "  0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000   100        0 12345 1 0000000000000000 100 0 0 10 0";
        let entry = parse_proc_net_line(line, "TCP", true).expect("listener should parse");
        assert_eq!(entry.local, "127.0.0.1:8080");
        assert_eq!(entry.state, "LISTEN");
    }

    #[test]
    fn ignores_tcp_connections() {
        let line = "  0: 0100007F:1F90 00000000:0000 01 00000000:00000000 00:00000000 00000000   100        0 12345 1 0000000000000000 100 0 0 10 0";
        assert!(parse_proc_net_line(line, "TCP", true).is_none());
    }
}
