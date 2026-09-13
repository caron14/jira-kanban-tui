pub mod state;

use anyhow::Result;
use crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, sync::Arc, time::Duration};
use tokio::sync::mpsc;

use crate::{
    domain::{activity::Activity, Board, Issue},
    infrastructure::{cli::Cli, config::Config, token},
    jira::{Choice, JiraService, TransitionOption},
};
pub use state::AppState;
use state::{AppAction, Modal, NetworkState, View};

type SetupBoardResults = Vec<(i64, Result<Board, String>)>;
type SetupConnectionResult = Result<(Choice, Vec<Choice>, SetupBoardResults), String>;

struct TerminalSession;
impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(error) =
            execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)
        {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            return Err(error.into());
        }
        Ok(Self)
    }
}
impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}

enum RuntimeResult {
    Loaded {
        generation: u64,
        result: Result<(Board, Vec<Issue>), crate::jira::JiraError>,
    },
    Updated {
        key: String,
        result: Result<Issue, crate::jira::JiraError>,
    },
    Transitions {
        issue_key: String,
        request_id: u64,
        result: Result<Vec<TransitionOption>, crate::jira::JiraError>,
    },
    Choices(Result<Vec<Choice>, crate::jira::JiraError>),
    AssigneeChoices {
        query: String,
        result: Result<Vec<Choice>, crate::jira::JiraError>,
    },
    Activity {
        board_ref: String,
        request_id: u64,
        result: Result<Vec<Activity>, crate::jira::JiraError>,
    },
    Viewer(Result<Choice, crate::jira::JiraError>),
    BoardNames(Vec<(usize, Result<Board, crate::jira::JiraError>)>),
    SetupConnection(SetupConnectionResult),
    SetupBoard {
        id: i64,
        result: Result<Board, String>,
    },
    SetupSaved(Result<(Config, JiraService, Board, Vec<Issue>), String>),
}

pub async fn run(cli: Cli, config: Option<Config>, config_error: Option<String>) -> Result<()> {
    let _session = TerminalSession::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    let mut state = AppState::default();
    let size = terminal.size()?;
    state.terminal_width = size.width;
    state.terminal_height = size.height;
    let config_path = Config::path(&cli);
    let mut current_config = config;
    let mut service: Option<Arc<JiraService>> = None;

    if let Some(config) = &current_config {
        state.board_refs = config.jira.board_refs();
        state.board_names = state.board_refs.clone();
        match initialise_service(config) {
            Ok(value) => {
                service = Some(value);
                state.loading = true;
            }
            Err(error) => {
                prepare_setup(&mut state, config, error);
            }
        }
    } else {
        state.view = View::Setup;
        state.setup.message = config_error;
    }

    terminal.draw(|frame| crate::ui::render(frame, &state))?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    if let Some(active) = service.clone() {
        load_cache(&mut state, current_config.as_ref());
        schedule_load(&mut state, active.clone(), tx.clone());
        schedule_viewer(active.clone(), tx.clone());
        schedule_board_names(active, tx.clone());
    }

    let mut dirty = false;
    loop {
        while let Ok(message) = rx.try_recv() {
            if let Some(action) =
                handle_result(message, &mut state, &mut service, &mut current_config)
            {
                if process_action(
                    action,
                    &mut state,
                    &mut service,
                    &mut current_config,
                    &config_path,
                    &tx,
                ) {
                    return Ok(());
                }
            }
            dirty = true;
        }

        if crossterm::event::poll(Duration::from_millis(50))? {
            let action = match crossterm::event::read()? {
                crossterm::event::Event::Key(key) => state.handle_key(key),
                crossterm::event::Event::Mouse(mouse) => state.handle_mouse(mouse),
                crossterm::event::Event::Paste(value) => state.handle_paste(&value),
                crossterm::event::Event::Resize(width, height) => {
                    state.terminal_width = width;
                    state.terminal_height = height;
                    AppAction::None
                }
                _ => AppAction::None,
            };
            if process_action(
                action,
                &mut state,
                &mut service,
                &mut current_config,
                &config_path,
                &tx,
            ) {
                break;
            }
            dirty = true;
        }

        if dirty {
            terminal.draw(|frame| crate::ui::render(frame, &state))?;
            dirty = false;
        }
    }
    Ok(())
}

