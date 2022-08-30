use crate::config::{Action, Config, Connection, TrafficMonitorOptions, Styles};
use crate::torrent_stats::TorrentGroupStats;
use crate::transmission::{TorrentDetails, TorrentInfo};
use tui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Span, Spans},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Sparkline, Table, Wrap,
    },
    Frame,
};

use crate::utils::{
    format_download_speed, format_eta, format_percent_done, format_size, format_status, format_time, process_folder,
    utf8_split, find_file_position,
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
        //Span::styled(format!("W: {}, H: {} ", frame.size().width, frame.size().height),
        //app.styles.text),
        Span::styled(
            format!("🔨 {}", app.config.connections[app.connection_idx].name),
            app.styles.text,
        ),
        //Span::styled(" | Client Mem: ", app.styles.text),
        //Span::styled(format_size(app.memory_usage as i64), app.styles.emphasis),
        Span::styled(" | Free Space: ", app.styles.text),
        Span::styled(format_size(app.free_space as i64), app.styles.emphasis),
        Span::styled(" | Up: ", app.styles.text),
        Span::styled(
            format_download_speed(app.stats.upload_speed as i64, false),
            app.styles.emphasis,
        ),
        Span::styled(" | Down: ", app.styles.text),
        Span::styled(
            format_download_speed(app.stats.download_speed as i64, false),
            app.styles.emphasis,
        ),
        Span::styled(" ", app.styles.text),
    ]))
    .alignment(Alignment::Right)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(app.styles.text)
            .title("Stats")
            .border_type(BorderType::Plain),
    );

    if app.transition == Transition::Search || app.transition.is_find() {
        let search = Paragraph::new(Spans::from(vec![Span::styled(
            format!("Search: {}▋", app.input),
            app.styles.emphasis,
        )]))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(app.styles.text)
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
        .style(app.styles.text);
    if app.config.traffic_monitor != TrafficMonitorOptions::None {
        frame.render_widget(sparkline, chunks[0]);
    }

    match app.transition {
        Transition::Help => {
            let help = help_dialog(&app.styles);
            let width = if size.width > 120 { 30 } else { 60 };
            let area = centered_rect(width, 90, chunks[1]);
            frame.render_widget(help, area);
        }
        Transition::Files | Transition::FileAction => {
            let pets_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(chunks[1]);
            if let Some(details) = &app.details {
                let details_frame = render_details(details, &app.styles);

                let area = centered_rect(90, 90, pets_chunks[0]);
                frame.render_widget(details_frame, area);

                let block = draw_tree(app.tree_items.clone(), &app.styles);
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
                &app.styles
            );
            let main_table = render_main_table(&app.left_filter_state, &app.groups, &app.filtered_torrents, &app.styles);

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
        let block = error_dialog(msg, details, &app.styles);
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
            let list = action_menu(&app.config.actions, &app.styles);
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
                .style(app.styles.text.add_modifier(Modifier::BOLD))
                .wrap(Wrap { trim: false });

            frame.render_widget(status, vert_layout[0]);
            frame.render_widget(list, vert_layout[1]);
        }
        Transition::FileAction => {
            let block = Block::default().title("File Actions").borders(Borders::ALL);
            let area_width = if size.width > 120 { 16 } else { 38 };
            let area = centered_rect(area_width, 42, size);
            let vert_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(15)].as_ref())
                .split(block.inner(area));
            //if let Some(details) = &app.details {
            let details = file_action_menu(&app.config.file_actions, &app.styles);
            frame.render_widget(Clear, area);
            frame.render_widget(block, area);

            let title = app.details.as_ref().map_or_else(
                || "".to_string(),
                |d| {
                    if let Some(file_idx) = find_file_position(&app.tree_state.selected(), &app.tree_index) {
                        d.files.get(file_idx).map(|f| {
                    let num_chars = f.name.chars().count();
                    if num_chars > 25 {
                        let skip = num_chars - 25;
                        "\n ...".to_owned() + &f.name.chars().skip(skip).collect::<String>()
                    } else {
                        "\n ...".to_owned() + &f.name
                    }
                        }).unwrap_or_else(|| "".to_string())
                    } else {
                        "".to_string()
                    }
                },
            );
            let status = Paragraph::new(title)
                .style(app.styles.text.add_modifier(Modifier::BOLD))
                .wrap(Wrap { trim: false });

            frame.render_widget(status, vert_layout[0]);
            frame.render_widget(details, vert_layout[1]);
            //}
        }

        Transition::ConfirmRemove(with_data) => {
            if let Some(x) = app
                .main_table_state
                .selected()
                .and_then(|x| app.filtered_torrents.get(x))
            {
                let area = centered_rect(46, 15, size);
                let block = delete_confirmation_dialog(with_data, &x.name, &app.styles);
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
                    &app.styles
                );
            }
        }
        Transition::ChooseSortFunc => {
            let area = centered_rect(26, 35, size);
            let block = choose_sort_dialog(&app.styles);
            frame.render_widget(Clear, area);
            frame.render_widget(block, area);
        }
        Transition::Connection => {
            let area = centered_rect(26, 35, size);
            let block = choose_connection(&app.config, &app.styles);
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
    styles: &Styles
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
            styles.highlight
        )
        .header(Row::new(vec![
            Cell::from(Span::styled(" ", styles.emphasis)),
            Cell::from(Span::styled("Name", styles.emphasis)),
            Cell::from(Span::styled("Done", styles.emphasis)),
            Cell::from(Span::styled("Eta", styles.emphasis)),
            Cell::from(Span::styled("Size", styles.emphasis)),
            Cell::from(Span::styled("Up", styles.emphasis)),
            Cell::from(Span::styled("Down", styles.emphasis)),
            Cell::from(Span::styled("Uploaded", styles.emphasis)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(styles.text)
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
    styles: &Styles,
) -> List<'a> {
    let filters = Block::default()
        .borders(Borders::ALL)
        .style(styles.text)
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
                    Span::styled(first, styles.text),
                    Span::styled(
                        c.to_string(),
                        styles.emphasis
                            .add_modifier(Modifier::UNDERLINED)
                    ),
                    Span::styled(format!("{}: {}", second, f.1), styles.text),
                ]))
            } else {
                ListItem::new(Spans::from(vec![Span::styled(
                    format!(" {}: {}", name, f.1),
                    styles.text,
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
                    Span::styled(first, styles.text),
                    Span::styled(
                        x.1.to_string(),
                        styles.emphasis
                            .add_modifier(Modifier::UNDERLINED)
                    ),
                    Span::styled(second, styles.text),
                ]))
            } else {
                ListItem::new(Spans::from(vec![
                    Span::raw(" "),
                    Span::styled(x.0.clone(), styles.text),
                ]))
            }
        })
        .collect();

    items.push(ListItem::new("────────────────────────".to_string()));
    items.append(&mut folder_items);

    let list = List::new(items).block(filters).highlight_style(
        styles.highlight
            .add_modifier(Modifier::BOLD),
    );

    list
}

