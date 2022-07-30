mod command_processor;
mod config;
mod torrent_stats;
mod transmission;
mod utils;

use binary_heap_plus::BinaryHeap;
use command_processor::{TorrentCmd, TorrentUpdate};
use config::{Config, Action, Connection};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{collections::HashMap, io};
use tokio::sync::mpsc::{Receiver, Sender};
use torrent_stats::{update_torrent_stats, TorrentGroupStats, DOWNLOADING, SEED_QUEUED, VERIFYING};
use transmission::{SessionStats, TorrentDetails, TorrentInfo};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Sparkline, Table,
        TableState, Wrap,
    },
    Frame, Terminal,
};
use tui_tree_widget::{flatten, get_identifier_without_leaf, Tree, TreeItem, TreeState};
use utils::{
    build_file_tree, format_download_speed, format_eta, format_percent_done, format_size, format_status, format_time,
    process_folder, utf8_split, DOWN_QUEUED, SEEDING, STOPPED, VERIFY_QUEUED,
};


#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    ByStatus(i64),
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
    Connection
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
        let name = process_folder(x, &app.config.connections[app.connection_idx].remote_base_dir);

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
    pub func: fn(&mut [TorrentInfo]) -> () 
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
   // pub memory_usage: u64,
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
    pub connection_idx: usize
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

