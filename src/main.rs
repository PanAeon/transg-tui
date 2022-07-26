mod command_processor;
mod config;
mod notification_utils;
mod torrent_stats;
mod transmission;
mod utils;

use std::{collections::HashMap, io};
use binary_heap_plus::BinaryHeap;
use command_processor::{TorrentUpdate, TorrentCmd};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use thiserror::Error;
use tokio::sync::mpsc::{Sender, Receiver};
use torrent_stats::{update_torrent_stats, TorrentGroupStats, VERIFYING, DOWNLOADING, SEED_QUEUED};
use transmission::{SessionStats, TorrentInfo, TorrentDetails};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap, Sparkline,
    },
    Frame, Terminal,
};
use tui_tree_widget::{Tree, TreeState, TreeItem, get_identifier_without_leaf, flatten};
use utils::{format_download_speed, format_eta, format_percent_done, format_size, format_status, process_folder, STOPPED, VERIFY_QUEUED, DOWN_QUEUED, SEEDING, build_file_tree};


#[derive(Error, Debug)]
pub enum Error {
    #[error("error reading the DB file: {0}")]
    ReadDBError(#[from] io::Error),
    #[error("error parsing the DB file: {0}")]
    ParseDBError(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
  ByStatus(i64),
  ByDirectory(String),
  Recent,
  Active,
  All,
  Search(String),
  Error
}

#[derive(Clone, Debug, PartialEq)]
pub enum Transition {
    MainScreen,
    Action,
    Filter,
    Search,
    ConfirmRemove(bool),
    Move,
    Files
}

#[derive(Copy, Clone, Debug)]
pub enum MenuItem {
    Pets,
}

pub fn calculate_folder_keys(app: &mut App, skip_folder: Option<String>) {
    let sk_folder = skip_folder.unwrap_or_else(|| "".to_string());
    let mut folder_items: Vec<String> = app.groups.folders
        .iter()
        .filter(|x| x.0 != &sk_folder)
        .map(|x|  x.0.clone())
        .collect();
    folder_items.sort();

    let mut mappings: Vec<(String, char, usize)> = vec![];

    folder_items
        .iter()
        .for_each(|x| {
            let name = process_folder(x);

            let (i,c) = name.chars().enumerate().find(|x| !mappings.iter().any(|y| y.1 == x.1)).expect("unique");
            mappings.push((x.to_string(), c, i));
        });
    app.folder_mapping = mappings;
}

pub struct App {
    pub transition: Transition,
    pub active_menu_item: MenuItem,
    pub left_filter_state: ListState,
    pub main_table_state: TableState,
    pub memory_usage: u64,
    pub torrents: HashMap<i64, TorrentInfo>,
    pub filtered_torrents: Vec<TorrentInfo>,
  //  pub filtered_torrents: Vec<i64>, strange, no use..?
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
//    pub tree_items: Vec<TreeItem<'a>>
}

impl Default for App {
    fn default() -> Self {
        let active_menu_item = MenuItem::Pets;
        let left_filter_state = ListState::default();
        let main_table_state = TableState::default();
        let memory_usage: u64 = 0;
        let torrents: HashMap<i64, TorrentInfo> = HashMap::new();
        let filtered_torrents: Vec<TorrentInfo> = vec![]; // now I need to filter efficiently... aah
        let free_space: u64 = 0;
        let stats: SessionStats = SessionStats::empty();
        let groups: TorrentGroupStats = TorrentGroupStats::empty();

        App {
            transition: Transition::MainScreen,
            active_menu_item,
            left_filter_state,
            main_table_state,
            memory_usage,
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
            details: None
        }
    }
}
fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App, mut rx: Receiver<TorrentUpdate>, sender: Sender<TorrentCmd>) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        match rx.blocking_recv() {
            Some(TorrentUpdate::UiTick) => {}
            Some(TorrentUpdate::Input(event)) => match event.code {
                KeyCode::Char('q') => {
                    break Ok(());
                }
                _ => {
                    match app.transition {
                        Transition::MainScreen => {
                            match event.code {
                                KeyCode::Char(' ') => {
                                    // TODO: if something is selected
                                    app.transition = Transition::Action;
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if let Some(selected) = app.main_table_state.selected() {
                                        let amount_pets = app.filtered_torrents.len();
                                        if selected >= amount_pets - 1 {
                                            app.main_table_state.select(Some(0));
                                            app.selected = Some(app.filtered_torrents[0].clone());
                                            sender.blocking_send(TorrentCmd::Select(Some(app.filtered_torrents[0].id))).expect("foo");
                                        } else {
                                            app.main_table_state.select(Some(selected + 1));
                                            app.selected = Some(app.filtered_torrents[selected + 1].clone());
                                            sender.blocking_send(TorrentCmd::Select(Some(app.filtered_torrents[selected + 1].id))).expect("foo");
                                        }
                                    } else {
                                        app.main_table_state.select(Some(0));
                                        app.selected = Some(app.filtered_torrents[0].clone());
                                        sender.blocking_send(TorrentCmd::Select(Some(app.filtered_torrents[0].id))).expect("foo");
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
                                            sender.blocking_send(TorrentCmd::Select(Some(app.filtered_torrents[selected - 1].id))).expect("foo");
                                        } else {
                                            app.main_table_state.select(Some(amount_pets - 1));
                                            app.selected = Some(app.filtered_torrents[amount_pets - 1].clone());
                                            sender.blocking_send(TorrentCmd::Select(Some(app.filtered_torrents[amount_pets - 1].id))).expect("foo");
                                        }
                                    } else {
                                        app.main_table_state.select(Some(0));
                                        app.selected = Some(app.filtered_torrents[0].clone());
                                        sender.blocking_send(TorrentCmd::Select(Some(app.filtered_torrents[0].id))).expect("foo");
                                    }
                                    } else {
                                        app.selected = None;
                                        sender.blocking_send(TorrentCmd::Select(None)).expect("foo");
                                    }
                                }
                                KeyCode::Char('f') => {
                                    calculate_folder_keys(&mut app, None);
                                    app.transition = Transition::Filter; 
                                }
                                KeyCode::Char('s') | KeyCode::Char('/') => {
                                    app.transition = Transition::Search;
                                }
                                KeyCode::Char('g') => {
                                  app.transition = Transition::Files;
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
                            KeyCode::Char('o') => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                    sender.blocking_send(TorrentCmd::OpenDlDir(x.id)).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('t') => { // TODO: add in-place terminal?
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                    sender.blocking_send(TorrentCmd::OpenDlTerm(x.id)).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('s') => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                    sender.blocking_send(TorrentCmd::Start(vec![x.id])).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('S') => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                    sender.blocking_send(TorrentCmd::StartNow(vec![x.id])).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('p') => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                    sender.blocking_send(TorrentCmd::Stop(vec![x.id])).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('v') => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                    sender.blocking_send(TorrentCmd::Verify(vec![x.id])).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('m') => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)).map(|x| x.download_dir.clone()) {
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
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                  sender.blocking_send(TorrentCmd::QueueMoveUp(vec![x.id])).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('j') => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                  sender.blocking_send(TorrentCmd::QueueMoveDown(vec![x.id])).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('K') => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                  sender.blocking_send(TorrentCmd::QueueMoveTop(vec![x.id])).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('J') => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                  sender.blocking_send(TorrentCmd::QueueMoveBottom(vec![x.id])).expect("should send");
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
                                    let idx = 12 + app.folder_mapping.iter().enumerate().find(|y| y.1.0 == x.0).map(|x| x.0).unwrap_or(0); 
                                    app.left_filter_state.select(Some(idx));
                                    app.transition = Transition::MainScreen;
                                    app.filtered_torrents = app.torrents.values().filter(|y| y.download_dir == x.0).cloned().collect();
                                    app.filtered_torrents.sort_unstable_by_key(|x| -x.added_date); // TODO: this somehow adds some overhead
                                } else {
/*

                    },
                    Filter::ByStatus(_) => {
                        if let Filter::ByStatus(s) = app.current_filter.clone() {
                        }
                    }
                    Filter::All => {
                    }
                    Filter::Active => {
                        app.filtered_torrents = xs.iter().skip(1).map(|x| TorrentInfo::new(x)).collect();
                    }
                    Filter::Recent => {
                    }
                }
*/
                                    match c {
                                        'R' => {
                                          app.current_filter = Filter::Recent;
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(0));
                                          app.filtered_torrents = most_recent_items(&app.torrents);
                        //let mut xs : Vec<_> = app.torrents.values().cloned().collect();
                        //xs.sort_unstable_by_key(|x| -x.added_date);
                        //xs.truncate(150);
                        // app.filtered_torrents = xs;
                                        }
                                        'A' => {
                                          app.current_filter = Filter::Active;
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(1));
                                        }
                                        'P' => {
                                          app.current_filter = Filter::ByStatus(STOPPED);
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(2));
                                          app.filtered_torrents = app.torrents.values().filter(|x| x.status == STOPPED).cloned().collect();
                                          app.filtered_torrents.sort_unstable_by_key(|x| -x.added_date); // TODO: this somehow adds some overhead
                                        }
                                        'L' => {
                                          app.current_filter = Filter::All;
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(10));
                                          app.filtered_torrents = app.torrents.values().cloned().collect();
                                          app.filtered_torrents.sort_unstable_by_key(|x| -x.added_date); // TODO: this somehow adds some overhead
                                        }
                                        'G' => {
                                          app.current_filter = Filter::ByStatus(VERIFY_QUEUED);
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(3));
                                          app.filtered_torrents = app.torrents.values().filter(|x| x.status == VERIFY_QUEUED).cloned().collect();
                                          app.filtered_torrents.sort_unstable_by_key(|x| -x.added_date); // TODO: this somehow adds some overhead
                                        }
                                        'C' => {
                                          app.current_filter = Filter::ByStatus(VERIFYING);
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(4));
                                          app.filtered_torrents = app.torrents.values().filter(|x| x.status == VERIFYING).cloned().collect();
                                          app.filtered_torrents.sort_unstable_by_key(|x| -x.added_date); // TODO: this somehow adds some overhead
                                        }
                                        'Q' => {
                                          app.current_filter = Filter::ByStatus(DOWN_QUEUED);
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(5));
                                          app.filtered_torrents = app.torrents.values().filter(|x| x.status == DOWN_QUEUED).cloned().collect();
                                          app.filtered_torrents.sort_unstable_by_key(|x| -x.added_date); // TODO: this somehow adds some overhead
                                        }
                                        'D' => {
                                          app.current_filter = Filter::ByStatus(DOWNLOADING);
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(6));
                                          app.filtered_torrents = app.torrents.values().filter(|x| x.status == DOWNLOADING).cloned().collect();
                                          app.filtered_torrents.sort_unstable_by_key(|x| -x.added_date); // TODO: this somehow adds some overhead
                                        }
                                        'U' => {
                                          app.current_filter = Filter::ByStatus(SEED_QUEUED);
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(7));
                                          app.filtered_torrents = app.torrents.values().filter(|x| x.status == SEED_QUEUED).cloned().collect();
                                          app.filtered_torrents.sort_unstable_by_key(|x| -x.added_date); // TODO: this somehow adds some overhead
                                        }
                                        'S' => {
                                          app.current_filter = Filter::ByStatus(SEEDING);
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(8));
                                          app.filtered_torrents = app.torrents.values().filter(|x| x.status == SEEDING).cloned().collect();
                                          app.filtered_torrents.sort_unstable_by_key(|x| -x.added_date); // TODO: this somehow adds some overhead
                                        }
                                        'E' => {
                                          app.current_filter = Filter::Error;
                                          app.filtered_torrents = app.torrents.values().filter(|x| x.error > 0).cloned().collect();
                                          app.transition = Transition::MainScreen;
                                          app.left_filter_state.select(Some(9));
                                        }
                                        _ => {}
                                    }
                                }
                                // FIXME: update filter function..
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
                                // commit search
                                app.current_filter = Filter::Search(app.input.clone());
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Backspace => {
                                app.input.pop();
                            }
                            KeyCode::Char(c) => {
                                app.input.push(c)
                            }
                            _ => {}
                        }
                        Transition::Move => match event.code {
                             KeyCode::Esc => {
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char(c) => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                    if let Some((f, _, _)) = app.folder_mapping.iter().find(|y| y.1 == c) {
                                        sender.blocking_send(TorrentCmd::Move(vec![x.id], f.to_string(), false)).expect("should send"); // TODO: move haz parameter
                                        app.transition = Transition::MainScreen;
                                    }
                                    /*if let Some(f) = app.groups.folder_keys.get(&c) {
                                    }*/
                                }
                            }
                            _ => {}

                        }
                        Transition::ConfirmRemove(with_data) => match event.code {
                            KeyCode::Char('n') | KeyCode::Char('N')  | KeyCode::Esc => {
                                app.transition = Transition::MainScreen;
                            }
                            KeyCode::Char('y') => {
                                if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
                                  sender.blocking_send(TorrentCmd::Delete(vec![x.id], with_data)).expect("should send");
                                }
                                app.transition = Transition::MainScreen;
                            }
                            _ => {}

                        }
                        Transition::Files => match event.code {
                            KeyCode::Esc  => app.transition = Transition::MainScreen,
                            KeyCode::Left => {
                                let selected = app.tree_state.selected();
        if !app.tree_state.close(&selected) {
            let (head, _) = get_identifier_without_leaf(&selected);
            app.tree_state.select(head);
        }
                            }
                KeyCode::Right => { app.tree_state.open(app.tree_state.selected());}
                KeyCode::Enter => app.tree_state.toggle(),
                KeyCode::Down => move_up_down(&mut app, true),
                KeyCode::Up => move_up_down(&mut app, false),
                _ => {}
                        }
                    }
                }
            },

            Some(TorrentUpdate::Partial(json, removed, _i, session_stats, free_space_opt, mem, details)) => {
                app.details = *details;
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
                app.memory_usage = mem;

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
                //xxs.sort_by(|a, b| a.as_array().unwrap()[0].as_i64().unwrap().cmp(&b.as_array().unwrap()[0].as_i64().unwrap()));

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
                app.groups = update_torrent_stats(&app.torrents);
                match app.current_filter.clone() { // FIXME: Error is not a status..
                    Filter::Search(text) => {
                        app.filtered_torrents = app.torrents.values().filter(|x| x.name.to_lowercase().contains(&text.to_lowercase())).cloned().collect();
                    },
                    Filter::ByDirectory(_) => {
                        if let Filter::ByDirectory(d) = app.current_filter.clone() {
                            app.filtered_torrents = app.torrents.values().filter(|x| x.download_dir == d).cloned().collect();
                        }
                    },
                    Filter::ByStatus(_) => {
                        if let Filter::ByStatus(s) = app.current_filter.clone() {
                            app.filtered_torrents = app.torrents.values().filter(|x| x.status == s).cloned().collect();
                        }
                    }
                    Filter::All => {
                        app.filtered_torrents = app.torrents.values().cloned().collect();
                    }
                    Filter::Active => {
                        app.filtered_torrents = xs.iter().skip(1).map(TorrentInfo::new).collect();
                    }
                    Filter::Recent => {
                        app.filtered_torrents = most_recent_items(&app.torrents);
                        // let mut xs : Vec<_> = app.torrents.values().cloned().collect();
                        // xs.sort_unstable_by_key(|x| -x.added_date);
                        // xs.truncate(150);
                        // app.filtered_torrents = xs;
                    }
                    Filter::Error => {
                            app.filtered_torrents = app.torrents.values().filter(|x| x.error > 0).cloned().collect();
                    }
                }
                app.filtered_torrents.sort_unstable_by_key(|x| -x.added_date); // TODO: this somehow adds some overhead
                if !app.filtered_torrents.is_empty() && app.main_table_state.selected().is_none()  {
                        app.main_table_state.select(Some(0));
                        app.selected = Some(app.filtered_torrents[0].clone());
                        sender.blocking_send(TorrentCmd::Select(Some(app.filtered_torrents[0].id))).expect("foo");
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

                        let mut xs : Vec<_> = app.torrents.values().cloned().collect();
                        xs.sort_by_key(|x| -x.added_date);
                        xs.truncate(150);
                        app.filtered_torrents = xs;
                //filtered_torrents = torrents.values().cloned().collect();

                //torrents.sort_by(|a, b| a.property_value("id").get::<i64>().expect("fkjf").cmp(&b.property_value("id").get::<i64>().expect("xx")));
                //model.remove_all();
                //model.splice(0, 0, &torrents);
                //update_torrent_stats(&model, &category_model );
            }
            Some(TorrentUpdate::Details(_details)) => {}
            None => {} // exit app, no more updates
        }
    }
}

 fn move_up_down(app: &mut App, down: bool) {
    let items = vec![
                TreeItem::new_leaf("a"),
                TreeItem::new(
                    "b",
                    vec![
                        TreeItem::new_leaf("c"),
                        TreeItem::new("d", vec![TreeItem::new_leaf("e"), TreeItem::new_leaf("f")]),
                        TreeItem::new_leaf("g"),
                    ],
                ),
                TreeItem::new_leaf("h"),
            ]; 
        let visible = flatten(&app.tree_state.get_all_opened(), &items);
        let current_identifier = app.tree_state.selected();
        let current_index = visible
            .iter()
            .position(|o| o.identifier == current_identifier);
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

impl From<MenuItem> for usize {
    fn from(input: MenuItem) -> usize {
        match input {
            MenuItem::Pets => 0,
        }
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
//                Constraint::Length(10),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);
    /*let details = Paragraph::new(Spans::from(vec![
        Span::styled("TODO!", Style::default()),
    ]))
    .style(Style::default().fg(Color::LightCyan))
    .alignment(Alignment::Left)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White))
            .title("Details")
            .border_type(BorderType::Plain),
    );

    frame.render_widget(details, chunks[2]);*/
    let status = Paragraph::new(Spans::from(vec![
            Span::styled("🔨 NAS", Style::default()),
            Span::styled(" | Client Mem: ", Style::default()),
            Span::styled(format_size(app.memory_usage as i64), Style::default().fg(Color::Yellow)),
            Span::styled(" | Free Space: ", Style::default()),
            Span::styled(format_size(app.free_space as i64), Style::default().fg(Color::Yellow)),
            Span::styled(" | Up: ", Style::default()),
            Span::styled(format_download_speed(app.stats.upload_speed as i64, false), Style::default().fg(Color::Yellow)),
            Span::styled(" | Down: ", Style::default()),
            Span::styled(format_download_speed(app.stats.download_speed as i64, false), Style::default().fg(Color::Yellow)),
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

    let search = Paragraph::new(Spans::from(vec![
        Span::styled(format!("Search: {}▋", app.input), Style::default().fg(Color::Yellow))
    ]))
    .alignment(Alignment::Left)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White))
            .title("Search")
            .border_type(BorderType::Plain),
    );

    if app.transition == Transition::Search { 
      frame.render_widget(search, chunks[2]);
    } else {
      frame.render_widget(status, chunks[2]);
    }
    /*let menu_titles = vec!["Pets", "Add", "Delete", "Quit"];

    let menu = menu_titles
        .iter()
        .map(|t| {
            let (first, rest) = t.split_at(1);
            Spans::from(vec![
                Span::styled(
                    first,
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(rest, Style::default().fg(Color::White)),
            ])
        })
        .collect();

    let tabs = Tabs::new(menu)
        .select(app.active_menu_item.into())
        .block(Block::default().title("Menu").borders(Borders::ALL))
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow))
        .divider(Span::raw("|"));

    frame.render_widget(tabs, chunks[0]);*/
    let sparkline = Sparkline::default()
        .block(
            Block::default()
                //.title("Upload rate")
                .borders(Borders::BOTTOM)
                //.border_style()
                //.border_type(BorderType::Thick),
        )
        .data(&app.upload_data)
        .style(Style::default());
    frame.render_widget(sparkline, chunks[0]);

    match app.active_menu_item {
        MenuItem::Pets => {
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
            let filters = render_filters(&app.groups, &app.transition, &app.folder_mapping, app.num_active);
            let main_table = render_main_table(&app.left_filter_state, &app.groups, &app.filtered_torrents);
            frame.render_stateful_widget(filters, pets_chunks[0], &mut app.left_filter_state);
            frame.render_stateful_widget(main_table, pets_chunks[1], &mut app.main_table_state);
        }
    }
    match app.transition {
      Transition::Action => {
        let block = Block::default().title("Actions").borders(Borders::ALL);
        let area = centered_rect(16, 38, size);
        let vert_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(2),
                Constraint::Length(15),
            ]
            .as_ref(),
        )
        .split(block.inner(area));
        let list = action_menu();
        frame.render_widget(Clear, area); //this clears out the background
        frame.render_widget(block, area); //this clears out the background

        let title = app.selected.as_ref().map_or_else(|| "".to_string(), |x| {
            if x.name.len() > 25 {
               x.name.chars().take(25).collect::<String>() + "…"
            } else {
                x.name.clone()
            }
        });
    let status = Paragraph::new(title).style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)).wrap(Wrap { trim: true });

        frame.render_widget(status, vert_layout[0]);
        frame.render_widget(list, vert_layout[1]);
    }
    Transition::ConfirmRemove(with_data) => {
        if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
            let area = centered_rect(46, 15, size);
            let block = delete_confirmation_dialog(with_data, &x.name);
            frame.render_widget(Clear, area); //this clears out the background
            frame.render_widget(block, area); //this clears out the background
        }
    }
    Transition::Move => {
        if let Some(x) = app.main_table_state.selected().and_then(|x| app.filtered_torrents.get(x)) {
            move_dialog(frame, &x.name, &app.folder_mapping);
        }
    }
    Transition::Files => {
        if let Some(details) = &app.details {
            let area = centered_rect(70, 80, size);
            let block = draw_tree(details);
            frame.render_widget(Clear, area); //this clears out the background
            frame.render_stateful_widget(block, area, &mut app.tree_state); //this clears out the background
        }
    }
    _ => {}
    }
    // restore terminal
}