fn action_menu<'a>(actions: &'a [Action], styles: &Styles) -> List<'a> {
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
                    styles.emphasis
                        .add_modifier(Modifier::UNDERLINED)
                ),
                Span::styled(desc, styles.text),
            ]))
        })
        .collect();
    List::new(items)
}

fn file_action_menu<'a>(actions: &'a [Action], styles: &'a Styles) -> List<'a> {
    let xs: Vec<(&str, &str)> = actions
        .iter()
        .map(|x| (x.shortcut.as_str(), x.description.as_str()))
        .collect();

    /*let mut ys = vec![
        ("", "───"),
        ("+", "Download this file"),
        ("-", "Skip"),
        ("r", "Rename"),
        ("l", "Low Priority"),
        ("m", "Medium Priority"),
        ("h", "High Priority"),
    ];
    xs.append(&mut ys);*/
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
                    styles.emphasis
                        .add_modifier(Modifier::UNDERLINED)
                ),
                Span::styled(desc, styles.text),
            ]))
        })
        .collect();
    List::new(items)
}

fn error_dialog<'a>(msg: &'a str, details: &'a str, styles: &Styles) -> Paragraph<'a> {
    let mut lines = vec![
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled("Bloody hell!", styles.error_text)]),
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled(msg, styles.error_text)]),
    ];
    for l in details.split('\n') {
        lines.push(Spans::from(vec![Span::styled(l, styles.blend_in)]));
    }
    let message = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .title("Aarrgh!")
            .borders(Borders::ALL)
            .border_style(styles.error_text),
    );
    message
}