impl Default for App<'_> {
    fn default() -> Self {
        let left_filter_state = ListState::default();
        let main_table_state = TableState::default();
        let torrents: HashMap<i64, TorrentInfo> = HashMap::new();
        let filtered_torrents: Vec<TorrentInfo> = vec![];
        let free_space: u64 = 0;
        let stats: SessionStats = SessionStats::empty();
        let groups: TorrentGroupStats = TorrentGroupStats::empty();
        let config = config::get_or_create_config();

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
            sort_func: SortFunction { name: String::from("Date Added"), func: by_date_added },
            connection_idx: 0
        }
    }
}
/*#[derive(PartialEq)]
enum ProgramRes {
    Exit,
    Reload(usize)
}*/

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    mut rx: Receiver<TorrentUpdate>,
    sender: Sender<TorrentCmd>,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        match rx.blocking_recv() {
            Some(TorrentUpdate::UiTick) => {}
            Some(TorrentUpdate::Err { msg, details }) => {
                if app.err.is_none() {
                    // keep first error, may be bad in general, but should do for now
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
                        Transition::MainScreen => {
                            match event.code {
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
                            }
                        }
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
                                    if let Some(idx) = app.config.actions.iter().enumerate().find(|x| x.1.shortcut.starts_with(c)) {
                                        sender.blocking_send(TorrentCmd::Action(x.id, idx.0)).expect("should send");
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
                                            app.current_filter = Filter::ByStatus(STOPPED);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(2));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == STOPPED)
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
                                            app.current_filter = Filter::ByStatus(VERIFY_QUEUED);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(3));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == VERIFY_QUEUED)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'C' => {
                                            app.current_filter = Filter::ByStatus(VERIFYING);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(4));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == VERIFYING)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'Q' => {
                                            app.current_filter = Filter::ByStatus(DOWN_QUEUED);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(5));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == DOWN_QUEUED)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'D' => {
                                            app.current_filter = Filter::ByStatus(DOWNLOADING);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(6));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == DOWNLOADING)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'U' => {
                                            app.current_filter = Filter::ByStatus(SEED_QUEUED);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(7));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == SEED_QUEUED)
                                                .cloned()
                                                .collect();
                                            (app.sort_func.func)(&mut app.filtered_torrents);
                                            select_first_torrent(&mut app, sender.clone());
                                        }
                                        'S' => {
                                            app.current_filter = Filter::ByStatus(SEEDING);
                                            app.transition = Transition::MainScreen;
                                            app.left_filter_state.select(Some(8));
                                            app.filtered_torrents = app
                                                .torrents
                                                .values()
                                                .filter(|x| x.status == SEEDING)
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
                                            sender
                                                .blocking_send(TorrentCmd::GetDetails(
                                                    x.id
                                                ))
                                                .expect("foo");
                                    app.transition = Transition::MainScreen;
                                }
                            }
                            KeyCode::Backspace => {
                                app.input.pop(); // now we need go back..
                                let maybe_x = if forward {
                                    app.filtered_torrents.iter().enumerate().skip(current.saturating_sub(1)).find(|x| x.1.name.contains(&app.input))
                                } else {
                                    app.filtered_torrents.iter().enumerate().rev().skip(
                                          app.filtered_torrents.len().saturating_sub(current.saturating_add(1))
                                        ).find(|x| x.1.name.contains(&app.input)) 
                                };
                                if let Some((i, _)) = maybe_x {
                                    app.main_table_state.select(Some(i));

                                }
                            }
                            KeyCode::Char(c) => {
                                app.input.push(c);
                                let maybe_x = if forward {
                                    app.filtered_torrents.iter().enumerate().skip(current.saturating_sub(1)).find(|x| x.1.name.contains(&app.input))
                                } else {
                                    app.filtered_torrents.iter().enumerate().rev().skip(
                                          app.filtered_torrents.len().saturating_sub(current.saturating_add(1))
                                        ).find(|x| x.1.name.contains(&app.input)) 
                                };
                                if let Some((i, _)) = maybe_x {
                                    app.main_table_state.select(Some(i));

                                }
                            }
                            _ => {}

                        }
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
                            KeyCode::Esc => { app.transition = Transition::MainScreen; }
                            KeyCode::Char('d') => {
                                app.sort_func = SortFunction { name: String::from("Date Added"), func: by_date_added };
                                (app.sort_func.func)(&mut app.filtered_torrents);
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('s') => {
                                app.sort_func = SortFunction { name: String::from("by size"), func: by_size };
                                (app.sort_func.func)(&mut app.filtered_torrents);
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('r') => {
                                app.sort_func = SortFunction { name: String::from("by ratio"), func: by_ratio };
                                (app.sort_func.func)(&mut app.filtered_torrents);
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('u') => {
                                app.sort_func = SortFunction { name: String::from("by uploaded"), func: by_uploaded };
                                (app.sort_func.func)(&mut app.filtered_torrents);
                                app.transition = Transition::MainScreen;
                            }
                            _ => {}
                        },
                        Transition::Connection => match event.code {
                            KeyCode::Esc => { app.transition = Transition::MainScreen; }
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

                        }
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
                    app.upload_data.insert(0, s.upload_speed);
                    app.stats = s;
                }
                if let Some(s) = free_space_opt {
                    app.free_space = s.size_bytes;
                }
                //app.memory_usage = mem;

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
                        app.torrents.insert(id, TorrentInfo::new(x));
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

fn ui<B: Backend>(frame: &mut Frame<B>, app: &mut App) {
    let size = frame.size();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(2),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

    let status = Paragraph::new(Spans::from(vec![
        //Span::styled(format!("W: {}, H: {} ", frame.size().width, frame.size().height), Style::default()),
        Span::styled(format!("🔨 {}", app.config.connections[app.connection_idx].name), Style::default()),
        //Span::styled(" | Client Mem: ", Style::default()),
        //Span::styled(format_size(app.memory_usage as i64), Style::default().fg(Color::Yellow)),
        Span::styled(" | Free Space: ", Style::default()),
        Span::styled(format_size(app.free_space as i64), Style::default().fg(Color::Yellow)),
        Span::styled(" | Up: ", Style::default()),
        Span::styled(
            format_download_speed(app.stats.upload_speed as i64, false),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(" | Down: ", Style::default()),
        Span::styled(
            format_download_speed(app.stats.download_speed as i64, false),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(" ", Style::default()),
    ]))
    .alignment(Alignment::Right)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White))
            .title("Stats")
            .border_type(BorderType::Plain),
    );


    if app.transition == Transition::Search || app.transition.is_find() {

    let search = Paragraph::new(Spans::from(vec![Span::styled(
        format!("Search: {}▋", app.input),
        Style::default().fg(Color::Yellow),
    )]))
    .alignment(Alignment::Left)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White))
            .title("Search")
            .border_type(BorderType::Plain),
    );
        frame.render_widget(search, chunks[2]);
    } else {
        frame.render_widget(status, chunks[2]);
    }

    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::BOTTOM), 
        )
        .data(&app.upload_data)
        .style(Style::default());
    frame.render_widget(sparkline, chunks[0]);

    match app.transition {
        Transition::Help => {
            let help = help_dialog();
            let width = if size.width > 120 { 30 } else { 60 };
            let area = centered_rect(width, 90, chunks[1]);
            frame.render_widget(help, area);
        }
        Transition::Files => {
            let pets_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
                .split(chunks[1]);
            if let Some(details) = &app.details {
                let details_frame = render_details(details);

                let area = centered_rect(90, 60, pets_chunks[0]);
                frame.render_widget(details_frame, area);

                let block = draw_tree(app.tree_items.clone());
                frame.render_stateful_widget(block, pets_chunks[1], &mut app.tree_state);
            }
        }
        _ => {
            let pets_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        if size.width >= 160 {
                            Constraint::Percentage(13)
                        } else {
                            Constraint::Percentage(25)
                        },
                        Constraint::Percentage(87),
                    ]
                    .as_ref(),
                )
                .split(chunks[1]);
            let filters = render_filters(
                &app.groups,
                &app.transition,
                &app.folder_mapping,
                app.num_active,
                &app.config.connections[app.connection_idx],
            );
            let main_table = render_main_table(&app.left_filter_state, &app.groups, &app.filtered_torrents);

            if size.width > 120 {
                frame.render_stateful_widget(filters, pets_chunks[0], &mut app.left_filter_state);
                frame.render_stateful_widget(main_table, pets_chunks[1], &mut app.main_table_state);
            } else {
                frame.render_stateful_widget(main_table, chunks[1], &mut app.main_table_state);
                if app.transition == Transition::Filter {
                    frame.render_widget(Clear, pets_chunks[0]);
                    frame.render_stateful_widget(filters, pets_chunks[0], &mut app.left_filter_state);
                }
            }
        }
    }
    if let Some((msg, details)) = &app.err {
                let area = centered_rect(36, 40, size);
                let block = error_dialog(msg, details);
                frame.render_widget(Clear, area);
                frame.render_widget(block, area);

    }
    match app.transition {
        Transition::Action => {
            let block = Block::default().title("Actions").borders(Borders::ALL);
            let area_width = if size.width > 120 { 16 } else { 38 };
            let area = centered_rect(area_width, 42, size);
            let vert_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(15)].as_ref())
                .split(block.inner(area));
            let list = action_menu(&app.config.actions);
            frame.render_widget(Clear, area);
            frame.render_widget(block, area);

            let title = app.selected.as_ref().map_or_else(
                || "".to_string(),
                |x| {
                    if x.name.len() > 25 {
                        "\n ".to_owned() + &x.name.chars().take(25).collect::<String>() + "…"
                    } else {
                        "\n ".to_owned() + &x.name
                    }
                },
            );
            let status = Paragraph::new(title)
                .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
                .wrap(Wrap { trim: false });

            frame.render_widget(status, vert_layout[0]);
            frame.render_widget(list, vert_layout[1]);
        }
        Transition::ConfirmRemove(with_data) => {
            if let Some(x) = app
                .main_table_state
                .selected()
                .and_then(|x| app.filtered_torrents.get(x))
            {
                let area = centered_rect(46, 15, size);
                let block = delete_confirmation_dialog(with_data, &x.name);
                frame.render_widget(Clear, area);
                frame.render_widget(block, area);
            }
        }
        Transition::Move => {
            if let Some(x) = app
                .main_table_state
                .selected()
                .and_then(|x| app.filtered_torrents.get(x))
            {
                move_dialog(frame, &x.name, &app.folder_mapping, &app.config.connections[app.connection_idx]);
            }
        }
        Transition::ChooseSortFunc => {
                let area = centered_rect(26, 35, size);
                let block = choose_sort_dialog();
                frame.render_widget(Clear, area);
                frame.render_widget(block, area);
        }
        Transition::Connection => {
                let area = centered_rect(26, 35, size);
                let block = choose_connection(&app.config);
                frame.render_widget(Clear, area);
                frame.render_widget(block, area);
        }
        _ => {}
    }
}