fn draw_tree<'a>(details: &'a TorrentDetails) -> Tree<'a> {
    let items = build_file_tree(&details.files);
    /*let items = vec![
                TreeItem::new_leaf("a"),
                TreeItem::new(
                    "b",
                    vec![
                        TreeItem::new_leaf("c"),
                        TreeItem::new("d", vec![TreeItem::new_leaf("e"), TreeItem::new_leaf("f")]),
                        TreeItem::new_leaf("g"),
                    ],
                ),
                TreeItem::new_leaf("h"),
            ];*/

     Tree::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Tree Widget "),
                )
                .highlight_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ")
}


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut processor, rx) = command_processor::CommandProcessor::create();
    let config = config::get_or_create_config();

    processor.run(config, true, true);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::default();

    run_app(&mut terminal, app, rx, processor.get_sender())?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

    terminal.show_cursor()?;
    Ok(())
}
#[allow(dead_code)]
fn utf8_truncate(input : &mut String, maxsize: usize) {
  let mut utf8_maxsize = input.len();
  if utf8_maxsize >= maxsize {
    { let mut char_iter = input.char_indices();
    while utf8_maxsize >= maxsize {
      utf8_maxsize = match char_iter.next_back() {
        Some((index, _)) => index,
        _ => 0
      };
    } } // Extra {} wrap to limit the immutable borrow of char_indices()
    input.truncate(utf8_maxsize);
  }
}