fn choose_sort_dialog(styles: &Styles) -> Paragraph {
    let key_style = styles.emphasis
        .add_modifier(Modifier::UNDERLINED);
    let lines = vec![
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled(" Choose sort function:", styles.text)]),
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
            .border_style(styles.text),
    );
    message
}

fn choose_connection<'a>(config: &Config, styles: &'a Styles) -> Paragraph<'a> {
    let key_style = styles.emphasis
        .add_modifier(Modifier::UNDERLINED);
    let mut lines = vec![
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled(" Choose connection:", styles.text)]),
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
            .border_style(styles.text),
    );
    message
}

fn delete_confirmation_dialog<'a>(with_data: bool, name: &'a str, styles: &Styles) -> Paragraph<'a> {
    let block = Block::default().title("Confirm").borders(Borders::ALL);
    let message = Paragraph::new(Spans::from(vec![
        Span::styled("Sure to remove '", styles.text),
        Span::styled(name, styles.blend_in),
        Span::styled("'", styles.text),
        if with_data {
            Span::styled(" with all its data?", styles.error_text)
        } else {
            Span::raw("?")
        },
        Span::styled("[y/n]", styles.text),
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
    styles: &Styles
) {
    let size = frame.size();
    let title = Paragraph::new(Spans::from(vec![Span::styled(name, styles.blend_in)]))
        .wrap(Wrap { trim: false });

    let items: Vec<_> = folders
        .iter()
        .map(|x| {
            let name = process_folder(&x.0, &connection.download_dir);
            let (first, second) = utf8_split(&name, x.2);
            let second: String = second.chars().skip(1).collect();

            ListItem::new(Spans::from(vec![
                Span::styled(first, styles.text),
                Span::styled(
                    x.1.to_string(),
                    styles.emphasis
                        .add_modifier(Modifier::UNDERLINED)
                ),
                Span::styled(second, styles.text),
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

fn help_dialog<'a>(styles: &Styles) -> Paragraph<'a> {
    let bold = styles.bold;
    let gray = styles.blend_in;
    Paragraph::new(vec![
        Spans::from(vec![Span::raw("")]),
        Spans::from(vec![Span::styled(
            "Transgression TUI",
            styles.details_emphasis,
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

fn extract_domain_name(s: &str) -> String {
    s.chars().take_while(|x| x != &'/').collect()
}
fn format_tracker_url(s: &str) -> String {
    s.strip_prefix("https://").map(extract_domain_name)
        .or_else(|| s.strip_prefix("http://").map(extract_domain_name))
        .or_else(|| s.strip_prefix("udp://").map(extract_domain_name))
        .unwrap_or_else(|| "".to_string())
}
fn render_details<'a>(details: &'a TorrentDetails, styles: &Styles) -> Table<'a> {
    let key_style = styles.details_emphasis;
    let value_style = styles.blend_in;
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

fn draw_tree<'a>(items: Vec<TreeItem<'a>>, styles: &Styles) -> Tree<'a> {
    Tree::new(items)
        .block(Block::default().borders(Borders::ALL).title("Files"))
        .highlight_style(
            styles.details_highlight
        )
        //.highlight_symbol(">> ")
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
