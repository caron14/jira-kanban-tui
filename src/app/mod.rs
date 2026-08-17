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
type SetupConnectionResult = Result<(Choice, SetupBoardResults), String>;

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
    Loaded { generation: u64, result: Result<(Board, Vec<Issue>), crate::jira::JiraError> },
    Updated { key: String, result: Result<Issue, crate::jira::JiraError> },
    Transitions(Result<Vec<TransitionOption>, crate::jira::JiraError>),
    Choices(Result<Vec<Choice>, crate::jira::JiraError>),
    AssigneeChoices { query: String, result: Result<Vec<Choice>, crate::jira::JiraError> },
    Activity(Result<Vec<Activity>, crate::jira::JiraError>),
    Viewer(Result<Choice, crate::jira::JiraError>),
    BoardNames(Vec<(usize, Result<Board, crate::jira::JiraError>)>),
    SetupConnection(SetupConnectionResult),
    SetupBoard { id: i64, result: Result<Board, String> },
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
    state.setup.url = config.jira.url.clone();
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
        AppAction::OpenIssue => {
            if let (Some(active), Some(issue)) = (service.as_ref(), state.selected_issue()) {
                if let Err(error) = open::that(active.issue_url(&issue.key)) {
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
                    schedule_load(state, active, tx.clone());
                }
            }
        }
        AppAction::LoadActivity => {
            if let (Some(active), Some(board_ref)) =
                (service.clone(), state.current_board_ref().map(str::to_owned))
            {
                let sender = tx.clone();
                tokio::spawn(async move {
                    let since = chrono::Utc::now() - chrono::Duration::days(1);
                    let _ = sender
                        .send(RuntimeResult::Activity(active.activity(&board_ref, since).await));
                });
            }
        }
        AppAction::LoadTransitions => {
            if state.offline {
                show_error(
                    state,
                    "Cached data is read-only. Refresh before editing.".into(),
                    Some(AppAction::Refresh),
                );
                return false;
            }
            if let (Some(active), Some(issue)) = (service.clone(), state.selected_issue()) {
                let key = issue.key.clone();
                let sender = tx.clone();
                tokio::spawn(async move {
                    let _ = sender.send(RuntimeResult::Transitions(active.transitions(&key).await));
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
        AppAction::Update(command) => {
            if state.offline {
                show_error(
                    state,
                    "Cached data is read-only. Refresh before editing.".into(),
                    Some(AppAction::Refresh),
                );
                return false;
            }
            let (Some(active), Some(issue)) = (service.clone(), state.selected_issue()) else {
                return false;
            };
            if state.updating_key.is_some() {
                return false;
            }
            let key = issue.key.clone();
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
            let preserved = state.setup.preserved_board_ids.clone();
            state.setup.busy = true;
            state.setup.message = None;
            let sender = tx.clone();
            tokio::spawn(async move {
                let result = async {
                    let service =
                        JiraService::new(&jira, secret).map_err(|error| error.to_string())?;
                    let viewer = service.viewer().await.map_err(|error| error.to_string())?;
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
                    Ok((viewer, boards))
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
                    save_cache(state, config.as_ref(), &board, &issues);
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
                    state.status_message = Some("Board is up to date".into());
                }
                Err(error) if state.board.is_some() => {
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
                    state.status_message = Some(format!("Updated {key}"));
                }
                Err(error) => show_jira_error(state, error, None),
            }
        }
        RuntimeResult::Transitions(result) => match result {
            Ok(transitions) if transitions.is_empty() => {
                show_error(state, "No Status transitions are available for this Issue".into(), None)
            }
            Ok(transitions) => {
                state.transitions = transitions;
                state.picker_index = 0;
                state.modal = Modal::TransitionPicker;
                state.status_message = None;
            }
            Err(error) => show_jira_error(state, error, None),
        },
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
        RuntimeResult::Activity(result) => match result {
            Ok(items) => {
                state.activities = items;
                state.activity_selected = 0;
            }
            Err(error) => show_jira_error(state, error, Some(AppAction::LoadActivity)),
        },
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
                Ok((viewer, preserved)) => {
                    state.setup.step = crate::ui::setup::SetupStep::Boards;
                    state.setup.field = crate::ui::setup::SetupField::BoardId;
                    state.setup.boards.clear();
                    let mut failed = Vec::new();
                    for (id, result) in preserved {
                        match result {
                            Ok(board) => state
                                .setup
                                .boards
                                .push(crate::ui::setup::SetupBoard { id, name: board.name }),
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
                    state.setup.boards.push(crate::ui::setup::SetupBoard { id, name: board.name });
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

fn save_cache(state: &AppState, config: Option<&Config>, board: &Board, issues: &[Issue]) {
    let Some(board_ref) = state.current_board_ref() else { return };
    let _ = crate::infrastructure::cache::CacheData::save(
        &cache_identity(config),
        board_ref,
        board,
        issues,
    );
}
