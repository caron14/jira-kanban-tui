use crate::domain::{filter::BuiltInFilter, Board, Issue};
use crate::infrastructure::config::{
    normalize_jira_base_url, Config, JiraAuth, JiraConfig, CONFIG_VERSION,
};
use crate::jira::{Choice, TransitionOption, UpdateCommand};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
    None,
    Quit,
    Refresh,
    OpenIssue(String),
    OpenSetup,
    SwitchBoard(usize),
    LoadActivity,
    LoadTransitions { issue_key: String, request_id: u64 },
    LoadAssignees(String),
    LoadPriorities,
    Update { issue_key: String, command: UpdateCommand },
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
    TransitionPicker,
    AssigneePicker,
    DueDatePicker,
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
    pub wbs_roots: Vec<crate::domain::wbs::WbsNode>,
    pub wbs_visible_keys: Vec<String>,
    pub wbs_cache_signature: Option<u64>,
    pub activity_selected: usize,
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub activities: Vec<crate::domain::activity::Activity>,
    pub activity_request_id: u64,
    pub activity_loading: bool,
    pub updating_key: Option<String>,
    pub detail_issue_key: Option<String>,
    pub editing_issue_key: Option<String>,
    pub edit_request_id: u64,
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
                .wbs_visible_keys
                .get(self.wbs_selected)
                .and_then(|key| self.issues.iter().find(|issue| &issue.key == key)),
            View::Activity => self
                .activities
                .get(self.activity_selected)
                .and_then(|activity| self.issues.iter().find(|issue| issue.key == activity.key)),
            View::Setup => None,
        }
    }

    pub fn detail_issue(&self) -> Option<&Issue> {
        let key = self.detail_issue_key.as_deref()?;
        self.issues.iter().find(|issue| issue.key == key)
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
            KeyCode::Char('s') if self.network == NetworkState::AuthError => {
                return AppAction::OpenSetup
            }
            KeyCode::Char('r') if self.view == View::Activity => return AppAction::LoadActivity,
            KeyCode::Char('r') => return AppAction::Refresh,
            KeyCode::Char('b') if self.board_refs.len() > 1 && self.updating_key.is_none() => {
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
            KeyCode::Enter | KeyCode::Char('e') => {
                if let Some(issue_key) = self.selected_issue().map(|issue| issue.key.clone()) {
                    self.open_issue_detail(issue_key);
                }
            }
            KeyCode::Char('o') => {
                if let Some(issue_key) = self.selected_issue().map(|issue| issue.key.clone()) {
                    return AppAction::OpenIssue(issue_key);
                }
            }
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
                    move_index(self.wbs_selected, self.wbs_visible_keys.len(), delta)
            }
            View::Activity => {
                self.activity_selected =
                    move_index(self.activity_selected, self.activities.len(), delta)
            }
            View::Setup => {}
        }
    }

    fn open_issue_detail(&mut self, issue_key: String) {
        if self.issues.iter().any(|issue| issue.key == issue_key) {
            self.detail_issue_key = Some(issue_key);
            self.edit_index = 0;
            self.modal = Modal::Detail;
        }
    }

    fn move_horizontal(&mut self, delta: i32) {
        match self.view {
            View::Board => {
                self.selected_col = move_index(self.selected_col, self.column_count(), delta);
                self.ensure_column_rows();
            }
            View::Wbs => {
                if let Some(key) = self.wbs_visible_keys.get(self.wbs_selected).cloned() {
                    if delta < 0 {
                        self.expanded.remove(&key);
                    } else {
                        self.expanded.insert(key);
                    }
                    self.rebuild_wbs_visible_keys();
                }
            }
            _ => {}
        }
    }

    fn invalidate_edit_request(&mut self) {
        self.edit_request_id = self.edit_request_id.wrapping_add(1);
        self.status_message = None;
    }

    fn begin_detail_edit(&mut self) -> AppAction {
        if self.offline || self.updating_key.is_some() {
            return AppAction::None;
        }
        let Some(issue_key) = self.detail_issue_key.clone() else { return AppAction::None };
        let due_date = self
            .detail_issue()
            .and_then(|issue| issue.due_date)
            .map(|value| value.to_string())
            .unwrap_or_default();

        self.invalidate_edit_request();
        self.editing_issue_key = Some(issue_key.clone());
        self.error = None;
        match self.edit_index {
            0 => {
                self.transitions.clear();
                self.picker_index = 0;
                AppAction::LoadTransitions { issue_key, request_id: self.edit_request_id }
            }
            1 => {
                self.input_buffer.clear();
                self.choices.clear();
                self.picker_index = 0;
                self.modal = Modal::AssigneePicker;
                AppAction::LoadAssignees(String::new())
            }
            2 => {
                self.input_buffer = due_date;
                self.picker_index = 0;
                self.modal = Modal::DueDatePicker;
                AppAction::None
            }
            _ => {
                self.choices.clear();
                self.picker_index = 0;
                self.modal = Modal::PriorityPicker;
                AppAction::LoadPriorities
            }
        }
    }

    fn finish_edit(&mut self, command: UpdateCommand) -> AppAction {
        let Some(issue_key) = self.editing_issue_key.take() else {
            self.modal = Modal::None;
            return AppAction::None;
        };
        self.invalidate_edit_request();
        self.modal = if self.detail_issue_key.is_some() { Modal::Detail } else { Modal::None };
        AppAction::Update { issue_key, command }
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> AppAction {
        if key.code == KeyCode::Esc {
            let closing = self.modal;
            if closing == Modal::Search {
                self.search_query = None;
                self.apply_filters();
            }
            if closing == Modal::DueDateEditor && self.detail_issue_key.is_some() {
                self.modal = Modal::DueDatePicker;
                self.error = None;
                return AppAction::None;
            }
            let return_to_detail = self.detail_issue_key.is_some()
                && matches!(
                    closing,
                    Modal::TransitionPicker
                        | Modal::AssigneePicker
                        | Modal::DueDatePicker
                        | Modal::DueDateEditor
                        | Modal::PriorityPicker
                        | Modal::Error
                );
            self.modal = if return_to_detail { Modal::Detail } else { Modal::None };
            if !return_to_detail {
                self.detail_issue_key = None;
            }
            self.error = None;
            self.input_buffer.clear();
            self.editing_issue_key = None;
            self.invalidate_edit_request();
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
                KeyCode::Char('j') | KeyCode::Down => {
                    self.edit_index = move_index(self.edit_index, 4, 1);
                    self.editing_issue_key = None;
                    self.invalidate_edit_request();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.edit_index = move_index(self.edit_index, 4, -1);
                    self.editing_issue_key = None;
                    self.invalidate_edit_request();
                }
                KeyCode::Enter => return self.begin_detail_edit(),
                KeyCode::Char('o') => {
                    if let Some(issue_key) = self.detail_issue_key.clone() {
                        return AppAction::OpenIssue(issue_key);
                    }
                }
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
                        return self.finish_edit(command);
                    }
                }
                _ => {}
            },
            Modal::AssigneePicker => match key.code {
                KeyCode::Up => {
                    let len = self.choices.len() + 1 + usize::from(self.current_user.is_some());
                    self.picker_index = move_index(self.picker_index, len, -1)
                }
                KeyCode::Down => {
                    let len = self.choices.len() + 1 + usize::from(self.current_user.is_some());
                    self.picker_index = move_index(self.picker_index, len, 1)
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                    return AppAction::LoadAssignees(self.input_buffer.clone());
                }
                KeyCode::Delete => {
                    return self.finish_edit(UpdateCommand::Assignee { account_id: None });
                }
                KeyCode::Enter => {
                    if self.picker_index == 0 {
                        if let Some(account_id) = self.current_user.clone() {
                            return self.finish_edit(UpdateCommand::Assignee {
                                account_id: Some(account_id),
                            });
                        }
                        return self.finish_edit(UpdateCommand::Assignee { account_id: None });
                    }
                    let unassign_index = usize::from(self.current_user.is_some());
                    if self.picker_index == unassign_index {
                        return self.finish_edit(UpdateCommand::Assignee { account_id: None });
                    }
                    let choice_index = self.picker_index - unassign_index - 1;
                    if let Some(choice) = self.choices.get(choice_index) {
                        let command =
                            UpdateCommand::Assignee { account_id: Some(choice.id.clone()) };
                        return self.finish_edit(command);
                    }
                }
                KeyCode::Char(c) => {
                    self.input_buffer.push(c);
                    return AppAction::LoadAssignees(self.input_buffer.clone());
                }
                _ => {}
            },
            Modal::DueDatePicker => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.picker_index = move_index(self.picker_index, 5, 1)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.picker_index = move_index(self.picker_index, 5, -1)
                }
                KeyCode::Enter => {
                    let today = chrono::Local::now().date_naive();
                    let value = match self.picker_index {
                        0 => Some(today),
                        1 => Some(today + chrono::Duration::days(1)),
                        2 => Some(today + chrono::Duration::days(7)),
                        3 => None,
                        _ => {
                            self.modal = Modal::DueDateEditor;
                            return AppAction::None;
                        }
                    };
                    return self.finish_edit(UpdateCommand::DueDate { value });
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
                        return self.finish_edit(command);
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
                    return self.finish_edit(UpdateCommand::DueDate { value });
                }
                _ => {}
            },
            Modal::Error => match key.code {
                KeyCode::Char('r') if self.retry_action.is_some() => {
                    self.modal = Modal::None;
                    self.detail_issue_key = None;
                    self.error = None;
                    return self.retry_action.clone().unwrap_or(AppAction::None);
                }
                KeyCode::Char('o') => {
                    if let Some(issue_key) = self
                        .detail_issue_key
                        .clone()
                        .or_else(|| self.selected_issue().map(|issue| issue.key.clone()))
                    {
                        return AppAction::OpenIssue(issue_key);
                    }
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
            KeyCode::Esc if self.setup.step == SetupStep::Boards => {
                self.setup.step = SetupStep::Connection;
                self.setup.field = SetupField::Token;
                self.setup.message = None;
            }
            KeyCode::Esc => self.setup.confirm_quit = true,
            KeyCode::Tab => self.setup.move_field(1),
            KeyCode::BackTab => self.setup.move_field(-1),
            KeyCode::Up if self.setup.step == SetupStep::Boards => {
                self.setup.board_index =
                    move_index(self.setup.board_index, self.setup.matching_boards().len(), -1);
            }
            KeyCode::Down if self.setup.step == SetupStep::Boards => {
                self.setup.board_index =
                    move_index(self.setup.board_index, self.setup.matching_boards().len(), 1);
            }
            KeyCode::Up => self.setup.move_field(-1),
            KeyCode::Down => self.setup.move_field(1),
            KeyCode::Left | KeyCode::Right if self.setup.field == SetupField::Auth => {
                self.setup.auth_explicit = true;
                self.setup.auth = match self.setup.auth {
                    JiraAuth::CloudBasicApiToken => JiraAuth::DataCenterBearerPat,
                    JiraAuth::DataCenterBearerPat => JiraAuth::CloudBasicApiToken,
                };
            }
            KeyCode::Left | KeyCode::Right if self.setup.field == SetupField::AllowInsecureHttp => {
                self.setup.allow_insecure_http = !self.setup.allow_insecure_http;
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
            KeyCode::Enter => return self.activate_setup_primary(),
            KeyCode::Char(' ') if self.setup.step == SetupStep::Boards => {
                self.setup.toggle_current_board();
            }
            KeyCode::Delete if self.setup.step == SetupStep::Boards => {
                self.setup.boards.pop();
            }
            KeyCode::Backspace => {
                let url_field = self.setup.field == SetupField::Url;
                if let Some(value) = self.setup.current_value_mut() {
                    value.pop();
                }
                if url_field {
                    self.setup.detect_auth_from_url();
                }
                if self.setup.step == SetupStep::Boards {
                    self.setup.board_index = 0;
                }
            }
            KeyCode::Char(c) => {
                let board_field = self.setup.field == SetupField::BoardId;
                let url_field = self.setup.field == SetupField::Url;
                if let Some(value) = self.setup.current_value_mut() {
                    value.push(c);
                }
                if url_field {
                    self.setup.detect_auth_from_url();
                }
                if board_field {
                    self.setup.board_index = 0;
                }
            }
            _ => {}
        }
        AppAction::None
    }

    fn activate_setup_primary(&mut self) -> AppAction {
        use crate::ui::setup::SetupStep;
        if self.setup.step == SetupStep::Connection {
            return AppAction::TestSetupConnection;
        }
        if self.setup.available_boards.is_empty()
            && self.setup.board_input.is_empty()
            && !self.setup.boards.is_empty()
        {
            return AppAction::SaveSetup;
        }
        if !self.setup.matching_boards().is_empty() {
            if self.setup.boards.is_empty() {
                self.setup.toggle_current_board();
            }
            return AppAction::SaveSetup;
        }
        match self.setup.board_input.parse::<i64>() {
            Ok(id) if id > 0 => AppAction::AddSetupBoard(id),
            _ => {
                self.setup.message = Some("No matching Board; enter a positive Board ID".into());
                AppAction::None
            }
        }
    }

    pub fn handle_paste(&mut self, value: &str) -> AppAction {
        if self.view == View::Setup {
            let url_field = self.setup.field == crate::ui::setup::SetupField::Url;
            if let Some(target) = self.setup.current_value_mut() {
                target.extend(value.chars().filter(|character| !matches!(character, '\r' | '\n')));
            }
            if url_field {
                self.setup.detect_auth_from_url();
            }
            if self.setup.field == crate::ui::setup::SetupField::BoardId {
                self.setup.board_index = 0;
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
        if self.modal != Modal::None {
            return self.handle_modal_mouse(event);
        }
        if self.view == View::Setup {
            if self.terminal_width < 80 || self.terminal_height < 24 {
                return AppAction::None;
            }
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let area =
                        ratatui::layout::Rect::new(0, 0, self.terminal_width, self.terminal_height);
                    match crate::ui::setup::hit_test(area, &self.setup, event.column, event.row) {
                        Some(crate::ui::setup::SetupHit::Field(field)) => {
                            self.setup.field = field;
                            match field {
                                crate::ui::setup::SetupField::Auth => {
                                    self.setup.auth_explicit = true;
                                    self.setup.auth = match self.setup.auth {
                                        JiraAuth::CloudBasicApiToken => {
                                            JiraAuth::DataCenterBearerPat
                                        }
                                        JiraAuth::DataCenterBearerPat => {
                                            JiraAuth::CloudBasicApiToken
                                        }
                                    };
                                }
                                crate::ui::setup::SetupField::AllowInsecureHttp => {
                                    self.setup.allow_insecure_http =
                                        !self.setup.allow_insecure_http;
                                }
                                _ => {}
                            }
                        }
                        Some(crate::ui::setup::SetupHit::Board(index)) => {
                            self.setup.board_index = index;
                            self.setup.toggle_current_board();
                        }
                        Some(crate::ui::setup::SetupHit::PrimaryAction) => {
                            return self.activate_setup_primary();
                        }
                        None => {}
                    }
                }
                MouseEventKind::ScrollDown
                    if self.setup.step == crate::ui::setup::SetupStep::Boards =>
                {
                    self.setup.board_index =
                        move_index(self.setup.board_index, self.setup.matching_boards().len(), 1);
                }
                MouseEventKind::ScrollUp
                    if self.setup.step == crate::ui::setup::SetupStep::Boards =>
                {
                    self.setup.board_index =
                        move_index(self.setup.board_index, self.setup.matching_boards().len(), -1);
                }
                _ => {}
            }
            return AppAction::None;
        }
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let area =
                    ratatui::layout::Rect::new(0, 0, self.terminal_width, self.terminal_height);
                let sections = crate::ui::layout::AppSections::new(area);
                if let Some(view) =
                    crate::ui::header_hit_test(sections.header, event.column, event.row)
                {
                    self.view = view;
                    return if view == View::Activity {
                        AppAction::LoadActivity
                    } else {
                        AppAction::None
                    };
                }
                match self.view {
                    View::Board => {
                        if let Some((column, row)) = crate::ui::board::hit_test(
                            sections.content,
                            self,
                            event.column,
                            event.row,
                        ) {
                            self.selected_col = column;
                            self.column_rows[column] = row;
                            if let Some(issue_key) =
                                self.selected_issue().map(|issue| issue.key.clone())
                            {
                                self.open_issue_detail(issue_key);
                            }
                        }
                    }
                    View::Dashboard => {
                        let attention = self.attention_items();
                        if let Some(index) = crate::ui::dashboard::hit_test(
                            sections.content,
                            attention.len(),
                            self.dashboard_selected,
                            event.column,
                            event.row,
                        ) {
                            let issue_key = attention[index].issue.key.clone();
                            self.dashboard_selected = index;
                            self.open_issue_detail(issue_key);
                        }
                    }
                    View::Wbs => {
                        if let Some(index) = crate::ui::wbs::hit_test(
                            sections.content,
                            self.wbs_visible_keys.len(),
                            self.wbs_selected,
                            event.column,
                            event.row,
                        ) {
                            let issue_key = self.wbs_visible_keys[index].clone();
                            self.wbs_selected = index;
                            self.open_issue_detail(issue_key);
                        }
                    }
                    View::Activity => {
                        if let Some(index) = crate::ui::activity::hit_test(
                            sections.content,
                            self.activities.len(),
                            self.activity_selected,
                            event.column,
                            event.row,
                        ) {
                            let issue_key = self.activities[index].key.clone();
                            self.activity_selected = index;
                            self.open_issue_detail(issue_key);
                        }
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

    fn handle_modal_mouse(&mut self, event: MouseEvent) -> AppAction {
        use crossterm::event::{MouseButton, MouseEventKind};
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        match event.kind {
            MouseEventKind::ScrollDown => self.handle_modal_key(key(KeyCode::Down)),
            MouseEventKind::ScrollUp => self.handle_modal_key(key(KeyCode::Up)),
            MouseEventKind::Down(MouseButton::Left) => {
                let area =
                    ratatui::layout::Rect::new(0, 0, self.terminal_width, self.terminal_height);
                match crate::ui::modal_hit_test(area, self, event.column, event.row) {
                    Some(crate::ui::ModalHit::DetailField(index)) => {
                        self.edit_index = index;
                        self.editing_issue_key = None;
                        self.invalidate_edit_request();
                        self.begin_detail_edit()
                    }
                    Some(crate::ui::ModalHit::ListItem(index)) => {
                        if self.modal == Modal::Filter {
                            self.filter_index = index;
                        } else {
                            self.picker_index = index;
                        }
                        self.handle_modal_key(key(KeyCode::Enter))
                    }
                    Some(crate::ui::ModalHit::Outside) => self.handle_modal_key(key(KeyCode::Esc)),
                    None => AppAction::None,
                }
            }
            _ => AppAction::None,
        }
    }

    pub fn setup_jira_config(&self, board_ids: Vec<i64>) -> Result<JiraConfig, String> {
        let url = normalize_jira_base_url(&self.setup.url).map_err(|error| error.to_string())?;
        let jira = JiraConfig {
            allow_insecure_http: self.setup.allow_insecure_http && url.starts_with("http://"),
            url,
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
        self.rebuild_wbs_cache();
        self.column_rows.fill(0);
        self.ensure_column_rows();
    }

    fn rebuild_wbs_cache(&mut self) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for issue in &self.issues {
            issue.key.hash(&mut hasher);
            issue.summary.hash(&mut hasher);
            issue.status.hash(&mut hasher);
            issue.parent_key.hash(&mut hasher);
            issue.epic_key.hash(&mut hasher);
        }
        let done = self
            .board
            .as_ref()
            .and_then(|board| board.columns.last())
            .map(|column| column.statuses.clone())
            .unwrap_or_default();
        done.hash(&mut hasher);
        let signature = hasher.finish();
        if self.wbs_cache_signature == Some(signature) {
            return;
        }
        self.wbs_cache_signature = Some(signature);
        self.wbs_roots = crate::domain::wbs::build_wbs(&self.issues, &done);
        self.rebuild_wbs_visible_keys();
        self.wbs_selected = self.wbs_selected.min(self.wbs_visible_keys.len().saturating_sub(1));
    }

    fn rebuild_wbs_visible_keys(&mut self) {
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
        let mut keys = Vec::new();
        visit(&mut keys, &self.wbs_roots, &self.expanded);
        self.wbs_visible_keys = keys;
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
    fn setup_detects_jira_type_from_url_until_user_overrides_it() {
        let mut cloud = AppState { view: View::Setup, ..Default::default() };
        cloud.setup.field = crate::ui::setup::SetupField::Url;
        cloud.handle_paste("https://team.atlassian.net/jira/software/c/projects/P/boards/1");
        assert_eq!(cloud.setup.auth, JiraAuth::CloudBasicApiToken);

        let mut data_center = AppState { view: View::Setup, ..Default::default() };
        data_center.setup.field = crate::ui::setup::SetupField::Url;
        data_center.handle_paste("https://jira.internal.test/jira");
        assert_eq!(data_center.setup.auth, JiraAuth::DataCenterBearerPat);

        data_center.setup.field = crate::ui::setup::SetupField::Auth;
        data_center.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(data_center.setup.auth_explicit);
        assert_eq!(data_center.setup.auth, JiraAuth::CloudBasicApiToken);
        data_center.setup.field = crate::ui::setup::SetupField::Url;
        data_center.setup.url.clear();
        data_center.handle_paste("https://another.internal.test");
        assert_eq!(data_center.setup.auth, JiraAuth::CloudBasicApiToken);
    }

    #[test]
    fn setup_uses_arrows_for_fields_and_enter_to_save_verified_boards() {
        let mut state = AppState { view: View::Setup, ..Default::default() };
        assert_eq!(state.setup.field, crate::ui::setup::SetupField::Auth);
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.setup.field, crate::ui::setup::SetupField::Url);

        state.setup.step = crate::ui::setup::SetupStep::Boards;
        state.setup.field = crate::ui::setup::SetupField::BoardId;
        state.setup.boards.push(crate::ui::setup::SetupBoard { id: 42, name: "Board".into() });
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            AppAction::SaveSetup
        );
    }

    #[test]
    fn setup_selects_discovered_board_by_name_without_numeric_id() {
        let mut state = AppState { view: View::Setup, ..Default::default() };
        state.setup.step = crate::ui::setup::SetupStep::Boards;
        state.setup.field = crate::ui::setup::SetupField::BoardId;
        state.setup.available_boards = vec![
            crate::ui::setup::SetupBoard { id: 1, name: "Alpha".into() },
            crate::ui::setup::SetupBoard { id: 2, name: "Beta".into() },
        ];

        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(action, AppAction::SaveSetup);
        assert_eq!(
            state.setup.boards,
            vec![crate::ui::setup::SetupBoard { id: 2, name: "Beta".into() }]
        );
    }

    #[test]
    fn escape_from_board_setup_returns_to_connection_without_discarding_boards() {
        let mut state = AppState { view: View::Setup, ..Default::default() };
        state.setup.step = crate::ui::setup::SetupStep::Boards;
        state.setup.field = crate::ui::setup::SetupField::BoardId;
        state.setup.boards.push(crate::ui::setup::SetupBoard { id: 42, name: "Board".into() });

        state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert_eq!(state.setup.step, crate::ui::setup::SetupStep::Connection);
        assert!(!state.setup.confirm_quit);
        assert_eq!(state.setup.boards.len(), 1);
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
    fn setup_requires_explicit_confirmation_for_insecure_http() {
        let mut state = AppState::default();
        state.setup.auth = JiraAuth::DataCenterBearerPat;
        state.setup.url = "http://jira.internal.test".into();

        assert!(state.setup_jira_config(vec![42]).is_err());
        state.setup.allow_insecure_http = true;
        assert!(state.setup_jira_config(vec![42]).is_ok());
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
    fn issue_detail_stages_status_before_any_update() {
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
        assert_eq!(state.modal, Modal::Detail);
        assert_eq!(state.detail_issue_key.as_deref(), Some("P-1"));
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            action,
            AppAction::LoadTransitions { issue_key, .. } if issue_key == "P-1"
        ));
    }

    #[test]
    fn issue_detail_keeps_original_issue_when_background_selection_changes() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do"), issue("P-2", "To Do")],
            ..Default::default()
        };
        state.apply_filters();
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        state.column_rows[0] = 1;

        assert_eq!(state.selected_issue().map(|issue| issue.key.as_str()), Some("P-2"));
        assert_eq!(state.detail_issue().map(|issue| issue.key.as_str()), Some("P-1"));
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
            AppAction::OpenIssue("P-1".into())
        );
    }

    #[test]
    fn due_date_picker_offers_custom_editor_with_existing_value() {
        let mut dated = issue("P-1", "To Do");
        dated.due_date = chrono::NaiveDate::from_ymd_opt(2026, 9, 30);
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![dated],
            ..Default::default()
        };
        state.apply_filters();
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            AppAction::None
        );
        assert_eq!(state.modal, Modal::DueDatePicker);
        for _ in 0..4 {
            state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(state.modal, Modal::DueDateEditor);
        assert_eq!(state.input_buffer, "2026-09-30");
    }

    #[test]
    fn assignee_picker_puts_assign_to_me_before_search_results() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do")],
            current_user: Some("account-42".into()),
            ..Default::default()
        };
        state.apply_filters();
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            AppAction::LoadAssignees(String::new())
        );

        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            AppAction::Update {
                issue_key: "P-1".into(),
                command: UpdateCommand::Assignee { account_id: Some("account-42".into()) }
            }
        );
        assert_eq!(state.modal, Modal::Detail);
    }

    #[test]
    fn assignee_picker_can_unassign_without_searching() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do")],
            current_user: Some("account-42".into()),
            ..Default::default()
        };
        state.apply_filters();
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            AppAction::Update {
                issue_key: "P-1".into(),
                command: UpdateCommand::Assignee { account_id: None }
            }
        );
    }

    #[test]
    fn due_date_picker_can_clear_date_without_text_input() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do")],
            ..Default::default()
        };
        state.apply_filters();
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        for _ in 0..3 {
            state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }

        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            AppAction::Update {
                issue_key: "P-1".into(),
                command: UpdateCommand::DueDate { value: None }
            }
        );
    }

    #[test]
    fn mouse_can_clear_due_date_directly_from_issue_detail() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do")],
            terminal_width: 100,
            terminal_height: 30,
            ..Default::default()
        };
        state.apply_filters();
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let click = |column, row| crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };

        assert_eq!(state.handle_mouse(click(12, 11)), AppAction::None);
        assert_eq!(state.modal, Modal::DueDatePicker);
        assert_eq!(
            state.handle_mouse(click(20, 15)),
            AppAction::Update {
                issue_key: "P-1".into(),
                command: UpdateCommand::DueDate { value: None }
            }
        );
        assert_eq!(state.modal, Modal::Detail);
    }

    #[test]
    fn clicking_outside_issue_detail_closes_it() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do")],
            terminal_width: 100,
            terminal_height: 30,
            ..Default::default()
        };
        state.apply_filters();
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        state.handle_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(state.modal, Modal::None);
        assert_eq!(state.detail_issue_key, None);
    }

    #[test]
    fn clicking_board_card_opens_its_issue_detail() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do")],
            terminal_width: 100,
            terminal_height: 30,
            ..Default::default()
        };
        state.apply_filters();

        state.handle_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 2,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(state.modal, Modal::Detail);
        assert_eq!(state.detail_issue_key.as_deref(), Some("P-1"));
    }

    #[test]
    fn rendered_header_regions_switch_views() {
        let mut state = AppState {
            view: View::Board,
            terminal_width: 100,
            terminal_height: 30,
            ..Default::default()
        };
        let click = |column, row| crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };

        state.handle_mouse(click(10, 1));
        assert_eq!(state.view, View::Dashboard);
        state.view = View::Board;
        state.handle_mouse(click(80, 1));
        assert_eq!(state.view, View::Board);
    }

    #[test]
    fn rendered_list_regions_open_details_in_every_list_view() {
        let click = |column, row| crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        let state = || AppState {
            board: Some(board()),
            issues: vec![issue("P-1", "To Do")],
            terminal_width: 100,
            terminal_height: 30,
            ..Default::default()
        };

        let mut dashboard = state();
        dashboard.view = View::Dashboard;
        dashboard.apply_filters();
        dashboard.handle_mouse(click(2, 9));
        assert_eq!(dashboard.detail_issue_key.as_deref(), Some("P-1"));

        let mut wbs = state();
        wbs.view = View::Wbs;
        wbs.apply_filters();
        wbs.handle_mouse(click(2, 3));
        assert_eq!(wbs.detail_issue_key.as_deref(), Some("P-1"));

        let mut activity = state();
        activity.view = View::Activity;
        activity.activities = vec![crate::domain::activity::Activity {
            key: "P-1".into(),
            summary: "summary".into(),
            kind: crate::domain::activity::ChangeKind::Status,
            from: Some("To Do".into()),
            to: Some("Done".into()),
            at: chrono::Utc::now(),
        }];
        activity.apply_filters();
        activity.handle_mouse(click(2, 3));
        assert_eq!(activity.detail_issue_key.as_deref(), Some("P-1"));
    }

    #[test]
    fn edit_action_keeps_original_issue_key_during_mouse_input() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do"), issue("P-2", "To Do")],
            ..Default::default()
        };
        state.apply_filters();
        state.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

        let mouse_action = state.handle_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollDown,
            column: 1,
            row: 4,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(mouse_action, AppAction::None);
        assert_eq!(state.selected_issue().map(|issue| issue.key.as_str()), Some("P-1"));

        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        for _ in 0..4 {
            state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        state.input_buffer = "2026-09-30".into();
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            action,
            AppAction::Update {
                issue_key,
                command: UpdateCommand::DueDate { .. }
            } if issue_key == "P-1"
        ));
    }

    #[test]
    fn escaping_edit_invalidates_pending_transition_request() {
        let mut state = AppState {
            view: View::Board,
            board: Some(board()),
            issues: vec![issue("P-1", "To Do")],
            ..Default::default()
        };
        state.apply_filters();
        state.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let AppAction::LoadTransitions { request_id, .. } = action else {
            panic!("expected transition request");
        };

        state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(state.editing_issue_key, None);
        assert_ne!(state.edit_request_id, request_id);
    }

    #[test]
    fn activity_refresh_requests_activity_data() {
        let mut state = AppState { view: View::Activity, ..Default::default() };
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
            AppAction::LoadActivity
        );
    }

    #[test]
    fn authentication_error_can_open_setup_after_error_modal_is_closed() {
        let mut state = AppState {
            view: View::Board,
            network: NetworkState::AuthError,
            offline: true,
            ..Default::default()
        };
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
            AppAction::OpenSetup
        );
    }

    #[test]
    fn setup_ignores_non_semantic_mouse_regions() {
        let mut state = AppState { view: View::Setup, ..Default::default() };
        let action = state.handle_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(action, AppAction::None);
        assert_eq!(state.view, View::Setup);
    }

    #[test]
    fn setup_mouse_focuses_fields_and_runs_primary_action() {
        let mut state = AppState {
            view: View::Setup,
            terminal_width: 80,
            terminal_height: 24,
            ..Default::default()
        };
        let click = |column, row| crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };

        state.handle_mouse(click(5, 4));
        assert_eq!(state.setup.field, crate::ui::setup::SetupField::Auth);
        assert_eq!(state.setup.auth, JiraAuth::DataCenterBearerPat);
        state.handle_mouse(click(5, 7));
        assert_eq!(state.setup.field, crate::ui::setup::SetupField::Url);
        assert_eq!(state.handle_mouse(click(5, 21)), AppAction::TestSetupConnection);
    }

    #[test]
    fn setup_mouse_selects_and_saves_a_discovered_board() {
        let mut state = AppState {
            view: View::Setup,
            terminal_width: 80,
            terminal_height: 24,
            ..Default::default()
        };
        state.setup.step = crate::ui::setup::SetupStep::Boards;
        state.setup.field = crate::ui::setup::SetupField::BoardId;
        state.setup.available_boards =
            vec![crate::ui::setup::SetupBoard { id: 42, name: "Team Board".into() }];
        let click = |column, row| crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };

        assert_eq!(state.handle_mouse(click(5, 7)), AppAction::None);
        assert_eq!(state.setup.boards[0].id, 42);
        assert_eq!(state.handle_mouse(click(5, 21)), AppAction::SaveSetup);
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
