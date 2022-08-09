use crate::config::{Action, Config, Connection, TrafficMonitorOptions};
use crate::torrent_stats::TorrentGroupStats;
use crate::transmission::{TorrentDetails, TorrentInfo};
use tui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Sparkline, Table, Wrap,
    },
    Frame,
};

use crate::utils::{
    format_download_speed, format_eta, format_percent_done, format_size, format_status, format_time, process_folder,
    utf8_split,
};
use tui_tree_widget::{Tree, TreeItem};

use crate::{App, Transition};

pub fn ui<B: Backend>(frame: &mut Frame<B>, app: &mut App) {
    let size = frame.size();

    // FIXME: hide bandwith monitor
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(if app.config.traffic_monitor == TrafficMonitorOptions::None {
                    0
                } else {
                    3
                }),
                Constraint::Min(2),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

    let status = Paragraph::new(Spans::from(vec![
        //Span::styled(format!("W: {}, H: {} ", frame.size().width, frame.size().height), Style::default()),
        Span::styled(
            format!("🔨 {}", app.config.connections[app.connection_idx].name),
            Style::default(),
        ),
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
        .block(Block::default().borders(Borders::BOTTOM))
        .data(&app.upload_data)
        .style(Style::default());
    if app.config.traffic_monitor != TrafficMonitorOptions::None {
        frame.render_widget(sparkline, chunks[0]);
    }

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
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(chunks[1]);
            if let Some(details) = &app.details {
                let details_frame = render_details(details);

                let area = centered_rect(90, 90, pets_chunks[0]);
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
                move_dialog(
                    frame,
                    &x.name,
                    &app.folder_mapping,
                    &app.config.connections[app.connection_idx],
                );
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

fn render_main_table<'a>(
    _filter_state: &ListState,
    _groups: &TorrentGroupStats,
    torrents: &[TorrentInfo],
) -> Table<'a> {
    let rows: Vec<_> = torrents
        .iter()
        .map(|x| {
            Row::new(vec![
                Cell::from(Span::raw(format_status(&x.status, x.error))),
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
            let name = process_folder(f.0, &connection.download_dir);
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
                ListItem::new(Spans::from(vec![
                    Span::raw(" "),
                    Span::styled(x.0.clone(), Style::default()),
                ]))
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

fn action_menu(actions: &[Action]) -> List {
    let mut xs: Vec<(&str, &str)> = actions
        .iter()
        .map(|x| (x.shortcut.as_str(), x.description.as_str()))
        .collect();

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
                if x.0.is_empty() {
                    Span::raw("─")
                } else {
                    Span::raw(" ")
                },
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
    let message = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .title("Aarrgh!")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red)),
    );
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
        Spans::from(vec![
            Span::raw(" By "),
            Span::styled("d", key_style),
            Span::raw("ate added (default)"),
        ]),
        Spans::from(vec![
            Span::raw(" By "),
            Span::styled("u", key_style),
            Span::raw("ploaded total"),
        ]),
        Spans::from(vec![Span::raw(" By "), Span::styled("s", key_style), Span::raw("ize")]),
        Spans::from(vec![Span::raw(" By "), Span::styled("r", key_style), Span::raw("atio")]),
    ];
    let message = Paragraph::new(lines).block(
        Block::default()
            .title("Sort")
            .borders(Borders::ALL)
            .border_style(Style::default()),
    );
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
    for (i, x) in config.connections.iter().enumerate() {
        lines.push(Spans::from(vec![
            Span::raw(" "),
            Span::styled((i + 1).to_string(), key_style),
            Span::raw("   "),
            Span::raw(x.name.clone()),
        ]));
    }
    let message = Paragraph::new(lines).block(
        Block::default()
            .title("Connections")
            .borders(Borders::ALL)
            .border_style(Style::default()),
    );
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
fn move_dialog<B: Backend>(
    frame: &mut Frame<B>,
    name: &str,
    folders: &[(String, char, usize)],
    connection: &Connection,
) {
    let size = frame.size();
    let title = Paragraph::new(Spans::from(vec![Span::styled(name, Style::default().fg(Color::Gray))]))
        .wrap(Wrap { trim: false });

    let items: Vec<_> = folders
        .iter()
        .map(|x| {
            let name = process_folder(&x.0, &connection.download_dir);
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
        Spans::from(vec![
            Span::styled("d        ", bold),
            Span::styled("Details screen", gray),
        ]),
        Spans::from(vec![
            Span::styled("/        ", bold),
            Span::styled("Find next item in list", gray),
        ]),
        Spans::from(vec![
            Span::styled("?        ", bold),
            Span::styled("Find prev item in list", gray),
        ]),
        Spans::from(vec![
            Span::styled("s        ", bold),
            Span::styled("Search across all torrents", gray),
        ]),
        Spans::from(vec![
            Span::styled("c        ", bold),
            Span::styled("Connection menu", gray),
        ]),
        Spans::from(vec![Span::styled("F1       ", bold), Span::styled("Help screen", gray)]),
        Spans::from(vec![
            Span::styled("Esc      ", bold),
            Span::styled("Exit from all menus", gray),
        ]),
        Spans::from(vec![Span::styled("q        ", bold), Span::styled("Quit", gray)]),
        Spans::from(""),
        Spans::from(vec![Span::raw("Configuration file: ~/.config/transg/transg-tui.toml")]),
    ])
}

fn format_tracker_url(s :&str) -> String {
    let s = s.strip_prefix("https://").unwrap_or(s);
    let s = s.strip_prefix("http://").unwrap_or(s);
    s.chars().take_while(|x| x != &'/').collect()
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
            Cell::from(Span::styled("First tracker:", key_style)),
            Cell::from(Span::styled(details.trackers.first().map_or(String::from(""), |t| format_tracker_url(&t.announce)), value_style)),
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