fn initialise_service(config: &Config) -> Result<Arc<JiraService>, String> {
    let providers = token::build_providers(&config.jira);
    let (_, secret) = token::resolve_token(&providers)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No credential found. Enter the API Token to repair Setup.".to_string())?;
    JiraService::new(&config.jira, secret).map(Arc::new).map_err(|error| error.to_string())
}

fn prepare_setup(state: &mut AppState, config: &Config, error: String) {
    state.view = View::Setup;
    state.setup.auth = config.jira.auth.clone();
    state.setup.auth_explicit = true;
    state.setup.url = config.jira.url.clone();
    state.setup.allow_insecure_http = config.jira.allow_insecure_http;
    state.setup.username = config.jira.username.clone().unwrap_or_default();
    state.setup.preserved_board_ids = config.jira.board_ids.clone();
    state.setup.preserved_token_env = config.jira.token_env.clone();
    state.setup.preserved_token_command = config.jira.token_command.clone();
    state.setup.message = Some(error);
}

fn schedule_load(
    state: &mut AppState,
    service: Arc<JiraService>,
    tx: mpsc::UnboundedSender<RuntimeResult>,
) {
    let Some(board_ref) = state.current_board_ref().map(str::to_owned) else { return };
    state.request_generation = state.request_generation.wrapping_add(1);
    let generation = state.request_generation;
    tokio::spawn(async move {
        let result = service.load_board_and_issues(&board_ref).await;
        let _ = tx.send(RuntimeResult::Loaded { generation, result });
    });
}

fn schedule_viewer(service: Arc<JiraService>, tx: mpsc::UnboundedSender<RuntimeResult>) {
    tokio::spawn(async move {
        let _ = tx.send(RuntimeResult::Viewer(service.viewer().await));
    });
}

fn schedule_board_names(service: Arc<JiraService>, tx: mpsc::UnboundedSender<RuntimeResult>) {
    tokio::spawn(async move {
        let mut results = Vec::new();
        for (index, board_ref) in service.board_refs().into_iter().enumerate() {
            results.push((index, service.inspect_board(&board_ref).await));
        }
        let _ = tx.send(RuntimeResult::BoardNames(results));
    });
}

fn schedule_activity(
    state: &mut AppState,
    service: Arc<JiraService>,
    tx: mpsc::UnboundedSender<RuntimeResult>,
) {
    let Some(board_ref) = state.current_board_ref().map(str::to_owned) else { return };
    state.activity_request_id = state.activity_request_id.wrapping_add(1);
    state.activity_loading = true;
    let request_id = state.activity_request_id;
    tokio::spawn(async move {
        let since = chrono::Utc::now() - chrono::Duration::days(1);
        let result = service.activity(&board_ref, since).await;
        let _ = tx.send(RuntimeResult::Activity { board_ref, request_id, result });
    });
}