fn help_dialog<'a>() -> Paragraph<'a> {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let gray = Style::default().fg(Color::Gray);
    Paragraph::new(vec![
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled(
            "Transgression TUI",
            Style::default().fg(Color::LightBlue),
        )]),
        Spans::from(""),
        Spans::from(vec![Span::styled("↑ / k    ", bold), Span::styled("Prev item", gray)]),
        Spans::from(vec![Span::styled("↓ / j    ", bold), Span::styled("Next item", gray)]),
        Spans::from(vec![Span::styled("f        ", bold), Span::styled("Filter menu", gray)]),
        Spans::from(vec![Span::styled("S        ", bold), Span::styled("Sort menu", gray)]), 
        Spans::from(vec![Span::styled("space    ", bold), Span::styled("Action menu", gray)]), 
        Spans::from(vec![Span::styled("/        ", bold), Span::styled("Find next item in list", gray)]),
        Spans::from(vec![Span::styled("?        ", bold), Span::styled("Find prev item in list", gray)]),
        Spans::from(vec![Span::styled("s        ", bold), Span::styled("Search across all torrents", gray)]),
        Spans::from(vec![Span::styled("c        ", bold), Span::styled("Connection menu", gray)]), 
        Spans::from(vec![Span::styled("F1       ", bold), Span::styled("Help screen", gray)]),  
        Spans::from(vec![Span::styled("Esc      ", bold), Span::styled("Exit from all menus", gray)]),
        Spans::from(vec![Span::styled("q        ", bold), Span::styled("Quit", gray)]),
        Spans::from(""),
        Spans::from(vec![Span::raw("Configuration file: ~/.config/transg/transg-tui.json")]),
    ])
