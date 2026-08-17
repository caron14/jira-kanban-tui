use crate::domain::{filter::BuiltInFilter, Board, Issue};
use crate::infrastructure::config::{Config, JiraAuth, JiraConfig, CONFIG_VERSION};
use crate::jira::{Choice, TransitionOption, UpdateCommand};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
    None,
    Quit,
    Refresh,
    OpenIssue,
    OpenSetup,
    SwitchBoard(usize),
    LoadActivity,
    LoadTransitions,
    LoadAssignees(String),
    LoadPriorities,
    Update(UpdateCommand),
    TestSetupConnection,
    AddSetupBoard(i64),
    SaveSetup,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Board,
    #[default]
    Dashboard,
    Wbs,
    Activity,
    Setup,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    #[default]
    Connected,
    Refreshing,
    Offline,
    RateLimited,
    AuthError,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    #[default]
    None,
    Detail,
    EditMenu,
    TransitionPicker,
    AssigneePicker,
    DueDateEditor,
    PriorityPicker,
    BoardPicker,
    Help,
    Error,
    Search,
    Filter,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub view: View,
    pub modal: Modal,
    pub network: NetworkState,
    pub loading: bool,
    pub refreshing: bool,
    pub offline: bool,
    pub board: Option<Board>,
    pub issues: Vec<Issue>,
    pub filtered_issues: Vec<usize>,
    pub column_issue_cache: Vec<Vec<usize>>,
    pub selected_col: usize,
    pub column_rows: Vec<usize>,
    pub col_scroll: usize,
    pub search_query: Option<String>,
    pub filter: Option<BuiltInFilter>,
    pub filter_index: usize,
    pub error: Option<String>,
    pub retry_action: Option<AppAction>,
    pub input_buffer: String,
    pub status_message: Option<String>,
    pub setup: crate::ui::setup::SetupState,
    pub transitions: Vec<TransitionOption>,
    pub choices: Vec<Choice>,
    pub picker_index: usize,
    pub edit_index: usize,
    pub board_refs: Vec<String>,
    pub board_names: Vec<String>,
    pub board_ref_index: usize,
    pub current_user: Option<String>,
    pub expanded: HashSet<String>,
    pub request_generation: u64,
    pub dashboard_selected: usize,
    pub wbs_selected: usize,
    pub activity_selected: usize,
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub activities: Vec<crate::domain::activity::Activity>,
    pub updating_key: Option<String>,
}

impl AppState {
    pub fn current_board_ref(&self) -> Option<&str> {
        self.board_refs.get(self.board_ref_index).map(String::as_str)
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        if self.search_query.is_some() || self.filter.is_some() {
            self.filtered_issues.clone()
        } else {
            (0..self.issues.len()).collect()
        }
    }

    pub fn visible_issues(&self) -> Vec<&Issue> {
        self.visible_indices().into_iter().filter_map(|index| self.issues.get(index)).collect()
    }

    pub fn has_other_column(&self) -> bool {
        self.board
            .as_ref()
            .map(|board| self.column_issue_cache.len() > board.columns.len())
            .unwrap_or(false)
    }

    pub fn column_count(&self) -> usize {
        if self.column_issue_cache.is_empty() {
            self.board.as_ref().map(|board| board.columns.len()).unwrap_or(0)
        } else {
            self.column_issue_cache.len()
        }
    }

    pub fn column_label(&self, column_index: usize) -> Option<String> {
        let board = self.board.as_ref()?;
        if let Some(column) = board.columns.get(column_index) {
            Some(column.name.clone())
        } else if column_index == board.columns.len() && self.has_other_column() {
            Some("Other".into())
        } else {
            None
        }
    }

    pub fn column_issue_indices(&self, column_index: usize) -> &[usize] {
        self.column_issue_cache.get(column_index).map(Vec::as_slice).unwrap_or(&[])
    }

