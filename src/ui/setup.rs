use crate::infrastructure::config::JiraAuth;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::layout::{HitRegion, SelectableListRegion};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SetupStep {
    #[default]
    Connection,
    Boards,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SetupField {
    #[default]
    Auth,
    Url,
    AllowInsecureHttp,
    Username,
    Token,
    BoardId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupBoard {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupHit {
    Field(SetupField),
    Board(usize),
    PrimaryAction,
}

#[derive(Debug, Clone)]
pub struct SetupState {
    pub step: SetupStep,
    pub field: SetupField,
    pub auth: JiraAuth,
    pub auth_explicit: bool,
    pub url: String,
    pub allow_insecure_http: bool,
    pub username: String,
    pub token: String,
    pub board_input: String,
    pub boards: Vec<SetupBoard>,
    pub available_boards: Vec<SetupBoard>,
    pub board_index: usize,
    pub preserved_board_ids: Vec<i64>,
    pub preserved_token_env: Option<String>,
    pub preserved_token_command: Option<Vec<String>>,
    pub message: Option<String>,
    pub show_token: bool,
    pub confirm_quit: bool,
    pub busy: bool,
}

impl Default for SetupState {
    fn default() -> Self {
        Self {
            step: SetupStep::Connection,
            field: SetupField::Auth,
            auth: JiraAuth::CloudBasicApiToken,
            auth_explicit: false,
            url: String::new(),
            allow_insecure_http: false,
            username: String::new(),
            token: String::new(),
            board_input: String::new(),
            boards: Vec::new(),
            available_boards: Vec::new(),
            board_index: 0,
            preserved_board_ids: Vec::new(),
            preserved_token_env: None,
            preserved_token_command: None,
            message: None,
            show_token: false,
            confirm_quit: false,
            busy: false,
        }
    }
}

impl SetupState {
    pub fn detect_auth_from_url(&mut self) {
        if self.auth_explicit {
            return;
        }
        let Ok(url) = url::Url::parse(self.url.trim()) else { return };
        let Some(host) = url.host_str() else { return };
        self.auth = if host.ends_with(".atlassian.net") {
            JiraAuth::CloudBasicApiToken
        } else {
            JiraAuth::DataCenterBearerPat
        };
    }

    pub fn fields(&self) -> Vec<SetupField> {
        match self.step {
            SetupStep::Boards => vec![SetupField::BoardId],
            SetupStep::Connection => {
                let mut fields = vec![SetupField::Auth, SetupField::Url];
                if self.url.trim_start().starts_with("http://") {
                    fields.push(SetupField::AllowInsecureHttp);
                }
                if self.auth == JiraAuth::CloudBasicApiToken {
                    fields.push(SetupField::Username);
                }
                fields.push(SetupField::Token);
                fields
            }
        }
    }

    pub fn move_field(&mut self, delta: i32) {
        let fields = self.fields();
        let current = fields.iter().position(|field| *field == self.field).unwrap_or(0) as i32;
        self.field = fields[(current + delta).rem_euclid(fields.len() as i32) as usize];
    }

    pub fn current_value_mut(&mut self) -> Option<&mut String> {
        match self.field {
            SetupField::Url => Some(&mut self.url),
            SetupField::Username => Some(&mut self.username),
            SetupField::Token => Some(&mut self.token),
            SetupField::BoardId => Some(&mut self.board_input),
            SetupField::Auth | SetupField::AllowInsecureHttp => None,
        }
    }

    pub fn matching_boards(&self) -> Vec<&SetupBoard> {
        let query = self.board_input.trim().to_lowercase();
        self.available_boards
            .iter()
            .filter(|board| {
                query.is_empty()
                    || board.name.to_lowercase().contains(&query)
                    || board.id.to_string().contains(&query)
            })
            .collect()
    }

    pub fn toggle_current_board(&mut self) {
        let selected = self.matching_boards().get(self.board_index).map(|board| (*board).clone());
        let Some(selected) = selected else { return };
        if let Some(index) = self.boards.iter().position(|board| board.id == selected.id) {
            self.boards.remove(index);
        } else {
            self.boards.push(selected);
        }
    }
}

pub fn render_setup(frame: &mut Frame, area: Rect, state: &SetupState) {
    frame.render_widget(Clear, area);
    let title = match state.step {
        SetupStep::Connection => " Jira Setup — 1/2 Connection ",
        SetupStep::Boards => " Jira Setup — 2/2 Boards ",
    };
    frame.render_widget(Block::default().borders(Borders::ALL).title(title), area);
    let inner = setup_inner(area);

    match state.step {
        SetupStep::Connection => render_connection(frame, inner, state),
        SetupStep::Boards => render_boards(frame, inner, state),
    }

    if state.confirm_quit {
        let popup = centered(50, 5, area);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new("Discard Setup and quit?\n[y] quit  [n/Esc] continue")
                .block(Block::default().borders(Borders::ALL).title(" Confirm ")),
            popup,
        );
    }
}

pub fn hit_test(area: Rect, state: &SetupState, column: u16, row: u16) -> Option<SetupHit> {
    if state.busy {
        return None;
    }
    let inner = setup_inner(area);
    match state.step {
        SetupStep::Connection => {
            let chunks = connection_chunks(inner, state);
            let mut fields = vec![(SetupField::Auth, 1), (SetupField::Url, 2)];
            let mut next = 3;
            if state.url.trim_start().starts_with("http://") {
                fields.push((SetupField::AllowInsecureHttp, next));
                next += 1;
            }
            if state.auth == JiraAuth::CloudBasicApiToken {
                fields.push((SetupField::Username, next));
                next += 1;
            }
            fields.push((SetupField::Token, next));
            for (field, index) in fields {
                if let Some(hit) =
                    (HitRegion { area: chunks[index], target: SetupHit::Field(field) })
                        .hit(column, row)
                {
                    return Some(hit);
                }
            }
            HitRegion { area: *chunks.last()?, target: SetupHit::PrimaryAction }.hit(column, row)
        }
        SetupStep::Boards => {
            let chunks = board_chunks(inner);
            if let Some(hit) =
                (HitRegion { area: chunks[1], target: SetupHit::Field(SetupField::BoardId) })
                    .hit(column, row)
            {
                return Some(hit);
            }
            let matches = state.matching_boards();
            if let Some(index) =
                board_region(chunks[2], matches.len(), state.board_index).hit(column, row)
            {
                return Some(SetupHit::Board(index));
            }
            HitRegion { area: chunks[3], target: SetupHit::PrimaryAction }.hit(column, row)
        }
    }
}

fn setup_inner(area: Rect) -> Rect {
    area.inner(ratatui::layout::Margin { horizontal: 2, vertical: 1 })
}

fn connection_chunks(area: Rect, state: &SetupState) -> Vec<Rect> {
    let cloud = state.auth == JiraAuth::CloudBasicApiToken;
    let show_insecure = state.url.trim_start().starts_with("http://");
    let mut constraints = vec![Constraint::Length(2), Constraint::Length(3), Constraint::Length(3)];
    if show_insecure {
        constraints.push(Constraint::Length(3));
    }
    if cloud {
        constraints.push(Constraint::Length(3));
    }
    constraints.extend([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)]);
    Layout::default().direction(Direction::Vertical).constraints(constraints).split(area).to_vec()
}

fn board_chunks(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(area)
        .to_vec()
}

fn board_region(area: Rect, item_count: usize, selected: usize) -> SelectableListRegion {
    SelectableListRegion { area, item_count, selected, row_height: 1 }
}

fn render_connection(frame: &mut Frame, area: Rect, state: &SetupState) {
    let cloud = state.auth == JiraAuth::CloudBasicApiToken;
    let show_insecure = state.url.trim_start().starts_with("http://");
    let chunks = connection_chunks(area, state);
    frame.render_widget(
        Paragraph::new("Use ↑/↓ between fields. Enter connects and loads accessible Boards."),
        chunks[0],
    );
    let auth = if cloud { "Jira Cloud" } else { "Jira Data Center" };
    render_field(
        frame,
        chunks[1],
        if state.auth_explicit {
            "Jira type · override (←/→)"
        } else {
            "Jira type · detected (←/→ override)"
        },
        auth,
        state.field == SetupField::Auth,
    );
    render_field(frame, chunks[2], "Jira URL", &state.url, state.field == SetupField::Url);
    let mut next = 3;
    if show_insecure {
        render_field(
            frame,
            chunks[next],
            "Security (←/→)",
            if state.allow_insecure_http { "Allow insecure HTTP" } else { "Require HTTPS" },
            state.field == SetupField::AllowInsecureHttp,
        );
        next += 1;
    }
    if cloud {
        render_field(
            frame,
            chunks[next],
            "Email",
            &state.username,
            state.field == SetupField::Username,
        );
        next += 1;
    }
    let masked = "•".repeat(state.token.chars().count());
    render_field(
        frame,
        chunks[next],
        "API Token / PAT",
        if state.show_token { &state.token } else { &masked },
        state.field == SetupField::Token,
    );
    render_message(
        frame,
        *chunks.last().expect("constraints are not empty"),
        state,
        "[ Verify connection ]  Ctrl+T: show token  Esc: quit",
    );
}

fn render_boards(frame: &mut Frame, area: Rect, state: &SetupState) {
    let chunks = board_chunks(area);
    frame.render_widget(
        Paragraph::new("Search accessible Boards by name. Space selects more than one."),
        chunks[0],
    );
    render_field(frame, chunks[1], "Find Board", &state.board_input, true);
    let matches = state.matching_boards();
    let lines = if matches.is_empty() {
        vec![Line::styled(
            "No accessible Board matches the search",
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        matches
            .iter()
            .enumerate()
            .map(|(index, board)| {
                let checked = state.boards.iter().any(|selected| selected.id == board.id);
                let marker = if index == state.board_index { "▶" } else { " " };
                Line::raw(format!(
                    "{marker} [{}] {} — {}",
                    if checked { "x" } else { " " },
                    board.name,
                    board.id
                ))
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((board_region(chunks[2], matches.len(), state.board_index).scroll() as u16, 0))
            .block(Block::default().borders(Borders::ALL).title(" Accessible Boards ")),
        chunks[2],
    );
    render_message(
        frame,
        chunks[3],
        state,
        "[ Use selected Boards ]  Space: select multiple  Esc: back",
    );
}

fn render_field(frame: &mut Frame, area: Rect, label: &str, value: &str, focused: bool) {
    let style = if focused { Style::default().fg(Color::Yellow) } else { Style::default() };
    frame.render_widget(
        Paragraph::new(value).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{} {label} ", if focused { "▶" } else { " " }))
                .border_style(style),
        ),
        area,
    );
}

fn render_message(frame: &mut Frame, area: Rect, state: &SetupState, help: &str) {
    let lines = if state.busy {
        vec![Line::styled("Working…", Style::default().fg(Color::Cyan)), Line::raw(help)]
    } else if let Some(message) = &state.message {
        vec![Line::styled(message.clone(), Style::default().fg(Color::Cyan)), Line::raw(help)]
    } else {
        vec![Line::from(vec![Span::raw(help)])]
    };
    frame.render_widget(Paragraph::new(lines), area);
}

fn centered(percent_x: u16, height: u16, area: Rect) -> Rect {
    let width = area.width.saturating_mul(percent_x) / 100;
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height: height.min(area.height),
    }
}
