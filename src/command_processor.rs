use crate::config::{Config, Connection};
use crate::transmission::{FreeSpace, Result, Session, SessionStats, TorrentAdd, TorrentDetails, TransmissionClient};
use crate::utils::build_tree;
use crossterm::event::{self, KeyEvent};
use lazy_static::lazy_static;
//use procfs::process::Process;
use std::fs;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
//use tokio::time::sleep;

#[derive(Debug)]
pub enum TorrentUpdate {
    Full(serde_json::Value),
    Partial(
        serde_json::Value,
        serde_json::Value,
        u64,
        Box<Option<SessionStats>>,
        Option<FreeSpace>,
        Box<Option<TorrentDetails>>,
    ),
    Details(Box<TorrentDetails>),
    Input(KeyEvent),
    UiTick,
    Err {
        msg: String,
        details: String,
    },
    Session(Session),
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum TorrentCmd {
    Tick(u64),
    Action(i64, usize),
    GetDetails(i64),
    Select(Option<i64>),
    QueueMoveUp(Vec<i64>),
    QueueMoveDown(Vec<i64>),
    QueueMoveTop(Vec<i64>),
    QueueMoveBottom(Vec<i64>),
    Delete(Vec<i64>, bool),
    Start(Vec<i64>),
    StartNow(Vec<i64>),
    Stop(Vec<i64>),
    Verify(Vec<i64>),
    Reannounce(Vec<i64>),
    Move(Vec<i64>, String, bool),
    AddTorrent(Option<String>, Option<String>, Option<String>, bool), // download dir, filename, metainfo, start_paused
    //PoisonPill,
    Reconnect(usize),
}

lazy_static! {
    pub static ref TORRENT_INFO_FIELDS: Vec<&'static str> = vec![
        "id",
        "name",
        "status",
        "percentDone",
        "error",
        "errorString",
        "eta",
        "queuePosition",
        "isFinished",
        "isStalled",
        "metadataPercentComplete",
        "peersConnected",
        "rateDownload",
        "rateUpload",
        "recheckProgress",
        "sizeWhenDone",
        "downloadDir",
        "uploadedEver",
        "uploadRatio",
        "addedDate"
    ];
}

pub struct CommandProcessor {
    sender: mpsc::Sender<TorrentCmd>,
    receiver: Option<mpsc::Receiver<TorrentCmd>>,
    update_sender: mpsc::Sender<TorrentUpdate>,
}

impl CommandProcessor {
    pub fn create() -> (Self, mpsc::Receiver<TorrentUpdate>) {
        let (sender, receiver) = mpsc::channel(1024);
        let (update_sender, update_receiver) = mpsc::channel(1024);
        (
            CommandProcessor {
                receiver: Some(receiver),
                sender,
                update_sender,
            },
            update_receiver,
        )
    }

    pub fn get_sender(&self) -> mpsc::Sender<TorrentCmd> {
        self.sender.clone()
    }

    pub fn run(&mut self, config: Config, connection_idx: usize) {
        let sender = self.sender.clone();
        let mut connection = config.connections[connection_idx].clone();

        let update_sender = self.update_sender.clone();
        let update_sender2 = self.update_sender.clone();
        let _receiver = std::mem::replace(&mut self.receiver, None);
        let mut receiver = _receiver.unwrap();

        let tick_rate = Duration::from_millis(200);
        let refresh_rate = Duration::from_millis(config.refresh_interval.into());
        std::thread::spawn(move || {
            let rt = Runtime::new().expect("can't create runtime");
            rt.block_on(async {
                let mut last_tick = Instant::now();
                let mut last_tick_cmd = Instant::now();
                let mut i: u64 = 0;
                loop {
                    let timeout = tick_rate
                        .checked_sub(last_tick.elapsed())
                        .unwrap_or_else(|| Duration::from_secs(0));

                    if sender.is_closed() || update_sender2.is_closed() {
                        break;
                    }

                    if event::poll(timeout).expect("poll works") {
                        if let event::Event::Key(key) = event::read().expect("can read events") {
                            let _ = update_sender2.send(TorrentUpdate::Input(key)).await;
                        }
                    }

                    if last_tick.elapsed() >= tick_rate {
                        let _ = update_sender2.send(TorrentUpdate::UiTick).await;
                        last_tick = Instant::now();
                    }

                    if last_tick_cmd.elapsed() >= refresh_rate {
                        last_tick_cmd = Instant::now();
                        if sender.send(TorrentCmd::Tick(i)).await.is_ok() {
                            i += 1;
                        }
                    }
                }
            });
        });

        std::thread::spawn(move || {
            let rt = Runtime::new().expect("can't create runtime");
            rt.block_on(async move {
                let mut client = TransmissionClient::new(&connection.url, &connection.username, &connection.password);
                // FIXME: technically incorrect, if client "comes back" after this the app will be
                // in inconsistent state..
                let _ = update_session(&client, &update_sender, &mut connection).await;
                let _ = send_full_update(&client, &update_sender).await;
                let mut details_id: Option<i64> = None;
                loop {
                    let result = update_step(
                        &mut receiver,
                        &update_sender,
                        &mut details_id,
                        &mut client,
                        &config,
                        &mut connection,
                    )
                    .await;
                    if let Err(error) = result {
                        let _ = update_sender
                            .send(TorrentUpdate::Err {
                                msg: "Communication failed".to_string(),
                                details: error.to_string(),
                            })
                            .await;
                    }
                }
            })
        });
    }
}