fn process_action(
    action: AppAction,
    state: &mut AppState,
    service: &mut Option<Arc<JiraService>>,
    config: &mut Option<Config>,
    config_path: &std::path::Path,
    tx: &mpsc::UnboundedSender<RuntimeResult>,
) -> bool {
    match action {
        AppAction::None => {}
        AppAction::Quit => return true,
        AppAction::Refresh => {
            if state.refreshing || state.loading {
                return false;
            }
            if let Some(active) = service.clone() {
                state.refreshing = true;
                state.network = NetworkState::Refreshing;
                schedule_load(state, active, tx.clone());
            }
        }
        AppAction::OpenIssue(issue_key) => {
            if let Some(active) = service.as_ref() {
                if let Err(error) = open::that(active.issue_url(&issue_key)) {
                    show_error(state, error.to_string(), None);
                }
            }
        }
        AppAction::OpenSetup => {
            if let Some(config) = config.as_ref() {
                prepare_setup(
                    state,
                    config,
                    "Enter a valid API Token and verify the account".into(),
                );
            }
        }
        AppAction::SwitchBoard(index) => {
            if index < state.board_refs.len() && index != state.board_ref_index {
                state.activity_request_id = state.activity_request_id.wrapping_add(1);
                state.activity_loading = false;
                state.board_ref_index = index;
                state.board = None;
                state.issues.clear();
                state.activities.clear();
                state.selected_col = 0;
                state.column_rows.clear();
                state.dashboard_selected = 0;
                state.wbs_selected = 0;
                state.activity_selected = 0;
                load_cache(state, config.as_ref());
                if let Some(active) = service.clone() {
                    state.loading = true;
                    schedule_load(state, active.clone(), tx.clone());
                    if state.view == View::Activity {
                        schedule_activity(state, active, tx.clone());
                    }
                }
            }
        }
        AppAction::LoadActivity => {
            if let Some(active) = service.clone() {
                schedule_activity(state, active, tx.clone());
            }
        }
        AppAction::LoadTransitions { issue_key, request_id } => {
            if state.offline {
                show_error(
                    state,
                    "Cached data is read-only. Refresh before editing.".into(),
                    Some(AppAction::Refresh),
                );
                return false;
            }
            if let Some(active) = service.clone() {
                let sender = tx.clone();
                tokio::spawn(async move {
                    let result = active.transitions(&issue_key).await;
                    let _ =
                        sender.send(RuntimeResult::Transitions { issue_key, request_id, result });
                });
                state.status_message = Some("Loading Status choices…".into());
            }
        }
        AppAction::LoadAssignees(query) => {
            if let Some(active) = service.clone() {
                let sender = tx.clone();
                tokio::spawn(async move {
                    let result = active.assignees(&query).await;
                    let _ = sender.send(RuntimeResult::AssigneeChoices { query, result });
                });
            }
        }
        AppAction::LoadPriorities => {
            if let Some(active) = service.clone() {
                let sender = tx.clone();
                tokio::spawn(async move {
                    let _ = sender.send(RuntimeResult::Choices(active.priorities().await));
                });
            }
        }
        AppAction::Update { issue_key, command } => {
            if state.offline {
                show_error(
                    state,
                    "Cached data is read-only. Refresh before editing.".into(),
                    Some(AppAction::Refresh),
                );
                return false;
            }
            let Some(active) = service.clone() else { return false };
            if state.updating_key.is_some() {
                return false;
            }
            if !state.issues.iter().any(|issue| issue.key == issue_key) {
                show_error(
                    state,
                    format!("Issue {issue_key} is no longer on the selected Board"),
                    Some(AppAction::Refresh),
                );
                return false;
            }
            let key = issue_key;
            let label = command.label();
            state.updating_key = Some(key.clone());
            state.status_message = Some(format!("Updating {label} for {key}…"));
            let sender = tx.clone();
            tokio::spawn(async move {
                let result = match active.update(&key, command).await {
                    Ok(()) => active.get_issue(&key).await,
                    Err(error) => Err(error),
                };
                let _ = sender.send(RuntimeResult::Updated { key, result });
            });
        }
        AppAction::TestSetupConnection => {
            let jira = match state.setup_jira_config(vec![1]) {
                Ok(jira) => jira,
                Err(error) => {
                    state.setup.message = Some(error);
                    return false;
                }
            };
            let secret = state.setup.token.clone();
            let preserved = if state.setup.boards.is_empty() {
                state.setup.preserved_board_ids.clone()
            } else {
                state.setup.boards.iter().map(|board| board.id).collect()
            };
            state.setup.busy = true;
            state.setup.message = None;
            let sender = tx.clone();
            tokio::spawn(async move {
                let result = async {
                    let service =
                        JiraService::new(&jira, secret).map_err(|error| error.to_string())?;
                    let (viewer, available_boards) =
                        tokio::try_join!(service.viewer(), service.available_boards())
                            .map_err(|error| error.to_string())?;
                    let mut boards = Vec::new();
                    for id in preserved {
                        boards.push((
                            id,
                            service
                                .inspect_board(&id.to_string())
                                .await
                                .map_err(|error| error.to_string()),
                        ));
                    }
                    Ok((viewer, available_boards, boards))
                }
                .await;
                let _ = sender.send(RuntimeResult::SetupConnection(result));
            });
        }
        AppAction::AddSetupBoard(id) => {
            if state.setup.boards.iter().any(|board| board.id == id) {
                state.setup.message = Some(format!("Board {id} is already added"));
                return false;
            }
            let jira = match state.setup_jira_config(vec![id]) {
                Ok(jira) => jira,
                Err(error) => {
                    state.setup.message = Some(error);
                    return false;
                }
            };
            let secret = state.setup.token.clone();
            state.setup.busy = true;
            state.setup.message = None;
            let sender = tx.clone();
            tokio::spawn(async move {
                let result = match JiraService::new(&jira, secret) {
                    Ok(service) => service
                        .inspect_board(&id.to_string())
                        .await
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                };
                let _ = sender.send(RuntimeResult::SetupBoard { id, result });
            });
        }
        AppAction::SaveSetup => {
            let new_config = match state.setup_config() {
                Ok(config) => config,
                Err(error) => {
                    state.setup.message = Some(error);
                    return false;
                }
            };
            let secret = state.setup.token.clone();
            let path = config_path.to_path_buf();
            state.setup.busy = true;
            state.setup.message = None;
            let sender = tx.clone();
            tokio::spawn(async move {
                let result = async {
                    token::save_to_keyring(&new_config.jira, &secret)
                        .map_err(|error| format!("Could not save credential: {error}"))?;
                    new_config
                        .save(&path)
                        .map_err(|error| format!("Could not save Config: {error}"))?;
                    let service = JiraService::new(&new_config.jira, secret)
                        .map_err(|error| error.to_string())?;
                    let board_ref = service
                        .board_refs()
                        .into_iter()
                        .next()
                        .ok_or_else(|| "No Board configured".to_string())?;
                    let (board, issues) = service
                        .load_board_and_issues(&board_ref)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok((new_config, service, board, issues))
                }
                .await;
                let _ = sender.send(RuntimeResult::SetupSaved(result));
            });
        }
    }
    false
}