fn utf8_split(input : &str, at: usize) -> (String, String) {
    let mut it = input.chars();
    let fst = it.by_ref().take(at).collect();
    let snd = it.collect();
    (fst, snd)
}

fn render_filters<'a>(groups: &TorrentGroupStats, transition: &Transition, mapping: &[(String, char, usize)], num_active: usize) -> List<'a> {
    let filters = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White))
        .title("Filters")
        .border_type(BorderType::Plain);
    let filter_items = vec![
        ("Recent".to_string(), 'R', 0),                     // TODO: add actually active items
        (format!("Active: {}", num_active), 'A', 0), // TODO: add actually active items
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
            let name = process_folder(f.0);
            if transition == &Transition::Filter {
            let (_,c, i) = mapping.iter().find(|y| &y.0 == f.0).expect("exist");
            let (first, second) = utf8_split(&name, *i); 
            let second: String = second.chars().skip(1).collect();
        ListItem::new(Spans::from(vec![
            Span::styled(first, Style::default()),
            Span::styled(c.to_string(),
                Style::default()
                    .add_modifier(Modifier::UNDERLINED)
                    .fg(Color::LightYellow),
            ),
            Span::styled(format!("{}: {}", second, f.1), Style::default()),
        ]))
            } else {
                  ListItem::new(Spans::from(vec![
                        Span::styled(format!("{}: {}", name, f.1), Style::default())
                  ]))

            }
        })
        .collect();
    //folder_items.sort();

    let mut items: Vec<_> = filter_items
        .iter()
        .map(|x| {
            if transition == &Transition::Filter {
            let (first, second) = utf8_split(&x.0, x.2); 
            let second: String = second.chars().skip(1).collect();

        ListItem::new(Spans::from(vec![
            Span::styled(first, Style::default()),
            Span::styled(x.1.to_string(),
                Style::default()
                    .add_modifier(Modifier::UNDERLINED)
                    .fg(Color::LightYellow),
            ),
            Span::styled(second, Style::default()),
        ]))
            } else {
               ListItem::new(Spans::from(vec![Span::styled(x.0.clone(), Style::default())]))
            }
        })
        .collect();
    
    items.push(ListItem::new(    "────────────────────────".to_string()));
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
    /*let pet_detail = Table::new(vec![Row::new(vec![
        Cell::from(Span::raw(selected_pet.id.to_string())),
        Cell::from(Span::raw(selected_pet.name)),
        Cell::from(Span::raw(selected_pet.category)),
        Cell::from(Span::raw(selected_pet.age.to_string())),
        Cell::from(Span::raw(selected_pet.created_at.to_string())),
    ])])*/
    let rows: Vec<_> = torrents
        .iter()
        //.take(40)
        .map(|x| {
            Row::new(vec![
                Cell::from(Span::raw(format_status(x.status))),
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

/// helper function to create a centered rect using up certain percentage of the available rect `r`
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

fn action_menu<'a>() -> List<'a> {
    let xs = vec![
            ("o", "    Open in file manager"),
            ("t", "    Open in terminal"),
            ("",  "──────────────────────────────────"),
            ("s", "    Start"),
            ("S", "    Start Now"),
            ("p", "    Pause"),
            ("v","    Verify"),
            ("m", "    Move"),
            ("x", "    Remove"),
            ("X", "    Remove with data"),
            ("", "──────────────────────────────────"),
            ("k", "    Queue Up"),
            ("j", "    Queue Down"),
            ("K", "    Queue Top"),
            ("J", "    Queue Bottom"),
    ];
    let items: Vec<_> = xs
        .iter() 
        .map(|x| {
        ListItem::new(Spans::from(vec![
            Span::styled(
                x.0,
                Style::default()
                    .add_modifier(Modifier::UNDERLINED)
                    .fg(Color::LightYellow),
            ),
            Span::styled(x.1, Style::default()),
        ]))
        }).collect();
    List::new(items)
}
fn delete_confirmation_dialog(with_data: bool, name: & str) -> Paragraph {
        let block = Block::default().title("Confirm").borders(Borders::ALL);
        let message = Paragraph::new(Spans::from(vec!(
            Span::styled("Sure to remove '", Style::default()),
            Span::styled(name, Style::default().fg(Color::Gray)),
            if with_data { Span::styled("' with all its data?[y/n]", Style::default().fg(Color::Red)) } else { Span::styled("'?[y/n]", Style::default()) },
            ))).wrap(Wrap { trim: false }).block(block);
        message
}

fn move_dialog<B: Backend>(frame: &mut Frame<B>, name: &str, folders: &[(String, char, usize)]) {
    let size = frame.size();
    let title = Paragraph::new(Spans::from(vec!(
            Span::styled(name, Style::default().fg(Color::Gray)),
            ))).wrap(Wrap { trim: false });

    /*let mut folder_items: Vec<_> = folders
        .iter()
        .filter(|x| x.0 != dir)
        .map(|x|  process_folder(x.0))
        .collect();
    folder_items.sort();*/

    // let mut occupied: HashSet<char> = HashSet::new();

    // probably should move to state, then use HashMap to get  correct folder..
    let items: Vec<_> = folders
        .iter()
        .map(|x| {
            let name = process_folder(&x.0);
            // better find unique character position,
            //let (i,c) = name.chars().enumerate().filter(|x| !occupied.contains(&x.1)).next().expect("unique");
            //occupied.insert(c);
            let (first, second) = utf8_split(&name, x.2); 
            let second: String = second.chars().skip(1).collect();

        ListItem::new(Spans::from(vec![
            Span::styled(first, Style::default()),
            Span::styled(x.1.to_string(),
                Style::default()
                    .add_modifier(Modifier::UNDERLINED)
                    .fg(Color::LightYellow),
            ),
            Span::styled(second, Style::default()),
        ]))
        }).collect();

    let folder_list = List::new(items);
    let block = Block::default().title("Move").borders(Borders::ALL);
        let area = centered_rect(42, 38, size);
        let vert_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(2),
                Constraint::Length(15),
            ]
            .as_ref(),
        )
        .split(block.inner(area));

        /*let title = app.selected.as_ref().map_or_else(|| "".to_string(), |x| {
            if x.name.len() > 25 {
               x.name.chars().take(25).collect::<String>() + "…"
            } else {
                x.name.clone()
            }
        });*/


        let area = centered_rect(46, 38, size);
        frame.render_widget(Clear, area); //this clears out the background
        frame.render_widget(block, area); //this clears out the background
        frame.render_widget(title, vert_layout[0]);
        frame.render_widget(folder_list, vert_layout[1]);
}

fn most_recent_items(torrents: &HashMap<i64, TorrentInfo>) -> Vec<TorrentInfo> {
    let mut heap = BinaryHeap::with_capacity_by(120, |a: &TorrentInfo, b: &TorrentInfo| b.added_date.cmp(&a.added_date));
    for x in torrents.values() {
        if heap.len() > 120 {
            heap.pop();
        }
        heap.push(x.clone());
    }
    heap.into_sorted_vec()
}
/*fn n_largest<T: PartialOrd>(array: &mut Vec<T>, n: usize) -> Vec<T> {
    let mut res = vec![];
  
    for i in 0..n { 
        let mut max1 = array[0];

        for j in 1..array.len() {
            if  array[j] >= max1 {
               max1 = array[j];
            }
        }
          
                  
        array.remove(j);
        res.push(max1)
    } 
    res 
}*/