    fn rebuild_column_cache(&mut self) {
        let Some(board) = &self.board else {
            self.column_issue_cache.clear();
            return;
        };
        let mut columns = vec![Vec::new(); board.columns.len()];
        let mut other = Vec::new();
        for issue_index in self.visible_indices() {
            if let Some(column_index) = board
                .columns
                .iter()
                .position(|column| column.statuses.contains(&self.issues[issue_index].status))
            {
                columns[column_index].push(issue_index);
            } else {
                other.push(issue_index);
            }
        }
        if !other.is_empty() {
            columns.push(other);
        }
        self.column_issue_cache = columns;
    }

    pub fn ensure_column_rows(&mut self) {
        if self.column_issue_cache.is_empty() && self.board.is_some() {
            self.rebuild_column_cache();
        }
        let count = self.column_count();
        self.column_rows.resize(count, 0);
        if count > 0 {
            self.selected_col = self.selected_col.min(count - 1);
            let len = self.column_issue_indices(self.selected_col).len();
            if len == 0 {
                self.column_rows[self.selected_col] = 0;
            } else {
                self.column_rows[self.selected_col] =
                    self.column_rows[self.selected_col].min(len.saturating_sub(1));
            }
        } else {
            self.selected_col = 0;
        }
    }

    fn board_selected_issue(&self) -> Option<&Issue> {
        let row = *self.column_rows.get(self.selected_col).unwrap_or(&0);
        self.column_issue_indices(self.selected_col)
            .get(row)
            .and_then(|index| self.issues.get(*index))
    }

    fn done_and_progress_statuses(&self) -> (Vec<String>, Vec<String>) {
        let Some(board) = &self.board else { return (Vec::new(), Vec::new()) };
        let done = board.columns.last().map(|column| column.statuses.clone()).unwrap_or_default();
        let progress = if board.columns.len() > 2 {
            board.columns[1..board.columns.len() - 1]
                .iter()
                .flat_map(|column| column.statuses.clone())
                .collect()
        } else {
            Vec::new()
        };
        (done, progress)
    }

    pub fn attention_items(&self) -> Vec<crate::domain::dashboard::AttentionItem> {
        let (done, progress) = self.done_and_progress_statuses();
        crate::domain::dashboard::attention_sorted(&self.issues, &done, &progress)
    }

    pub fn selected_issue(&self) -> Option<&Issue> {
        match self.view {
            View::Board => self.board_selected_issue(),
            View::Dashboard => self
                .attention_items()
                .get(self.dashboard_selected)
                .and_then(|item| self.issues.iter().find(|issue| issue.key == item.issue.key)),
            View::Wbs => self
                .visible_wbs_keys()
                .get(self.wbs_selected)
                .and_then(|key| self.issues.iter().find(|issue| &issue.key == key)),
            View::Activity => self
                .activities
                .get(self.activity_selected)
                .and_then(|activity| self.issues.iter().find(|issue| issue.key == activity.key)),
            View::Setup => None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if self.view == View::Setup {
            return self.handle_setup_key(key);
        }
        self.ensure_column_rows();
        if self.modal != Modal::None {
            return self.handle_modal_key(key);
        }
        self.status_message = None;
        match key.code {
            KeyCode::Char('q') if key.modifiers.is_empty() => return AppAction::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return AppAction::Quit
            }
            KeyCode::Char('1') => self.view = View::Board,
            KeyCode::Char('2') => self.view = View::Dashboard,
            KeyCode::Char('3') => self.view = View::Wbs,
            KeyCode::Char('4') => {
                self.view = View::Activity;
                return AppAction::LoadActivity;
            }
            KeyCode::Char('?') => self.modal = Modal::Help,
            KeyCode::Char('r') => return AppAction::Refresh,
            KeyCode::Char('b') if self.board_refs.len() > 1 => {
                self.picker_index = self.board_ref_index;
                self.modal = Modal::BoardPicker;
            }
            KeyCode::Char('/') if self.view == View::Board => {
                self.input_buffer = self.search_query.clone().unwrap_or_default();
                self.modal = Modal::Search;
            }
            KeyCode::Char('f') if self.view == View::Board => {
                self.filter_index = self
                    .filter
                    .as_ref()
                    .and_then(|selected| {
                        BuiltInFilter::all().iter().position(|filter| filter == selected)
                    })
                    .map(|index| index + 1)
                    .unwrap_or(0);
                self.modal = Modal::Filter;
            }
            KeyCode::Enter if self.selected_issue().is_some() => self.modal = Modal::Detail,
            KeyCode::Char('e') if self.selected_issue().is_some() && !self.offline => {
                self.edit_index = 0;
                self.modal = Modal::EditMenu;
            }
            KeyCode::Char('o') if self.selected_issue().is_some() => return AppAction::OpenIssue,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('h') | KeyCode::Left => self.move_horizontal(-1),
            KeyCode::Char('l') | KeyCode::Right => self.move_horizontal(1),
            _ => {}
        }
        AppAction::None
    }