async fn send_full_update(client: &TransmissionClient, update_sender: &mpsc::Sender<TorrentUpdate>) -> Result<()> {
    let res = client.get_all_torrents(&TORRENT_INFO_FIELDS).await;
    if let Err(error) = res {
        let _ = update_sender
            .send(TorrentUpdate::Err {
                msg: "Can't connect to transmission!".to_string(),
                details: format!("Please, check connection string and restart the app:\n\n{}", error),
            })
            .await;
    } else {
        let response = res.unwrap();
        let ts = response.get("arguments").unwrap().get("torrents").unwrap().to_owned();
        let _ = update_sender.send(TorrentUpdate::Full(ts)).await;
    }
    Ok(())
}

// a bit tricky config synchronization.., still better then Rc<Mutex>..
async fn update_session(
    client: &TransmissionClient,
    update_sender: &mpsc::Sender<TorrentUpdate>,
    connection: &mut Connection,
) -> Result<()> {
    let res = client.get_session().await;
    if let Err(error) = res {
        let _ = update_sender
            .send(TorrentUpdate::Err {
                msg: "Can't connect to transmission!".to_string(),
                details: format!("Please, check connection string and restart the app:\n\n{}", error),
            })
            .await;
    } else {
        let response = res.unwrap();
        if connection.download_dir.is_empty() {
            connection.download_dir = response.arguments.download_dir.clone();
        }
        let _ = update_sender.send(TorrentUpdate::Session(response.arguments)).await;
    }
    Ok(())
}