fn handle_result(
    message: RuntimeResult,
    state: &mut AppState,
    service: &mut Option<Arc<JiraService>>,
    config: &mut Option<Config>,
) -> Option<AppAction> {
    match message {
        RuntimeResult::Loaded { generation, result } => {
            if generation != state.request_generation {
                return None;
            }
            state.loading = false;
            state.refreshing = false;
            match result {
                Ok((board, issues)) => {
                    let cache_error = save_cache(state, config.as_ref(), &board, &issues).err();
                    if let Some(name) = state.board_names.get_mut(state.board_ref_index) {
                        *name = board.name.clone();
                    }
                    state.board = Some(board);
                    state.issues = issues;
                    state.offline = false;
                    state.network = NetworkState::Connected;
                    state.error = None;
                    state.retry_action = None;
                    state.apply_filters();
                    state.status_message = Some(match cache_error {
                        Some(error) => {
                            format!("Board is up to date · cache was not saved: {error}")
                        }
                        None => "Board is up to date".into(),
                    });
                }
                Err(error)
                    if state.board.is_some()
                        && !matches!(&error, crate::jira::JiraError::Authentication(_)) =>
                {
                    state.offline = true;
                    state.network = network_state_for_error(&error);
                    state.status_message = Some(format!("Read-only cache · {error}"));
                }
                Err(error) => show_jira_error(state, error, Some(AppAction::Refresh)),
            }
        }
        RuntimeResult::Updated { key, result } => {
            state.updating_key = None;
            match result {
                Ok(issue) => {
                    if let Some(slot) = state.issues.iter_mut().find(|item| item.key == key) {
                        *slot = issue;
                    }
                    state.apply_filters();
                    let cache_error = state.board.as_ref().and_then(|board| {
                        save_cache(state, config.as_ref(), board, &state.issues).err()
                    });
                    state.status_message = Some(match cache_error {
                        Some(error) => format!("Updated {key} · cache was not saved: {error}"),
                        None => format!("Updated {key}"),
                    });
                }
                Err(error) => show_jira_error(state, error, None),
            }
        }
        RuntimeResult::Transitions { issue_key, request_id, result } => {
            if request_id != state.edit_request_id
                || state.editing_issue_key.as_deref() != Some(issue_key.as_str())
                || state.modal != Modal::Detail
                || state.edit_index != 0
            {
                return None;
            }
            match result {
                Ok(transitions) if transitions.is_empty() => show_error(
                    state,
                    "No Status transitions are available for this Issue".into(),
                    None,
                ),
                Ok(transitions) => {
                    state.transitions = transitions;
                    state.picker_index = 0;
                    state.modal = Modal::TransitionPicker;
                    state.status_message = None;
                }
                Err(error) => show_jira_error(state, error, None),
            }
        }
        RuntimeResult::Choices(result) => match result {
            Ok(choices) => {
                state.choices = choices;
                state.picker_index = 0;
            }
            Err(error) => show_jira_error(state, error, None),
        },
        RuntimeResult::AssigneeChoices { query, result } => {
            if state.modal != Modal::AssigneePicker || query != state.input_buffer {
                return None;
            }
            match result {
                Ok(choices) => {
                    state.choices = choices;
                    state.picker_index = 0;
                }
                Err(error) => show_jira_error(state, error, None),
            }
        }
        RuntimeResult::Activity { board_ref, request_id, result } => {
            if request_id != state.activity_request_id
                || state.current_board_ref() != Some(board_ref.as_str())
            {
                return None;
            }
            state.activity_loading = false;
            match result {
                Ok(items) => {
                    state.activities = items;
                    state.activity_selected = 0;
                }
                Err(error) => show_jira_error(state, error, Some(AppAction::LoadActivity)),
            }
        }
        RuntimeResult::Viewer(result) => match result {
            Ok(viewer) => {
                state.current_user = Some(viewer.id);
                state.apply_filters();
            }
            Err(error) => state.status_message = Some(format!("My Issues unavailable: {error}")),
        },
        RuntimeResult::BoardNames(results) => {
            let mut failed = Vec::new();
            for (index, result) in results {
                match result {
                    Ok(board) => {
                        if let Some(name) = state.board_names.get_mut(index) {
                            *name = board.name;
                        }
                    }
                    Err(_) => failed.push(
                        state.board_refs.get(index).cloned().unwrap_or_else(|| "unknown".into()),
                    ),
                }
            }
            if !failed.is_empty() {
                state.status_message =
                    Some(format!("Unavailable Board ID(s): {} · run doctor", failed.join(", ")));
            }
        }
        RuntimeResult::SetupConnection(result) => {
            state.setup.busy = false;
            match result {
                Ok((viewer, available, preserved)) => {
                    state.setup.step = crate::ui::setup::SetupStep::Boards;
                    state.setup.field = crate::ui::setup::SetupField::BoardId;
                    state.setup.boards.clear();
                    state.setup.available_boards = available
                        .into_iter()
                        .filter_map(|board| {
                            Some(crate::ui::setup::SetupBoard {
                                id: board.id.parse().ok()?,
                                name: board.label,
                            })
                        })
                        .collect();
                    state.setup.board_index = 0;
                    state.setup.board_input.clear();
                    let mut failed = Vec::new();
                    for (id, result) in preserved {
                        match result {
                            Ok(board) => {
                                let board = crate::ui::setup::SetupBoard { id, name: board.name };
                                state.setup.boards.push(board.clone());
                                if !state
                                    .setup
                                    .available_boards
                                    .iter()
                                    .any(|available| available.id == id)
                                {
                                    state.setup.available_boards.push(board);
                                }
                            }
                            Err(error) => failed.push(format!("{id}: {error}")),
                        }
                    }
                    state.setup.preserved_board_ids.clear();
                    state.setup.message = Some(if failed.is_empty() {
                        format!("Authenticated as {}", viewer.label)
                    } else {
                        format!(
                            "Authenticated as {}; re-add failed Board(s): {}",
                            viewer.label,
                            failed.join("; ")
                        )
                    });
                }
                Err(error) => state.setup.message = Some(format!("Connection failed: {error}")),
            }
        }
        RuntimeResult::SetupBoard { id, result } => {
            state.setup.busy = false;
            match result {
                Ok(board) => {
                    let board = crate::ui::setup::SetupBoard { id, name: board.name };
                    state.setup.boards.push(board.clone());
                    if !state.setup.available_boards.iter().any(|available| available.id == id) {
                        state.setup.available_boards.push(board);
                    }
                    state.setup.board_input.clear();
                    state.setup.message = Some(format!("Board {id} verified"));
                }
                Err(error) => state.setup.message = Some(format!("Board {id} failed: {error}")),
            }
        }
        RuntimeResult::SetupSaved(result) => {
            state.setup.busy = false;
            match result {
                Ok((new_config, new_service, board, issues)) => {
                    state.board_refs = new_config.jira.board_refs();
                    state.board_names =
                        state.setup.boards.iter().map(|board| board.name.clone()).collect();
                    state.board_ref_index = 0;
                    state.board = Some(board);
                    state.issues = issues;
                    state.view = View::Dashboard;
                    state.modal = Modal::None;
                    state.loading = false;
                    state.offline = false;
                    state.network = NetworkState::Connected;
                    state.apply_filters();
                    *service = Some(Arc::new(new_service));
                    *config = Some(new_config);
                    state.status_message = Some("Setup complete".into());
                }
                Err(error) => state.setup.message = Some(error),
            }
        }
    }
    None
}