//    .alignment(Alignment::Center)
   /* .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White))
            .title("Help")
            .border_type(BorderType::Plain),
    );*/

}
fn render_details(details: &TorrentDetails) -> Table {
    let key_style = Style::default().fg(Color::LightBlue);
    let value_style = Style::default().fg(Color::Gray);
    let rows = vec![
        Row::new(vec![
            Cell::from(Span::styled("Name:", key_style)),
            Cell::from(Span::styled(details.name.clone(), value_style)),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Size:", key_style)),
            Cell::from(Span::styled(format_size(details.size_when_done as i64), value_style)),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Priority", key_style)),
            Cell::from(Span::styled(format!("{}", details.priority), value_style)),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Completed At:", key_style)),
            Cell::from(Span::styled(format_time(details.done_date), value_style)),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Upload Ratio:", key_style)),
            Cell::from(Span::styled(format!("{:.2}", details.upload_ratio), value_style)),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Location:", key_style)),
            Cell::from(Span::styled(details.download_dir.clone(), value_style)),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Hash:", key_style)),
            Cell::from(Span::styled(details.hash_string.clone(), value_style)),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Comment:", key_style)),
            Cell::from(Span::styled(details.comment.clone(), value_style)),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Error:", key_style)),
            Cell::from(Span::styled(details.error_string.clone(), value_style)),
        ]),
    ];
    Table::new(rows)
        .widths(&[Constraint::Length(17), Constraint::Length(60)])
        .column_spacing(1)
}

fn draw_tree(items: Vec<TreeItem>) -> Tree {
    Tree::new(items)
        .block(Block::default().borders(Borders::ALL).title("Files"))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (mut processor, rx) = command_processor::CommandProcessor::create();

    //let app = App::<'_>{connection_idx: current_connection, ..Default::default()};
    let app = App::default();
    processor.run(app.config.clone(), app.connection_idx);
    run_app(&mut terminal, app, rx, processor.get_sender())?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

    terminal.show_cursor()?;
    Ok(())
}

