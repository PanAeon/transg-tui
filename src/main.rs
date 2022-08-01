mod command_processor;
mod config;
mod torrent_stats;
mod transmission;
mod ui;
mod utils;

use binary_heap_plus::BinaryHeap;
use command_processor::{TorrentCmd, TorrentUpdate};
use config::{Config, TrafficMonitorOptions};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{collections::HashMap, io};
use tokio::sync::mpsc::{Receiver, Sender};
use torrent_stats::{update_torrent_stats, TorrentGroupStats};
use transmission::{SessionStats, TorrentDetails, TorrentInfo, TorrentStatus};
use tui::{
    backend::{Backend, CrosstermBackend},
    widgets::{ListState, TableState},
    Terminal,
};
use tui_tree_widget::{flatten, get_identifier_without_leaf, TreeItem, TreeState};
use utils::{build_file_tree, process_folder};

#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    ByStatus(TorrentStatus),
    ByDirectory(String),
    Recent,
    Active,
    All,
    Search(String),
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Transition {
    MainScreen,
    Action,
    Filter,
    Search,
    ConfirmRemove(bool),
    Move,
    Files,
    Help,
    Find(bool, usize),
    ChooseSortFunc,
    Connection,
}

impl Transition {
    pub fn is_find(&self) -> bool {
        matches!(self, Transition::Find(_, _))
    }
}

pub fn calculate_folder_keys(app: &mut App, skip_folder: Option<String>) {
    let sk_folder = skip_folder.unwrap_or_else(|| "".to_string());
    let mut folder_items: Vec<String> = app
        .groups
        .folders
        .iter()
        .filter(|x| x.0 != &sk_folder)
        .map(|x| x.0.clone())
        .collect();
    folder_items.sort();

    let mut mappings: Vec<(String, char, usize)> = vec![];

    folder_items.iter().for_each(|x| {
        let name = process_folder(x, &app.config.connections[app.connection_idx].download_dir);

        let (i, c) = name
            .chars()
            .enumerate()
            .find(|x| !mappings.iter().any(|y| y.1 == x.1))
            .expect("unique");
        mappings.push((x.to_string(), c, i));
    });
    app.folder_mapping = mappings;
}

pub struct SortFunction {
    pub name: String,
    pub func: fn(&mut [TorrentInfo]) -> (),
}

fn by_date_added(xs: &mut [TorrentInfo]) {
    xs.sort_unstable_by_key(|x| -x.added_date);
}

fn by_size(xs: &mut [TorrentInfo]) {
    xs.sort_unstable_by_key(|x| -x.size_when_done);
}

fn by_ratio(xs: &mut [TorrentInfo]) {
    xs.sort_unstable_by_key(|x| (-x.upload_ratio * 1000.0) as i64);
}

fn by_uploaded(xs: &mut [TorrentInfo]) {
    xs.sort_unstable_by_key(|x| -x.uploaded_ever);
}

pub struct App<'a> {
    pub transition: Transition,
    pub prev_transition: Transition,
    pub left_filter_state: ListState,
    pub main_table_state: TableState,
    pub torrents: HashMap<i64, TorrentInfo>,
    pub filtered_torrents: Vec<TorrentInfo>,
    pub free_space: u64,
    pub stats: SessionStats,
    pub groups: TorrentGroupStats,
    pub selected: Option<TorrentInfo>,
    pub folder_mapping: Vec<(String, char, usize)>,
    pub current_filter: Filter,
    pub upload_data: Vec<u64>,
    pub num_active: usize,
    pub input: String,
    pub tree_state: TreeState,
    pub details: Option<TorrentDetails>,
    pub tree_items: Vec<TreeItem<'a>>,
    pub config: Config,
    pub err: Option<(String, String)>,
    pub sort_func: SortFunction,
    pub connection_idx: usize,
}

impl App<'_> {
    fn reset(&mut self) {
        self.transition = Transition::MainScreen;
        self.prev_transition = Transition::MainScreen;
        self.left_filter_state = ListState::default();
        self.main_table_state = TableState::default();
        self.torrents = HashMap::new();
        self.filtered_torrents = vec![];
        self.free_space = 0;
        self.stats = SessionStats::empty();
        self.groups = TorrentGroupStats::empty();
        self.selected = None;
        self.folder_mapping = vec![];
        self.upload_data = vec![];
        self.num_active = 0;
        self.input = "".to_string();
        self.tree_state = TreeState::default();
        self.details = None;
        self.tree_items = vec![];
        self.err = None;
    }
}