    fn move_selection(&mut self, delta: i32) {
        match self.view {
            View::Board => {
                let len = self.column_issue_indices(self.selected_col).len();
                if let Some(row) = self.column_rows.get_mut(self.selected_col) {
                    *row = move_index(*row, len, delta);
                }
            }
            View::Dashboard => {
                self.dashboard_selected =
                    move_index(self.dashboard_selected, self.attention_items().len(), delta)
            }
            View::Wbs => {
                self.wbs_selected =
                    move_index(self.wbs_selected, self.visible_wbs_keys().len(), delta)
            }
            View::Activity => {
                self.activity_selected =
                    move_index(self.activity_selected, self.activities.len(), delta)
            }
            View::Setup => {}
        }
    }

    fn move_horizontal(&mut self, delta: i32) {
        match self.view {
            View::Board => {
                self.selected_col = move_index(self.selected_col, self.column_count(), delta);
                self.ensure_column_rows();
            }
            View::Wbs => {
                if let Some(key) = self.visible_wbs_keys().get(self.wbs_selected).cloned() {
                    if delta < 0 {
                        self.expanded.remove(&key);
                    } else {
                        self.expanded.insert(key);
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> AppAction {
        if key.code == KeyCode::Esc {
            if self.modal == Modal::Search {
                self.search_query = None;
                self.apply_filters();
            }
            self.modal = Modal::None;
            self.error = None;
            self.input_buffer.clear();
            return AppAction::None;
        }
        match self.modal {
            Modal::Search => match key.code {
                KeyCode::Enter => self.modal = Modal::None,
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                    self.search_query = non_empty(&self.input_buffer);
                    self.apply_filters();
                }
                KeyCode::Char(c) => {
                    self.input_buffer.push(c);
                    self.search_query = non_empty(&self.input_buffer);
                    self.apply_filters();
                }
                _ => {}
            },
            Modal::Filter => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.filter_index = move_index(self.filter_index, 4, 1)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.filter_index = move_index(self.filter_index, 4, -1)
                }
                KeyCode::Enter => {
                    self.filter = if self.filter_index == 0 {
                        None
                    } else {
                        BuiltInFilter::all().get(self.filter_index - 1).cloned()
                    };
                    self.apply_filters();
                    self.modal = Modal::None;
                }
                _ => {}
            },
            Modal::BoardPicker => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.picker_index = move_index(self.picker_index, self.board_refs.len(), 1)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.picker_index = move_index(self.picker_index, self.board_refs.len(), -1)
                }
                KeyCode::Enter => {
                    let index = self.picker_index;
                    self.modal = Modal::None;
                    return AppAction::SwitchBoard(index);
                }
                _ => {}
            },
            Modal::Detail => match key.code {
                KeyCode::Char('e') if !self.offline => {
                    self.edit_index = 0;
                    self.modal = Modal::EditMenu;
                }
                KeyCode::Char('o') => return AppAction::OpenIssue,
                _ => {}
            },
            Modal::EditMenu => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.edit_index = move_index(self.edit_index, 4, 1)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.edit_index = move_index(self.edit_index, 4, -1)
                }
                KeyCode::Enter => match self.edit_index {
                    0 => return AppAction::LoadTransitions,
                    1 => {
                        self.input_buffer.clear();
                        self.choices.clear();
                        self.picker_index = 0;
                        self.modal = Modal::AssigneePicker;
                        return AppAction::LoadAssignees(String::new());
                    }
                    2 => {
                        self.input_buffer.clear();
                        self.modal = Modal::DueDateEditor;
                    }
                    _ => {
                        self.choices.clear();
                        self.picker_index = 0;
                        self.modal = Modal::PriorityPicker;
                        return AppAction::LoadPriorities;
                    }
                },
                _ => {}
            },
            Modal::TransitionPicker => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.picker_index = move_index(self.picker_index, self.transitions.len(), 1)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.picker_index = move_index(self.picker_index, self.transitions.len(), -1)
                }
                KeyCode::Enter => {
                    if let Some(transition) = self.transitions.get(self.picker_index) {
                        let command =
                            UpdateCommand::Transition { transition_id: transition.id.clone() };
                        self.modal = Modal::None;
                        return AppAction::Update(command);
                    }
                }
                _ => {}
            },
            Modal::AssigneePicker => match key.code {
                KeyCode::Up => {
                    self.picker_index = move_index(self.picker_index, self.choices.len(), -1)
                }
                KeyCode::Down => {
                    self.picker_index = move_index(self.picker_index, self.choices.len(), 1)
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                    return AppAction::LoadAssignees(self.input_buffer.clone());
                }
                KeyCode::Delete => {
                    self.modal = Modal::None;
                    return AppAction::Update(UpdateCommand::Assignee { account_id: None });
                }
                KeyCode::Enter => {
                    if let Some(choice) = self.choices.get(self.picker_index) {
                        let command =
                            UpdateCommand::Assignee { account_id: Some(choice.id.clone()) };
                        self.modal = Modal::None;
                        return AppAction::Update(command);
                    }
                }
                KeyCode::Char(c) => {
                    self.input_buffer.push(c);
                    return AppAction::LoadAssignees(self.input_buffer.clone());
                }
                _ => {}
            },
            Modal::PriorityPicker => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.picker_index = move_index(self.picker_index, self.choices.len(), 1)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.picker_index = move_index(self.picker_index, self.choices.len(), -1)
                }
                KeyCode::Enter => {
                    if let Some(choice) = self.choices.get(self.picker_index) {
                        let command = UpdateCommand::Priority { id: choice.id.clone() };
                        self.modal = Modal::None;
                        return AppAction::Update(command);
                    }
                }
                _ => {}
            },
            Modal::DueDateEditor => match key.code {
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                }
                KeyCode::Char(c) => self.input_buffer.push(c),
                KeyCode::Enter => {
                    let value = if self.input_buffer.is_empty() {
                        None
                    } else {
                        match chrono::NaiveDate::parse_from_str(&self.input_buffer, "%Y-%m-%d") {
                            Ok(value) => Some(value),
                            Err(_) => {
                                self.error = Some("Use YYYY-MM-DD, or leave empty to clear".into());
                                return AppAction::None;
                            }
                        }
                    };
                    self.modal = Modal::None;
                    return AppAction::Update(UpdateCommand::DueDate { value });
                }
                _ => {}
            },
            Modal::Error => match key.code {
                KeyCode::Char('r') if self.retry_action.is_some() => {
                    self.modal = Modal::None;
                    self.error = None;
                    return self.retry_action.clone().unwrap_or(AppAction::None);
                }
                KeyCode::Char('o') if self.selected_issue().is_some() => {
                    return AppAction::OpenIssue
                }
                KeyCode::Char('s') if self.network == NetworkState::AuthError => {
                    self.modal = Modal::None;
                    return AppAction::OpenSetup;
                }
                _ => {}
            },
            Modal::Help | Modal::None => {}
        }
        AppAction::None
    }

    fn handle_setup_key(&mut self, key: KeyEvent) -> AppAction {
        use crate::ui::setup::{SetupField, SetupStep};
        if self.setup.confirm_quit {
            match key.code {
                KeyCode::Char('y') => return AppAction::Quit,
                KeyCode::Char('n') | KeyCode::Esc => self.setup.confirm_quit = false,
                _ => {}
            }
            return AppAction::None;
        }
        if self.setup.busy {
            return AppAction::None;
        }
        match key.code {
            KeyCode::Esc => self.setup.confirm_quit = true,
            KeyCode::Tab => self.setup.move_field(1),
            KeyCode::BackTab => self.setup.move_field(-1),
            KeyCode::Left | KeyCode::Right if self.setup.field == SetupField::Auth => {
                self.setup.auth = match self.setup.auth {
                    JiraAuth::CloudBasicApiToken => JiraAuth::DataCenterBearerPat,
                    JiraAuth::DataCenterBearerPat => JiraAuth::CloudBasicApiToken,
                };
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.setup.show_token = !self.setup.show_token
            }
            KeyCode::Char('s')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.setup.step == SetupStep::Boards =>
            {
                if self.setup.boards.is_empty() {
                    self.setup.message = Some("Add at least one verified Board".into());
                } else {
                    return AppAction::SaveSetup;
                }
            }
            KeyCode::Enter if self.setup.step == SetupStep::Connection => {
                return AppAction::TestSetupConnection
            }
            KeyCode::Enter if self.setup.step == SetupStep::Boards => {
                match self.setup.board_input.parse::<i64>() {
                    Ok(id) if id > 0 => return AppAction::AddSetupBoard(id),
                    _ => self.setup.message = Some("Board ID must be a positive number".into()),
                }
            }
            KeyCode::Delete if self.setup.step == SetupStep::Boards => {
                self.setup.boards.pop();
            }
            KeyCode::Backspace => {
                if let Some(value) = self.setup.current_value_mut() {
                    value.pop();
                }
            }
            KeyCode::Char(c) => {
                let board_field = self.setup.field == SetupField::BoardId;
                if let Some(value) = self.setup.current_value_mut() {
                    if !board_field || c.is_ascii_digit() {
                        value.push(c);
                    }
                }
            }
            _ => {}
        }
        AppAction::None
    }

    pub fn handle_paste(&mut self, value: &str) -> AppAction {
        if self.view == View::Setup {
            let board_field = self.setup.field == crate::ui::setup::SetupField::BoardId;
            if let Some(target) = self.setup.current_value_mut() {
                if board_field {
                    target.extend(value.chars().filter(char::is_ascii_digit));
                } else {
                    target.extend(
                        value.chars().filter(|character| !matches!(character, '\r' | '\n')),
                    );
                }
            }
            return AppAction::None;
        }
        match self.modal {
            Modal::Search | Modal::AssigneePicker | Modal::DueDateEditor => {
                self.input_buffer
                    .extend(value.chars().filter(|character| !matches!(character, '\r' | '\n')));
                if self.modal == Modal::Search {
                    self.search_query = non_empty(&self.input_buffer);
                    self.apply_filters();
                } else if self.modal == Modal::AssigneePicker {
                    return AppAction::LoadAssignees(self.input_buffer.clone());
                }
            }
            _ => {}
        }
        AppAction::None
    }

    pub fn handle_mouse(&mut self, event: MouseEvent) -> AppAction {
        use crossterm::event::{MouseButton, MouseEventKind};
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if event.row <= 1 => {
                let tab_width = (self.terminal_width / 4).max(1);
                self.view = match usize::from(event.column / tab_width).min(3) {
                    0 => View::Board,
                    1 => View::Dashboard,
                    2 => View::Wbs,
                    _ => View::Activity,
                };
                if self.view == View::Activity {
                    AppAction::LoadActivity
                } else {
                    AppAction::None
                }
            }
            MouseEventKind::Down(MouseButton::Left) if self.modal == Modal::None => {
                match self.view {
                    View::Board => {
                        let count = self.column_count();
                        let visible = usize::from((self.terminal_width / 24).max(1)).min(count);
                        let start = if self.selected_col >= self.col_scroll + visible {
                            self.selected_col + 1 - visible
                        } else {
                            self.col_scroll.min(count.saturating_sub(visible))
                        };
                        let width = (self.terminal_width / visible.max(1) as u16).max(1);
                        let column = start + usize::from(event.column / width);
                        if column < count {
                            self.selected_col = column;
                            let visible_rows =
                                usize::from(self.terminal_height.saturating_sub(5)) / 4;
                            let selected = *self.column_rows.get(column).unwrap_or(&0);
                            let offset = selected.saturating_sub(visible_rows.saturating_sub(1));
                            let row = offset + usize::from(event.row.saturating_sub(3)) / 4;
                            let len = self.column_issue_indices(column).len();
                            if len > 0 {
                                self.column_rows[column] = row.min(len - 1);
                            }
                        }
                    }
                    View::Dashboard => {
                        let row = usize::from(event.row.saturating_sub(9));
                        let visible = usize::from(self.terminal_height.saturating_sub(15));
                        let offset =
                            self.dashboard_selected.saturating_sub(visible.saturating_sub(1));
                        self.dashboard_selected =
                            (offset + row).min(self.attention_items().len().saturating_sub(1));
                    }
                    View::Wbs => {
                        let row = usize::from(event.row.saturating_sub(3));
                        let visible = usize::from(self.terminal_height.saturating_sub(5));
                        let offset = self.wbs_selected.saturating_sub(visible.saturating_sub(1));
                        self.wbs_selected =
                            (offset + row).min(self.visible_wbs_keys().len().saturating_sub(1));
                    }
                    View::Activity => {
                        let row = usize::from(event.row.saturating_sub(3));
                        let visible = usize::from(self.terminal_height.saturating_sub(5));
                        let offset =
                            self.activity_selected.saturating_sub(visible.saturating_sub(1));
                        self.activity_selected =
                            (offset + row).min(self.activities.len().saturating_sub(1));
                    }
                    View::Setup => {}
                }
                AppAction::None
            }
            MouseEventKind::ScrollDown => {
                self.move_selection(1);
                AppAction::None
            }
            MouseEventKind::ScrollUp => {
                self.move_selection(-1);
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    pub fn setup_jira_config(&self, board_ids: Vec<i64>) -> Result<JiraConfig, String> {
        let jira = JiraConfig {
            url: self.setup.url.trim().trim_end_matches('/').to_string(),
            auth: self.setup.auth.clone(),
            username: (self.setup.auth == JiraAuth::CloudBasicApiToken)
                .then(|| self.setup.username.trim().to_string()),
            board_ids,
            token_env: self.setup.preserved_token_env.clone(),
            token_command: self.setup.preserved_token_command.clone(),
        };
        jira.validate().map_err(|error| error.to_string())?;
        Ok(jira)
    }

    pub fn setup_config(&self) -> Result<Config, String> {
        Ok(Config {
            version: CONFIG_VERSION,
            jira: self
                .setup_jira_config(self.setup.boards.iter().map(|board| board.id).collect())?,
        })
    }

    pub fn apply_filters(&mut self) {
        self.filtered_issues = self
            .issues
            .iter()
            .enumerate()
            .filter(|(_, issue)| {
                self.search_query.as_ref().map(|query| issue.matches_query(query)).unwrap_or(true)
                    && self
                        .filter
                        .as_ref()
                        .map(|filter| filter.matches(issue, self.current_user.as_deref()))
                        .unwrap_or(true)
            })
            .map(|(index, _)| index)
            .collect();
        self.rebuild_column_cache();
        self.column_rows.fill(0);
        self.ensure_column_rows();
    }

    pub fn visible_wbs_keys(&self) -> Vec<String> {
        fn visit(
            output: &mut Vec<String>,
            nodes: &[crate::domain::wbs::WbsNode],
            expanded: &HashSet<String>,
        ) {
            for node in nodes {
                output.push(node.issue.key.clone());
                if expanded.contains(&node.issue.key) {
                    visit(output, &node.children, expanded);
                }
            }
        }
        let done = self
            .board
            .as_ref()
            .and_then(|board| board.columns.last())
            .map(|column| column.statuses.clone())
            .unwrap_or_default();
        let roots = crate::domain::wbs::build_wbs(&self.issues, &done);
        let mut keys = Vec::new();
        visit(&mut keys, &roots, &self.expanded);
        keys
    }
}

// Kept private so every list has identical clamped navigation semantics.
fn move_index(current: usize, len: usize, delta: i32) -> usize {
    if len == 0 {
        0
    } else {
        (current as i32 + delta).clamp(0, len as i32 - 1) as usize
    }
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{BoardColumn, IssueType};

    fn issue(key: &str, status: &str) -> Issue {
        Issue {
            key: key.into(),
            summary: "summary".into(),
            issue_type: IssueType::Task,
            status: status.into(),
            assignee: None,
            priority: None,
            due_date: None,
            updated: None,
            epic_key: None,
            parent_key: None,
            links: vec![],
            blocked: false,
            overdue: false,
        }
    }

    fn board() -> Board {
        Board {
            id: 1,
            name: "Board".into(),
            columns: vec![
                BoardColumn { name: "To Do".into(), statuses: vec!["To Do".into()] },
                BoardColumn { name: "Done".into(), statuses: vec!["Done".into()] },
            ],
        }
    }

    #[test]
    fn setup_accepts_t_and_q_as_text() {
        let mut state = AppState { view: View::Setup, ..Default::default() };
        state.setup.field = crate::ui::setup::SetupField::Url;
        for value in ['h', 't', 't', 'p', 's', 'q'] {
            assert_eq!(
                state.handle_key(KeyEvent::new(KeyCode::Char(value), KeyModifiers::NONE)),
                AppAction::None
            );
        }
        assert_eq!(state.setup.url, "httpsq");
    }

    #[test]
    fn setup_paste_and_token_toggle_are_unambiguous() {
        let mut state = AppState { view: View::Setup, ..Default::default() };
        state.setup.field = crate::ui::setup::SetupField::Url;
        state.handle_paste("https://jira.example.test/q\n");
        assert_eq!(state.setup.url, "https://jira.example.test/q");
        state.setup.field = crate::ui::setup::SetupField::Token;
        state.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        assert!(state.setup.show_token);
        assert!(state.setup.token.is_empty());
    }

    #[test]
    fn data_center_setup_omits_username() {
        let mut state = AppState::default();
        state.setup.auth = JiraAuth::DataCenterBearerPat;
        state.setup.url = "https://jira.example.test".into();
        state.setup.username = "must-not-be-saved".into();
        let config = state.setup_jira_config(vec![42]).unwrap();
        assert_eq!(config.username, None);
    }

    #[test]
    fn unknown_status_is_selectable_in_other_column() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do"), issue("P-2", "Custom")],
            ..Default::default()
        };
        state.ensure_column_rows();
        assert_eq!(state.column_count(), 3);
        state.selected_col = 2;
        assert_eq!(state.selected_issue().map(|issue| issue.key.as_str()), Some("P-2"));
    }

    #[test]
    fn horizontal_keys_only_move_focus() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do"), issue("P-2", "Done")],
            ..Default::default()
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert_eq!(action, AppAction::None);
        assert_eq!(state.selected_col, 1);
    }

    #[test]
    fn edit_menu_stages_status_before_any_update() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do")],
            ..Default::default()
        };
        state.apply_filters();
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)),
            AppAction::None
        );
        assert_eq!(state.modal, Modal::EditMenu);
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            AppAction::LoadTransitions
        );
    }

    #[test]
    fn escape_clears_incremental_search() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do")],
            ..Default::default()
        };
        state.apply_filters();
        state.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(state.visible_issues().is_empty());
        state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(state.search_query, None);
        assert_eq!(state.visible_issues().len(), 1);
    }
}