fn render_filters<'a>(
    groups: &TorrentGroupStats,
    transition: &Transition,
    mapping: &[(String, char, usize)],
    num_active: usize,
    connection: &Connection,
) -> List<'a> {
    let filters = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White))
        .title("Filters")
        .border_type(BorderType::Plain);
    let filter_items = vec![
        ("Recent".to_string(), 'R', 0),
        (format!("Active: {}", num_active), 'A', 0),
        (format!("Paused: {}", groups.num_stopped), 'P', 0),
        (format!("Checking queue: {}", groups.num_queue_checking), 'G', 7),
        (format!("Checking: {}", groups.num_checking), 'C', 0),
        (format!("Download queue: {}", groups.num_queue_down), 'Q', 9),
        (format!("Downloading: {}", groups.num_downloading), 'D', 0),
        (format!("Seeding queue: {}", groups.num_queue_up), 'U', 9),
        (format!("Seeding: {}", groups.num_seeding), 'S', 0),
        (format!("Error: {}", groups.num_error), 'E', 0),
        (format!("All: {}", groups.num_total), 'L', 1),
    ];
    let mut folders: Vec<_> = groups.folders.iter().collect();
    folders.sort();
    let mut folder_items: Vec<_> = folders
        .iter()
        .map(|f| {
            let name = process_folder(f.0, &connection.remote_base_dir);
            if transition == &Transition::Filter {
                let (_, c, i) = mapping.iter().find(|y| &y.0 == f.0).expect("exist");
                let (first, second) = utf8_split(&name, *i);
                let second: String = second.chars().skip(1).collect();
                ListItem::new(Spans::from(vec![
                    Span::raw(" "),
                    Span::styled(first, Style::default()),
                    Span::styled(
                        c.to_string(),
                        Style::default()
                            .add_modifier(Modifier::UNDERLINED)
                            .fg(Color::LightYellow),
                    ),
                    Span::styled(format!("{}: {}", second, f.1), Style::default()),
                ]))
            } else {
                ListItem::new(Spans::from(vec![Span::styled(
                    format!(" {}: {}", name, f.1),
                    Style::default(),
                )]))
            }
        })
        .collect();

    let mut items: Vec<_> = filter_items
        .iter()
        .map(|x| {
            if transition == &Transition::Filter {
                let (first, second) = utf8_split(&x.0, x.2);
                let second: String = second.chars().skip(1).collect();

                ListItem::new(Spans::from(vec![
                    Span::raw(" "),
                    Span::styled(first, Style::default()),
                    Span::styled(
                        x.1.to_string(),
                        Style::default()
                            .add_modifier(Modifier::UNDERLINED)
                            .fg(Color::LightYellow),
                    ),
                    Span::styled(second, Style::default()),
                ]))
            } else {
                ListItem::new(Spans::from(vec![Span::raw(" "), Span::styled(x.0.clone(), Style::default())]))
            }
        })
        .collect();

    items.push(ListItem::new("────────────────────────".to_string()));
    items.append(&mut folder_items);

    let list = List::new(items).block(filters).highlight_style(
        Style::default()
            .bg(Color::Yellow)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    );

    list
}

fn render_main_table<'a>(
    _filter_state: &ListState,
    _groups: &TorrentGroupStats,
    torrents: &[TorrentInfo],
) -> Table<'a> {
    let rows: Vec<_> = torrents
        .iter()
        .map(|x| {
            Row::new(vec![
                Cell::from(Span::raw(format_status(x.status, x.error))),
                Cell::from(Span::raw(x.name.clone())),
                Cell::from(Span::raw(format_percent_done(x.percent_done))),
                Cell::from(Span::raw(format_eta(x.eta))),
                Cell::from(Span::raw(format_size(x.size_when_done))),
                Cell::from(Span::raw(format_download_speed(x.rate_upload, true))),
                Cell::from(Span::raw(format_download_speed(x.rate_download, true))),
                Cell::from(Span::raw(format_size(x.uploaded_ever))),
            ])
        })
        .collect();
    let pet_detail = Table::new(rows)
        .highlight_style(
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .header(Row::new(vec![
            Cell::from(Span::styled(" ", Style::default().add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled("Name", Style::default().add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled("Done", Style::default().add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled("Eta", Style::default().add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled("Size", Style::default().add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled("Up", Style::default().add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled("Down", Style::default().add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled("Uploaded", Style::default().add_modifier(Modifier::BOLD))),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White))
                .title("Darlings")
                .border_type(BorderType::Plain),
        )
        .widths(&[
            Constraint::Min(3),
            Constraint::Percentage(40),
            Constraint::Percentage(5),
            Constraint::Percentage(10),
            Constraint::Percentage(10),
            Constraint::Percentage(10),
            Constraint::Percentage(10),
            Constraint::Percentage(10),
            Constraint::Percentage(10),
        ]);

    pet_detail
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

fn action_menu(actions: &[Action]) -> List {
    let mut xs: Vec<(&str, &str)> = actions.iter().map(|x| (x.shortcut.as_str(), x.description.as_str())).collect(); 
    
    
    let mut ys = vec![
        ("", "───"),
        ("s", "Start"),
        ("S", "Start Now"),
        ("p", "Pause"),
        ("v", "Verify"),
        ("m", "Move"),
        ("x", "Remove"),
        ("X", "Remove with data"),
        ("", "───"),
        ("k", "Queue Up"),
        ("j", "Queue Down"),
        ("K", "Queue Top"),
        ("J", "Queue Bottom"),
    ];
    xs.append(&mut ys);
    let items: Vec<_> = xs
        .iter()
        .map(|x| {
            let desc = if x.0.is_empty() {
                  "──────────────────────────────────".to_string()
            } else {
               "    ".to_owned() + x.1
            };
            ListItem::new(Spans::from(vec![
                if x.0.is_empty() { Span::raw("─") } else { Span::raw(" ") },
                
                Span::styled(
                    x.0,
                    Style::default()
                        .add_modifier(Modifier::UNDERLINED)
                        .fg(Color::LightYellow),
                ),
                Span::styled(desc, Style::default()),
            ]))
        })
        .collect();
    List::new(items)
}
fn error_dialog<'a>(msg: &'a str, details: &'a str) -> Paragraph<'a> {
    let mut lines = vec![
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled("Bloody hell!", Style::default().fg(Color::Red))]),
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled(msg, Style::default().fg(Color::Red))]),
    ];
    for l in details.split('\n') {
        lines.push(Spans::from(vec![Span::styled(l, Style::default().fg(Color::Gray))]));
    }
    let message = Paragraph::new(lines)
    .wrap(Wrap { trim: false })
    .block(Block::default().title("Aarrgh!").borders(Borders::ALL).border_style(Style::default().fg(Color::Red)));
    message
}

fn choose_sort_dialog<'a>() -> Paragraph<'a> {
    let key_style = Style::default()
                        .add_modifier(Modifier::UNDERLINED)
                        .fg(Color::LightYellow);
    let lines = vec![
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled(" Choose sort function:", Style::default())]),
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::raw(" By "), Span::styled("d",key_style), Span::raw("ate added (default)")]),
        Spans::from(vec![Span::raw(" By "), Span::styled("u",key_style), Span::raw("ploaded total")]),
        Spans::from(vec![Span::raw(" By "), Span::styled("s",key_style), Span::raw("ize")]),
        Spans::from(vec![Span::raw(" By "), Span::styled("r",key_style), Span::raw("atio")]),
    ];
    let message = Paragraph::new(lines)
    .block(Block::default().title("Sort").borders(Borders::ALL).border_style(Style::default()));
    message
}