async fn update_step(
    receiver: &mut mpsc::Receiver<TorrentCmd>,
    update_sender: &mpsc::Sender<TorrentUpdate>,
    details_id: &mut Option<i64>,
    client: &mut TransmissionClient,
    config: &Config,
    connection: &mut Connection,
) -> Result<()> {
    let cmd = receiver.recv().await.expect("hmm, need to handle this error");

    match cmd {
        TorrentCmd::Select(maybe_id) => {
            *details_id = maybe_id;
        }
        TorrentCmd::Reconnect(idx) => {
            *connection = config.connections[idx].clone();
            *client = TransmissionClient::new(&connection.url, &connection.username, &connection.password);
            let _ = update_session(client, update_sender, connection).await;
            let _ = send_full_update(client, update_sender).await;
            *details_id = None;
        }
        TorrentCmd::GetDetails(id) => {
            *details_id = Some(id);
            let details = client.get_torrent_details(vec![id]).await?; // TODO: what if id is wrong?
            if !details.arguments.torrents.is_empty() {
                update_sender
                    .send(TorrentUpdate::Details(Box::new(
                        details.arguments.torrents[0].to_owned(),
                    )))
                    .await?;
            }
        }
        TorrentCmd::Tick(i) => {
            let resp = client.get_recent_torrents(&TORRENT_INFO_FIELDS).await?;
            let torrents = resp.get("arguments").unwrap().get("torrents").unwrap().to_owned();
            let removed = resp.get("arguments").unwrap().get("removed").unwrap().to_owned();

            let stats = client.get_session_stats().await?;
            let session_stats = Some(stats.arguments);

            let free_space = if i % 60 == 0 {
                let free_space = client.get_free_space(&connection.download_dir).await?;
                    Some(free_space.arguments)
            } else {
                None
            };

            //let me = Process::myself().unwrap();
            //let me_mem = me.statm().unwrap();
            //let page_size = procfs::page_size().unwrap() as u64;

            //let mem = page_size * (me_mem.resident - me_mem.shared);
            let mut maybe_details: Option<TorrentDetails> = None;
            if let Some(id) = details_id {
                let details = client.get_torrent_details(vec![*id]).await?; // TODO: what if id is wrong?
                if !details.arguments.torrents.is_empty() {
                    maybe_details = Some(details.arguments.torrents[0].to_owned());
                }
            }
            let _ = update_sender
                .send(TorrentUpdate::Partial(
                    torrents,
                    removed,
                    i,
                    Box::new(session_stats),
                    free_space,
                    Box::new(maybe_details),
                ))
                .await;
        }
        TorrentCmd::Action(id, idx) => {
            let details = client.get_torrent_details(vec![id]).await?; // TODO: what if id is wrong?
            if !details.arguments.torrents.is_empty() {
                let torrent = &details.arguments.torrents[0];
                let location = if !connection.local_download_dir.is_empty() {
                    torrent
                        .download_dir
                        .replace(&connection.download_dir, &connection.local_download_dir)
                } else {
                    torrent.download_dir.clone()
                };
                let tree = build_tree(&details.arguments.torrents[0].files);
                let p = location.clone() + "/" + &tree[0].path;

                let l = if tree.len() == 1 && fs::read_dir(&p).is_ok() {
                    p
                } else {
                    location
                };
                let action = config.actions.get(idx).expect("Wrong action index!");
                let mut cmd_builder = std::process::Command::new(action.cmd.clone());
                for a in &action.args {
                    let arg = a
                        .replace("{location}", &l)
                        .replace("{id}", &torrent.id.to_string())
                        .replace("{hash}", &torrent.hash_string)
                        .replace("{download_dir}", &torrent.download_dir)
                        .replace("{name}", &torrent.name);
                    cmd_builder.arg(&arg);
                }
                cmd_builder.spawn()?; // TODO: differentiate between different kind of errors
            }
        }
        TorrentCmd::QueueMoveUp(ids) => {
            client.queue_move_up(ids).await?; // TODO: proper error handling
        }
        TorrentCmd::QueueMoveDown(ids) => {
            client.queue_move_down(ids).await?; // TODO: proper error handling
        }
        TorrentCmd::QueueMoveTop(ids) => {
            client.queue_move_top(ids).await?; // TODO: proper error handling
        }
        TorrentCmd::QueueMoveBottom(ids) => {
            client.queue_move_bottom(ids).await?;
        }
        TorrentCmd::Delete(ids, delete_local_data) => {
            client.torrent_remove(ids, delete_local_data).await?;
        }
        TorrentCmd::Start(ids) => {
            client.torrent_start(ids).await?; // TODO: proper error handling
        }
        TorrentCmd::StartNow(ids) => {
            client.torrent_start_now(ids).await?;
        }
        TorrentCmd::Stop(ids) => {
            client.torrent_stop(ids).await?; // TODO: proper error handling
        }
        TorrentCmd::Verify(ids) => {
            client.torrent_verify(ids).await?; // TODO: proper error handling
        }
        TorrentCmd::Reannounce(ids) => {
            client.torrent_reannounce(ids).await?;
        }
        TorrentCmd::Move(ids, location, is_move) => {
            client.torrent_move(ids, &location, is_move).await?;
        }
        TorrentCmd::AddTorrent(download_dir, filename, metainfo, paused) => {
            let tadd = TorrentAdd {
                cookies: None,
                bandwith_priority: None,
                download_dir,
                filename,
                metainfo,
                files_unwanted: None,
                files_wanted: None,
                labels: None,
                paused: Some(paused),
                peer_limit: None,
                priority_high: None,
                priority_low: None,
                priority_normal: None,
            };
            let res = client.torrent_add(&tadd).await?;
            let _ = res
                .as_object()
                .expect("should return object")
                .get("result")
                .expect("must result")
                .as_str()
                .unwrap()
                .to_string();
        }
    };
    Ok(())
}
