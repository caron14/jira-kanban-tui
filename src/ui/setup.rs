use crate::infrastructure::config::JiraAuth;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

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
    Username,
    Token,
    BoardId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupBoard {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SetupState {
    pub step: SetupStep,
    pub field: SetupField,
    pub auth: JiraAuth,
    pub url: String,
    pub username: String,
    pub token: String,
    pub board_input: String,
    pub boards: Vec<SetupBoard>,
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
            url: String::new(),
            username: String::new(),
            token: String::new(),
            board_input: String::new(),
            boards: Vec::new(),
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
    pub fn fields(&self) -> Vec<SetupField> {
        match self.step {
            SetupStep::Boards => vec![SetupField::BoardId],
            SetupStep::Connection => {
                let mut fields = vec![SetupField::Auth, SetupField::Url];
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
            SetupField::Auth => None,
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
    let inner = area.inner(ratatui::layout::Margin { horizontal: 2, vertical: 1 });

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

fn render_connection(frame: &mut Frame, area: Rect, state: &SetupState) {
    let cloud = state.auth == JiraAuth::CloudBasicApiToken;
    let constraints = if cloud {
        vec![
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ]
    } else {
        vec![
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ]
    };
    let chunks =
        Layout::default().direction(Direction::Vertical).constraints(constraints).split(area);
    frame.render_widget(
        Paragraph::new("Tab moves between fields. Enter verifies the account before Board setup."),
        chunks[0],
    );
    let auth = if cloud { "Jira Cloud" } else { "Jira Data Center" };
    render_field(frame, chunks[1], "Jira type (←/→)", auth, state.field == SetupField::Auth);
    render_field(frame, chunks[2], "Jira URL", &state.url, state.field == SetupField::Url);
    let token_index = if cloud {
        render_field(
            frame,
            chunks[3],
            "Email",
            &state.username,
            state.field == SetupField::Username,
        );
        4
    } else {
        3
    };
    let masked = "•".repeat(state.token.chars().count());
    render_field(
        frame,
        chunks[token_index],
        "API Token / PAT",
        if state.show_token { &state.token } else { &masked },
        state.field == SetupField::Token,
    );
    render_message(
        frame,
        *chunks.last().expect("constraints are not empty"),
        state,
        "Enter: verify  Ctrl+T: show token  Esc: quit",
    );
}

fn render_boards(frame: &mut Frame, area: Rect, state: &SetupState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(
            "Add every Board by numeric ID. Each Board is verified before it is listed.",
        ),
        chunks[0],
    );
    render_field(
        frame,
        chunks[1],
        "Board ID",
        &state.board_input,
        state.field == SetupField::BoardId,
    );
    let lines = if state.boards.is_empty() {
        vec![Line::styled("No Boards added", Style::default().fg(Color::DarkGray))]
    } else {
        state
            .boards
            .iter()
            .map(|board| Line::raw(format!("✓ {} — {}", board.id, board.name)))
            .collect()
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Verified Boards ")),
        chunks[2],
    );
    render_message(
        frame,
        chunks[3],
        state,
        "Enter: add Board  Delete: remove last  Ctrl+S: save and open Dashboard",
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
    let line = if state.busy {
        Line::styled("Working…", Style::default().fg(Color::Cyan))
    } else if let Some(message) = &state.message {
        Line::styled(message.clone(), Style::default().fg(Color::Cyan))
    } else {
        Line::from(vec![Span::raw(help)])
    };
    frame.render_widget(Paragraph::new(line), area);
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