fn choose_connection<'a>(config: &Config) -> Paragraph<'a> {
    let key_style = Style::default()
                        .add_modifier(Modifier::UNDERLINED)
                        .fg(Color::LightYellow);
    let mut lines = vec![
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled(" Choose connection:", Style::default())]),
        Spans::from(vec![Span::raw("")]),
    ];
    for (i,x) in config.connections.iter().enumerate() {
       lines.push(Spans::from(vec![
            Span::raw(" "), Span::styled((i+1).to_string(), key_style), Span::raw("   "), Span::raw(x.name.clone())]),
       );
    }
    let message = Paragraph::new(lines)
    .block(Block::default().title("Connections").borders(Borders::ALL).border_style(Style::default()));
    message
}

fn delete_confirmation_dialog(with_data: bool, name: &str) -> Paragraph {
    let block = Block::default().title("Confirm").borders(Borders::ALL);
    let message = Paragraph::new(Spans::from(vec![
        Span::styled("Sure to remove '", Style::default()),
        Span::styled(name, Style::default().fg(Color::Gray)),
        Span::styled("'", Style::default()),
        if with_data {
            Span::styled(" with all its data?", Style::default().fg(Color::Red))
        } else {
            Span::raw("?")
        },
        Span::styled("[y/n]", Style::default()),
    ]))
    .wrap(Wrap { trim: false })
    .block(block);
    message
}

fn move_dialog<B: Backend>(frame: &mut Frame<B>, name: &str, folders: &[(String, char, usize)], connection: &Connection) {
    let size = frame.size();
    let title = Paragraph::new(Spans::from(vec![Span::styled(name, Style::default().fg(Color::Gray))]))
        .wrap(Wrap { trim: false });

    let items: Vec<_> = folders
        .iter()
        .map(|x| {
            let name = process_folder(&x.0, &connection.remote_base_dir);
            let (first, second) = utf8_split(&name, x.2);
            let second: String = second.chars().skip(1).collect();

            ListItem::new(Spans::from(vec![
                Span::styled(first, Style::default()),
                Span::styled(
                    x.1.to_string(),
                    Style::default()
                        .add_modifier(Modifier::UNDERLINED)
                        .fg(Color::LightYellow),
                ),
                Span::styled(second, Style::default()),
            ]))
        })
        .collect();

    let folder_list = List::new(items);
    let block = Block::default().title("Move").borders(Borders::ALL);
    let area = centered_rect(42, 38, size);
    let vert_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(15)].as_ref())
        .split(block.inner(area));

    let area = centered_rect(46, 38, size);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(title, vert_layout[0]);
    frame.render_widget(folder_list, vert_layout[1]);
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