impl App<'_> {
    fn new(config: Config) -> Self {
        let left_filter_state = ListState::default();
        let main_table_state = TableState::default();
        let torrents: HashMap<i64, TorrentInfo> = HashMap::new();
        let filtered_torrents: Vec<TorrentInfo> = vec![];
        let free_space: u64 = 0;
        let stats: SessionStats = SessionStats::empty();
        let groups: TorrentGroupStats = TorrentGroupStats::empty();

        App {
            transition: Transition::MainScreen,
            prev_transition: Transition::MainScreen,
            left_filter_state,
            main_table_state,
            torrents,
            filtered_torrents,
            free_space,
            stats,
            groups,
            selected: None,
            folder_mapping: vec![],
            current_filter: Filter::Recent,
            upload_data: vec![],
            num_active: 0,
            input: "".to_string(),
            tree_state: TreeState::default(),
            details: None,
            tree_items: vec![],
            config,
            err: None,
            sort_func: SortFunction {
                name: String::from("Date Added"),
                func: by_date_added,
            },
            connection_idx: 0,
        }
    }
}

// TODO: min size 67 x 20, then super-min-size.
// then we need name, size, done, up, down
fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    mut rx: Receiver<TorrentUpdate>,
    sender: Sender<TorrentCmd>,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::ui(f, &mut app))?;

        match rx.blocking_recv() {
            Some(TorrentUpdate::UiTick) => {}
            Some(TorrentUpdate::Err { msg, details }) => {
                if app.err.is_none() {
                    // FIXME: poor keep first error, may be bad in general, but should do for now
                    app.err = Some((msg, details));
                }
            }
            Some(TorrentUpdate::Input(event)) => match event.code {
                KeyCode::Char('q') => {
                    //let _ = sender.blocking_send(TorrentCmd::PoisonPill);
                    break Ok(());
                }
                _ => {
                    match app.transition {
                        Transition::MainScreen => match event.code {
                            KeyCode::Char(' ') => {
                                if app.selected.is_some() {
                                    app.transition = Transition::Action;
                                }
                            }
                            KeyCode::F(1) => {
                                app.prev_transition = app.transition;
                                app.transition = Transition::Help;
                            }
                            KeyCode::Char('c') => {
                                app.transition = Transition::Connection;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if let Some(selected) = app.main_table_state.selected() {
                                    let amount_pets = app.filtered_torrents.len();
                                    if selected >= amount_pets - 1 {
                                        select_first_torrent(&mut app, sender.clone());
                                    } else {
                                        app.main_table_state.select(Some(selected + 1));
                                        app.selected = Some(app.filtered_torrents[selected + 1].clone());
                                        sender
                                            .blocking_send(TorrentCmd::GetDetails(
                                                app.filtered_torrents[selected + 1].id,
                                            ))
                                            .expect("foo");
                                    }
                                } else {
                                    select_first_torrent(&mut app, sender.clone());
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if !app.filtered_torrents.is_empty() {
                                    if let Some(selected) = app.main_table_state.selected() {
                                        let selected = selected.min(app.filtered_torrents.len());
                                        let amount_pets = app.filtered_torrents.len();
                                        if selected > 0 {
                                            app.main_table_state.select(Some(selected - 1));
                                            app.selected = Some(app.filtered_torrents[selected - 1].clone());
                                            sender
                                                .blocking_send(TorrentCmd::GetDetails(
                                                    app.filtered_torrents[selected - 1].id,
                                                ))
                                                .expect("foo");
                                        } else {
                                            app.main_table_state.select(Some(amount_pets - 1));
                                            app.selected = Some(app.filtered_torrents[amount_pets - 1].clone());
                                            sender
                                                .blocking_send(TorrentCmd::GetDetails(
                                                    app.filtered_torrents[amount_pets - 1].id,
                                                ))
                                                .expect("foo");
                                        }
                                    } else {
                                        select_first_torrent(&mut app, sender.clone());
                                    }
                                } else {
                                    app.selected = None;
                                    let _ = sender.blocking_send(TorrentCmd::Select(None));
                                }
                            }
                            KeyCode::Char('f') => {
                                calculate_folder_keys(&mut app, None);
                                app.transition = Transition::Filter;
                            }
                            KeyCode::Char('s') => {
                                app.transition = Transition::Search;
                            }
                            KeyCode::Char('/') => {
                                app.input = "".to_string();
                                app.transition = Transition::Find(true, app.main_table_state.selected().unwrap_or(0));
                            }
                            KeyCode::Char('?') => {
                                app.input = "".to_string();
                                app.transition = Transition::Find(false, app.main_table_state.selected().unwrap_or(0));
                            }
                            KeyCode::Char('d') => {
                                app.transition = Transition::Files;
                                app.tree_state = TreeState::default();
                                open_first_level(&mut app);
                            }
                            KeyCode::Char('S') => {
                                app.transition = Transition::ChooseSortFunc;
                            }
                            KeyCode::Esc => {
                                if let Filter::Search(_) = app.current_filter {
                                    app.current_filter = Filter::Recent;
                                }
                            }
                            _ => {}
                        },
                        Transition::Action => match event.code {
                            KeyCode::Char(' ') | KeyCode::Esc => {
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('s') => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    sender
                                        .blocking_send(TorrentCmd::Start(vec![x.id]))
                                        .expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('S') => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    sender
                                        .blocking_send(TorrentCmd::StartNow(vec![x.id]))
                                        .expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('p') => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    sender.blocking_send(TorrentCmd::Stop(vec![x.id])).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('v') => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    sender
                                        .blocking_send(TorrentCmd::Verify(vec![x.id]))
                                        .expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('m') => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                    .map(|x| x.download_dir.clone())
                                {
                                    calculate_folder_keys(&mut app, Some(x));
                                    app.transition = Transition::Move;
                                }
                            }
                            KeyCode::Char('x') => {
                                app.transition = Transition::ConfirmRemove(false);
                            }
                            KeyCode::Char('X') => {
                                app.transition = Transition::ConfirmRemove(true);
                            }
                            KeyCode::Char('k') => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    sender
                                        .blocking_send(TorrentCmd::QueueMoveUp(vec![x.id]))
                                        .expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('j') => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    sender
                                        .blocking_send(TorrentCmd::QueueMoveDown(vec![x.id]))
                                        .expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('K') => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    sender
                                        .blocking_send(TorrentCmd::QueueMoveTop(vec![x.id]))
                                        .expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('J') => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    sender
                                        .blocking_send(TorrentCmd::QueueMoveBottom(vec![x.id]))
                                        .expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char(c) => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    if let Some(idx) = app
                                        .config
                                        .actions
                                        .iter()
                                        .enumerate()
                                        .find(|x| x.1.shortcut.starts_with(c))
                                    {
                                        sender
                                            .blocking_send(TorrentCmd::Action(x.id, idx.0))
                                            .expect("should send");
                                    }
                                }
                                app.transition = Transition::MainScreen;
                            }
                            _ => {}
                        },
                        Transition::Filter => match event.code {
                            KeyCode::Esc => {
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char(c) => {
                                if let Some(x) = app.folder_mapping.iter().find(|x| x.1 == c) {
                                    app.current_filter = Filter::ByDirectory(x.0.clone());
                                    let idx = 12
                                        + app
                                            .folder_mapping
                                            .iter()
                                            .enumerate()
                                            .find(|y| y.1 .0 == x.0)
                                            .map(|x| x.0)
                                            .unwrap_or(0);
                                    app.left_filter_state.select(Some(idx));
                                    app.transition = Transition::MainScreen;
                                    app.filtered_torrents = app
                                        .torrents
                                        .values()
                                        .filter(|y| y.download_dir == x.0)
                                        .cloned()
                                        .collect();
                                    (app.sort_func.func)(&mut app.filtered_torrents);
                                    select_first_torrent(&mut app, sender.clone());
                                } else {
                                    match c {
                                        'R' => {
                                            app.current_filter = Filter::Recent;
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(0));
                                            app.filtered_torrents = most_recent_items(&app.torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'A' => {
                                            app.current_filter = Filter::Active;
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(1));
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'P' => {
                                            app.current_filter = Filter::ByStatus(TorrentStatus::Paused);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(2));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == TorrentStatus::Paused)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'L' => {
                                            app.current_filter = Filter::All;
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(10));
                                            app.filtered_torrents = app.torrents.values().cloned().collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'G' => {
                                            app.current_filter = Filter::ByStatus(TorrentStatus::VerifyQueued);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(3));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == TorrentStatus::VerifyQueued)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'C' => {
                                            app.current_filter = Filter::ByStatus(TorrentStatus::Verifying);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(4));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == TorrentStatus::Verifying)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'Q' => {
                                            app.current_filter = Filter::ByStatus(TorrentStatus::DownQueued);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(5));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == TorrentStatus::DownQueued)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'D' => {
                                            app.current_filter = Filter::ByStatus(TorrentStatus::Downloading);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(6));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == TorrentStatus::Downloading)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'U' => {
                                            app.current_filter = Filter::ByStatus(TorrentStatus::SeedQueued);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(7));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == TorrentStatus::SeedQueued)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'S' => {
                                            app.current_filter = Filter::ByStatus(TorrentStatus::Seeding);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(8));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == TorrentStatus::Seeding)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'E' => {
                                            app.current_filter = Filter::Error;
                                            app.filtered_torrents =
                                                app.torrents.values().filter(|x| x.error > 0).cloned().collect();
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(9));
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        },
                        Transition::Help => match event.code {
                            KeyCode::F(1) | KeyCode::Esc => {
                                app.transition = app.prev_transition.clone();
                            }
                            _ => {}
                        },
                        Transition::Find(forward, current) => match event.code {
                            KeyCode::Esc => {
                                app.input = "".to_string();
                                app.transition = Transition::MainScreen;
                                app.main_table_state.select(Some(current));
                            }
                            KeyCode::Enter => {
                                if let Some(i) = &app.main_table_state.selected() {
                                    let x = &app.filtered_torrents[*i];
                                    app.selected = Some(x.clone());
                                    sender.blocking_send(TorrentCmd::GetDetails(x.id)).expect("foo");
                                    app.transition = Transition::MainScreen;
                                }
                            }
                            KeyCode::Backspace => {
                                app.input.pop(); // now we need go back..
                                let maybe_x = if forward {
                                    app.filtered_torrents
                                        .iter()
                                        .enumerate()
                                        .skip(current.saturating_sub(1))
                                        .find(|x| x.1.name.contains(&app.input))
                                } else {
                                    app.filtered_torrents
                                        .iter()
                                        .enumerate()
                                        .rev()
                                        .skip(app.filtered_torrents.len().saturating_sub(current.saturating_add(1)))
                                        .find(|x| x.1.name.contains(&app.input))
                                };
                                if let Some((i, _)) = maybe_x {
                                    app.main_table_state.select(Some(i));
                                }
                            }
                            KeyCode::Char(c) => {
                                app.input.push(c);
                                let maybe_x = if forward {
                                    app.filtered_torrents
                                        .iter()
                                        .enumerate()
                                        .skip(current.saturating_sub(1))
                                        .find(|x| x.1.name.contains(&app.input))
                                } else {
                                    app.filtered_torrents
                                        .iter()
                                        .enumerate()
                                        .rev()
                                        .skip(app.filtered_torrents.len().saturating_sub(current.saturating_add(1)))
                                        .find(|x| x.1.name.contains(&app.input))
                                };
                                if let Some((i, _)) = maybe_x {
                                    app.main_table_state.select(Some(i));
                                }
                            }
                            _ => {}
                        },
                        Transition::Search => match event.code {
                            KeyCode::Esc => {
                                app.input = "".to_string();
                                app.transition = Transition::MainScreen;
                                if let Filter::Search(_) = app.current_filter {
                                    app.current_filter = Filter::Recent;
                                };
                            }
                            KeyCode::Enter => {
                                app.current_filter = Filter::Search(app.input.clone());
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Backspace => {
                                app.input.pop();
                            }
                            KeyCode::Char(c) => app.input.push(c),
                            _ => {}
                        },
                        Transition::Move => match event.code {
                            KeyCode::Esc => {
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char(c) => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    if let Some((f, _, _)) = app.folder_mapping.iter().find(|y| y.1 == c) {
                                        sender
                                            .blocking_send(TorrentCmd::Move(vec![x.id], f.to_string(), false))
                                            .expect("should send");
                                        app.transition = Transition::MainScreen;
                                    }
                                    /*if let Some(f) = app.groups.folder_keys.get(&c) {
                                    }*/
                                }
                            }
                            _ => {}
                        },
                        Transition::ConfirmRemove(with_data) => match event.code {
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('y') => {
                                if let Some(x) = app
                                    .main_table_state
                                    .selected()
                                    .and_then(|x| app.filtered_torrents.get(x))
                                {
                                    sender
                                        .blocking_send(TorrentCmd::Delete(vec![x.id], with_data))
                                        .expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            _ => {}
                        },
                        Transition::Files => match event.code {
                            KeyCode::Esc | KeyCode::Char('d') => app.transition = Transition::MainScreen,
                            KeyCode::Left | KeyCode::Char('h') => {
                                let selected = app.tree_state.selected();
                                if !app.tree_state.close(&selected) {
                                    let (head, _) = get_identifier_without_leaf(&selected);
                                    app.tree_state.select(head);
                                }
                            }
                            KeyCode::Right | KeyCode::Char('l') => {
                                app.tree_state.open(app.tree_state.selected());
                            }
                            KeyCode::Enter => app.tree_state.toggle(),
                            KeyCode::Down | KeyCode::Char('j') => move_up_down(&mut app, true),
                            KeyCode::Up | KeyCode::Char('k') => move_up_down(&mut app, false),
                            _ => {}
                        },
                        Transition::ChooseSortFunc => match event.code {
                            KeyCode::Esc => {
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('d') => {
                                app.sort_func = SortFunction {
                                    name: String::from("Date Added"),
                                    func: by_date_added,
                                };
                                (app.sort_func.func)(&mut app.filtered_torrents);
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('s') => {
                                app.sort_func = SortFunction {
                                    name: String::from("by size"),
                                    func: by_size,
                                };
                                (app.sort_func.func)(&mut app.filtered_torrents);
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('r') => {
                                app.sort_func = SortFunction {
                                    name: String::from("by ratio"),
                                    func: by_ratio,
                                };
                                (app.sort_func.func)(&mut app.filtered_torrents);
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('u') => {
                                app.sort_func = SortFunction {
                                    name: String::from("by uploaded"),
                                    func: by_uploaded,
                                };
                                (app.sort_func.func)(&mut app.filtered_torrents);
                                app.transition = Transition::MainScreen;
                            }
                            _ => {}
                        },
                        Transition::Connection => match event.code {
                            KeyCode::Esc => {
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char(x) => {
                                if x.is_ascii_digit() {
                                    let x = x as usize - '0' as usize;
                                    if x > 0 {
                                        let x = x - 1;
                                        if x < app.config.connections.len() {
                                            app.reset();
                                            app.connection_idx = x;
                                            let _ = sender.blocking_send(TorrentCmd::Reconnect(x));
                                        }
                                    }
                                }
                            }
                            _ => {}
                        },
                    }
                }
            },

            Some(TorrentUpdate::Partial(json, removed, _i, session_stats, free_space_opt, details)) => {
                app.details = *details;
                app.err = None;

                if let Some(s) = *session_stats {
                    if app.upload_data.len() > 200 {
                        app.upload_data.pop();
                    }
                    if app.config.traffic_monitor == TrafficMonitorOptions::Download {
                        app.upload_data.insert(0, s.download_speed);
                    } else {
                        app.upload_data.insert(0, s.upload_speed);
                    }
                    app.stats = s;
                }
                if let Some(s) = free_space_opt {
                    app.free_space = s.size_bytes;
                }

                let removed: Vec<i64> = removed
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_i64().unwrap())
                    .collect();
                for k in removed {
                    app.torrents.remove(&k);
                }
                let xs = json.as_array().unwrap().clone();

                //let prev_length = app.torrents.len();
                //let prev_filtered_length = app.filtered_torrents.len();

                for x in xs.iter().skip(1) {
                    let ys = x.as_array().unwrap();
                    let id = ys[0].as_i64().unwrap();
                    if let Some(y) = app.torrents.get_mut(&id) {
                        y.update(ys);
                    } else {
                        let info = TorrentInfo::from_json(x).map_err(|r| std::io::Error::new(std::io::ErrorKind::Other, r))?;
                        app.torrents.insert(id, info);
                    }
                }
                app.num_active = xs.len() - 1;
                //if app.torrents.len() != prev_length || app.filtered_torrents.is_empty() {
                app.groups = update_torrent_stats(&app.torrents);
                //}
                match app.current_filter.clone() {
                    Filter::Search(text) => {
                        app.filtered_torrents = app
                            .torrents
                            .values()
                            .filter(|x| x.name.to_lowercase().contains(&text.to_lowercase()))
                            .cloned()
                            .collect();
                        (app.sort_func.func)(&mut app.filtered_torrents);
                    }
                    Filter::ByDirectory(_) => {
                        if let Filter::ByDirectory(d) = app.current_filter.clone() {
                            app.filtered_torrents =
                                app.torrents.values().filter(|x| x.download_dir == d).cloned().collect();
                            (app.sort_func.func)(&mut app.filtered_torrents);
                        }
                    }
                    Filter::ByStatus(_) => {
                        if let Filter::ByStatus(s) = app.current_filter.clone() {
                            app.filtered_torrents = app.torrents.values().filter(|x| x.status == s).cloned().collect();
                            (app.sort_func.func)(&mut app.filtered_torrents);
                        }
                    }
                    Filter::All => {
                        app.filtered_torrents = app.torrents.values().cloned().collect();
                        (app.sort_func.func)(&mut app.filtered_torrents);
                    }
                    Filter::Active => {
                        app.filtered_torrents = xs.iter().skip(1).map(TorrentInfo::new).collect();
                        (app.sort_func.func)(&mut app.filtered_torrents);
                    }
                    Filter::Recent => {
                        // if app.torrents.len() != prev_length || app.filtered_torrents.is_empty() {
                        app.filtered_torrents = most_recent_items(&app.torrents);
                        //}
                        if app.sort_func.name != "Date Added" {
                            (app.sort_func.func)(&mut app.filtered_torrents);
                        }
                    }
                    Filter::Error => {
                        app.filtered_torrents = app.torrents.values().filter(|x| x.error > 0).cloned().collect();
                        (app.sort_func.func)(&mut app.filtered_torrents);
                    }
                }
                if app.main_table_state.selected().is_none() {
                    select_first_torrent(&mut app, sender.clone());
                }
            }
            Some(TorrentUpdate::Full(xs)) => {
                let ts = xs
                    .as_array()
                    .unwrap()
                    .iter()
                    .skip(1)
                    .map(TorrentInfo::new)
                    .map(|it| (it.id, it));
                app.torrents = HashMap::from_iter(ts);
                app.groups = update_torrent_stats(&app.torrents);
                app.left_filter_state.select(Some(0));
                let _ = sender.blocking_send(TorrentCmd::Tick(0));

                //let mut xs: Vec<_> = app.torrents.values().cloned().collect();
                //xs.sort_by_key(|x| -x.added_date);
                //xs.truncate(150);
                //app.filtered_torrents = xs;
            }
            Some(TorrentUpdate::Details(details)) => {
                app.details = Some(*details);
                if let Some(d) = &app.details {
                    app.tree_items = build_file_tree(&d.files);
                    app.tree_state = TreeState::default();
                }
            }
            Some(TorrentUpdate::Session(session)) => {
                if app.config.connections[app.connection_idx].download_dir.is_empty() {
                    app.config.connections[app.connection_idx].download_dir = session.download_dir;
                }
            }
            None => {}
        }
    }
}

fn select_first_torrent(app: &mut App, sender: Sender<TorrentCmd>) {
    if !app.filtered_torrents.is_empty() {
        app.main_table_state.select(Some(0));
        app.selected = Some(app.filtered_torrents[0].clone());
        sender
            .blocking_send(TorrentCmd::GetDetails(app.filtered_torrents[0].id))
            .expect("foo");
    } else {
        app.selected = None;
        let _ = sender.blocking_send(TorrentCmd::Select(None));
    }
}

fn open_first_level(app: &mut App) {
    let visible = flatten(&app.tree_state.get_all_opened(), &app.tree_items);
    for x in visible {
        app.tree_state.open(x.identifier);
    }
}

fn move_up_down(app: &mut App, down: bool) {
    let visible = flatten(&app.tree_state.get_all_opened(), &app.tree_items);
    if !visible.is_empty() {
        let current_identifier = app.tree_state.selected();
        let current_index = visible.iter().position(|o| o.identifier == current_identifier);
        let new_index = current_index.map_or(0, |current_index| {
            if down {
                current_index.saturating_add(1)
            } else {
                current_index.saturating_sub(1)
            }
            .min(visible.len() - 1)
        });
        let new_identifier = visible.get(new_index).unwrap().identifier.clone();
        app.tree_state.select(new_identifier);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initialize config early, so if there's any serious error we don't mess with the terminal
    let config = config::get_or_create_config()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (mut processor, rx) = command_processor::CommandProcessor::create();

    let app = App::new(config);
    processor.run(app.config.clone(), app.connection_idx);
    run_app(&mut terminal, app, rx, processor.get_sender())?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

    terminal.show_cursor()?;
    Ok(())
}

fn most_recent_items(torrents: &HashMap<i64, TorrentInfo>) -> Vec<TorrentInfo> {
    let mut heap =
        BinaryHeap::with_capacity_by(120, |a: &TorrentInfo, b: &TorrentInfo| b.added_date.cmp(&a.added_date));
    for x in torrents.values() {
        if heap.len() > 120 {
            heap.pop();
        }
        heap.push(x.clone());
    }
    heap.into_sorted_vec()
}