fn show_error(state: &mut AppState, error: String, retry: Option<AppAction>) {
    state.error = Some(error);
    state.retry_action = retry;
    state.modal = Modal::Error;
    state.loading = false;
    state.refreshing = false;
}

fn network_state_for_error(error: &crate::jira::JiraError) -> NetworkState {
    match error {
        crate::jira::JiraError::Authentication(_) => NetworkState::AuthError,
        crate::jira::JiraError::RateLimited { .. } => NetworkState::RateLimited,
        crate::jira::JiraError::TimeoutOrOffline(_) | crate::jira::JiraError::Network(_) => {
            NetworkState::Offline
        }
        _ => NetworkState::Connected,
    }
}

fn show_jira_error(state: &mut AppState, error: crate::jira::JiraError, retry: Option<AppAction>) {
    state.network = network_state_for_error(&error);
    if matches!(
        state.network,
        NetworkState::Offline | NetworkState::RateLimited | NetworkState::AuthError
    ) {
        state.offline = true;
    }
    show_error(state, error.to_string(), retry);
}

fn cache_identity(config: Option<&Config>) -> String {
    config
        .map(|config| format!("{}:{}", config.jira.keyring_service(), config.jira.keyring_user()))
        .unwrap_or_else(|| "jira".into())
}

fn load_cache(state: &mut AppState, config: Option<&Config>) {
    let Some(board_ref) = state.current_board_ref().map(str::to_owned) else { return };
    let identity = cache_identity(config);
    if let Some(cache) = crate::infrastructure::cache::CacheData::load(&identity, &board_ref, true)
    {
        let expired = cache.is_expired();
        state.board = Some(cache.board);
        state.issues = cache.issues;
        state.offline = true;
        state.network = NetworkState::Offline;
        state.apply_filters();
        state.status_message = Some(format!(
            "Read-only {}cache · saved {}",
            if expired { "stale " } else { "" },
            cache.cached_at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")
        ));
    }
}

fn save_cache(
    state: &AppState,
    config: Option<&Config>,
    board: &Board,
    issues: &[Issue],
) -> anyhow::Result<()> {
    let Some(board_ref) = state.current_board_ref() else { return Ok(()) };
    crate::infrastructure::cache::CacheData::save(&cache_identity(config), board_ref, board, issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::BoardColumn;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use wiremock::{
        matchers::{header, method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    fn board() -> Board {
        Board {
            id: 1,
            name: "Board".into(),
            columns: vec![BoardColumn { name: "To Do".into(), statuses: vec!["To Do".into()] }],
        }
    }

    fn empty_runtime_dependencies() -> (Option<Arc<JiraService>>, Option<Config>) {
        (None, None)
    }

    async fn run_setup_connection(state: &mut AppState) {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (mut service, mut config) = empty_runtime_dependencies();
        let temp = tempfile::tempdir().unwrap();
        assert!(!process_action(
            AppAction::TestSetupConnection,
            state,
            &mut service,
            &mut config,
            &temp.path().join("config.toml"),
            &tx,
        ));
        assert!(state.setup.busy);
        let result = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("Setup request timed out")
            .expect("Setup request task closed without a result");
        handle_result(result, state, &mut service, &mut config);
    }

    fn setup_state(url: String, auth: crate::infrastructure::config::JiraAuth) -> AppState {
        let mut state = AppState { view: View::Setup, ..Default::default() };
        state.setup.url = url;
        state.setup.allow_insecure_http = true;
        state.setup.auth = auth;
        state.setup.auth_explicit = true;
        state.setup.username = "alice@example.com".into();
        state.setup.token = "new-token".into();
        state
    }

    async fn mount_board_discovery(server: &MockServer, authorization: &str) {
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board"))
            .and(query_param("startAt", "0"))
            .and(query_param("maxResults", "50"))
            .and(header("authorization", authorization))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "startAt": 0,
                "maxResults": 50,
                "total": 1,
                "isLast": true,
                "values": [{"id": 42, "name": "Team Board", "type": "kanban"}]
            })))
            .expect(1)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn cloud_setup_authenticates_and_discovers_boards() {
        let server = MockServer::start().await;
        let authorization = format!("Basic {}", BASE64.encode("alice@example.com:new-token"));
        Mock::given(method("GET"))
            .and(path("/rest/api/2/myself"))
            .and(header("authorization", authorization.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accountId": "cloud-account",
                "displayName": "Alice Cloud"
            })))
            .expect(1)
            .mount(&server)
            .await;
        mount_board_discovery(&server, &authorization).await;
        let mut state =
            setup_state(server.uri(), crate::infrastructure::config::JiraAuth::CloudBasicApiToken);

        run_setup_connection(&mut state).await;

        assert_eq!(state.setup.step, crate::ui::setup::SetupStep::Boards);
        assert_eq!(state.setup.available_boards.len(), 1);
        assert_eq!(state.setup.available_boards[0].name, "Team Board");
        assert_eq!(state.setup.message.as_deref(), Some("Authenticated as Alice Cloud"));
    }

    #[tokio::test]
    async fn data_center_auth_recovery_revalidates_existing_board() {
        let server = MockServer::start().await;
        let authorization = "Bearer new-token";
        Mock::given(method("GET"))
            .and(path("/rest/api/2/myself"))
            .and(header("authorization", authorization))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "alice",
                "displayName": "Alice Data Center"
            })))
            .expect(1)
            .mount(&server)
            .await;
        mount_board_discovery(&server, authorization).await;
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board/42"))
            .and(header("authorization", authorization))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "name": "Team Board",
                "type": "kanban"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/agile/1.0/board/42/configuration"))
            .and(header("authorization", authorization))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "columnConfig": {
                    "constraintType": "none",
                    "columns": [{
                        "name": "Offen",
                        "statuses": [{"id": "1", "name": "Offen"}]
                    }]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let mut state =
            setup_state(server.uri(), crate::infrastructure::config::JiraAuth::DataCenterBearerPat);
        state.network = NetworkState::AuthError;
        state.offline = true;
        state.setup.preserved_board_ids = vec![42];

        run_setup_connection(&mut state).await;

        assert_eq!(state.setup.step, crate::ui::setup::SetupStep::Boards);
        assert_eq!(state.setup.boards.len(), 1);
        assert_eq!(state.setup.boards[0].id, 42);
        assert!(state.setup.preserved_board_ids.is_empty());
        assert_eq!(state.setup.message.as_deref(), Some("Authenticated as Alice Data Center"));
    }

    #[test]
    fn stale_transition_response_does_not_reopen_editor() {
        let mut state = AppState {
            modal: Modal::Detail,
            edit_index: 0,
            detail_issue_key: Some("P-1".into()),
            editing_issue_key: Some("P-1".into()),
            edit_request_id: 7,
            ..Default::default()
        };
        let (mut service, mut config) = empty_runtime_dependencies();

        handle_result(
            RuntimeResult::Transitions {
                issue_key: "P-1".into(),
                request_id: 6,
                result: Ok(vec![TransitionOption {
                    id: "31".into(),
                    name: "Start".into(),
                    target_status: "In Progress".into(),
                }]),
            },
            &mut state,
            &mut service,
            &mut config,
        );

        assert_eq!(state.modal, Modal::Detail);
        assert!(state.transitions.is_empty());
    }

    #[test]
    fn activity_response_for_previous_board_is_ignored() {
        let mut state = AppState {
            board_refs: vec!["1".into(), "2".into()],
            board_ref_index: 1,
            activity_request_id: 4,
            activity_loading: true,
            ..Default::default()
        };
        let (mut service, mut config) = empty_runtime_dependencies();

        handle_result(
            RuntimeResult::Activity {
                board_ref: "1".into(),
                request_id: 4,
                result: Ok(Vec::new()),
            },
            &mut state,
            &mut service,
            &mut config,
        );

        assert!(state.activity_loading);
    }

    #[test]
    fn cached_authentication_failure_opens_repairable_error() {
        let mut state =
            AppState { board: Some(board()), request_generation: 3, ..Default::default() };
        let (mut service, mut config) = empty_runtime_dependencies();

        handle_result(
            RuntimeResult::Loaded {
                generation: 3,
                result: Err(crate::jira::JiraError::Authentication("expired".into())),
            },
            &mut state,
            &mut service,
            &mut config,
        );

        assert_eq!(state.modal, Modal::Error);
        assert_eq!(state.network, NetworkState::AuthError);
        assert!(state.offline);
    }
}
