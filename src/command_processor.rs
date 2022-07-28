use crate::config::Config;
use crate::notification_utils::notify;
use crate::transmission::{FreeSpace, SessionStats, TorrentAdd, TorrentDetails, TransmissionClient};
use crate::utils::build_tree;
use crossterm::event::{self, KeyEvent};
use lazy_static::lazy_static;
use procfs::process::Process;
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
        u64,
        Box<Option<TorrentDetails>>
    ),
    Details(Box<TorrentDetails>),
    Input(KeyEvent),
    UiTick,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum TorrentCmd {
    Tick(u64),
    UiTick(),
    OpenDlDir(i64),
    OpenDlTerm(i64),
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
    Input(KeyEvent),
    Move(Vec<i64>, String, bool),
    AddTorrent(Option<String>, Option<String>, Option<String>, bool), // download dir, filename, metainfo, start_paused
                                                                      //PoisonPill()
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
        let (sender, receiver) = mpsc::channel(2048);
        let (update_sender, update_receiver) = mpsc::channel(2048);
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

    //  pub fn stop(&self) {
    //    self.sender.blocking_send(TorrentCmd::PoisonPill()).expect("can't stop..");
    //}

    pub fn run(&mut self, config: Config, ping: bool, notify_on_add: bool) {
        let sender = self.sender.clone();

        let update_sender = self.update_sender.clone();
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

                    if event::poll(timeout).expect("poll works") {
                        if let event::Event::Key(key) = event::read().expect("can read events") {
                            sender.send(TorrentCmd::Input(key)).await.expect("can send events");
                        }
                    }

                    if last_tick.elapsed() >= tick_rate {
                        sender.send(TorrentCmd::UiTick()).await.expect("send");
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

        let transmission_url = config.connection_string.to_string();
        let remote_base_dir = config.remote_base_dir.to_string();
        std::thread::spawn(move || {
            let rt = Runtime::new().expect("can't create runtime");
            rt.block_on(async move {
                let client = TransmissionClient::new(&transmission_url);
                if ping {
                    let response = client.get_all_torrents(&TORRENT_INFO_FIELDS).await.expect("oops1");
                    let ts = response.get("arguments").unwrap().get("torrents").unwrap().to_owned();
                    update_sender.send(TorrentUpdate::Full(ts)).await.expect("blah");
                }
                let mut details_id: Option<i64> = None;
                loop {
                    // should move into async
                    let cmd = receiver.recv().await.expect("probably ticker thread panicked");

                    match cmd {
                        TorrentCmd::Input(key_event) => {
                            update_sender
                                .send(TorrentUpdate::Input(key_event))
                                .await
                                .expect("send events");
                        }
                        TorrentCmd::Select(maybe_id) => {
                            details_id = maybe_id;
                        }
                        TorrentCmd::GetDetails(id) => {
                            details_id = Some(id);
                            let details = client.get_torrent_details(vec![id]).await.expect("oops3"); // TODO: what if id is wrong?
                            if !details.arguments.torrents.is_empty() {
                                let res = update_sender
                                    .send(TorrentUpdate::Details(Box::new(details.arguments.torrents[0].to_owned())))
                                    .await;
                                if res.is_err() {
                                    println!("{:#?}", res.err().unwrap());
                                }
                            }
                        }
                        TorrentCmd::Tick(i) => {
                            let resp = client.get_recent_torrents(&TORRENT_INFO_FIELDS).await.expect("oops2");
                            let torrents = resp.get("arguments").unwrap().get("torrents").unwrap().to_owned();
                            let removed = resp.get("arguments").unwrap().get("removed").unwrap().to_owned();

                            //let session_stats = if i % 3 == 0 {
                                let stats = client.get_session_stats().await.expect("boo");
                                let session_stats = Some(stats.arguments);
                                //update_sender.send(TorrentUpdate::Stats(stats.arguments)).await.expect("foo");
                            //} else {
                             //   None
                            //};

                            let free_space = if i % 60 == 0 {
                                let free_space = client.get_free_space(&remote_base_dir).await.expect("brkjf");
                                Some(free_space.arguments)
                            } else {
                                None
                            };

                            let me = Process::myself().unwrap();
                            // let me_stat = me.stat().unwrap();
                            let me_mem = me.statm().unwrap();
                            let page_size = procfs::page_size().unwrap() as u64;
                            //update_sender
                            //   .send(TorrentUpdate::ClientStats { mem: (page_size * (me_mem.resident - me_mem.shared)) })
                            //  .await
                            // .expect("foo");

                            let mem = page_size * (me_mem.resident - me_mem.shared);
                            let mut maybe_details: Option<TorrentDetails> = None;
                            if let Some(id) = details_id {
                                let details = client.get_torrent_details(vec![id]).await.expect("oops3"); // TODO: what if id is wrong?
                                if !details.arguments.torrents.is_empty() {
                                     maybe_details = Some(details.arguments.torrents[0].to_owned());
                                }
                            }
                            update_sender
                                .send(TorrentUpdate::Partial(
                                    torrents,
                                    removed,
                                    i,
                                    Box::new(session_stats),
                                    free_space,
                                    mem,
                                    Box::new(maybe_details)
                                ))
                                .await
                                .expect("blah");
                            //}
                        }
                        TorrentCmd::OpenDlDir(id) => {
                            let details = client.get_torrent_details(vec![id]).await.expect("oops3"); // TODO: what if id is wrong?
                            if !details.arguments.torrents.is_empty() {
                                let location = details.arguments.torrents[0].download_dir.clone();
                                let my_loc = location.replace(&config.remote_base_dir, &config.local_base_dir);
                                let me_loc2 = my_loc.clone();
                                let tree = build_tree(&details.arguments.torrents[0].files);
                                let p = my_loc + "/" + &tree[0].path;
                                /*    if tree.len() == 1 && fs::read_dir(&p).is_ok() {
                                    std::process::Command::new("nautilus")
                                        .arg(p)
                                        .spawn()
                                        .expect("failed to spawn");
                                } else {
                                    std::process::Command::new("nautilus")
                                        .arg(me_loc2)
                                        .spawn()
                                        .expect("failed to spawn");
                                }*/
                                let l = if tree.len() == 1 && fs::read_dir(&p).is_ok() {
                                    p
                                } else {
                                    me_loc2
                                };
                                let mut cmd_builder = std::process::Command::new(config.file_manager.cmd.clone());
                                for a in &config.file_manager.args {
                                    let arg = a.replace("{location}", &l);
                                    cmd_builder.arg(&arg);
                                }
                                cmd_builder.spawn().expect("failed to spawn");
                            }
                        }
                        TorrentCmd::OpenDlTerm(id) => {
                            // TODO: refactor both into single function
                            let details = client.get_torrent_details(vec![id]).await.expect("oops3"); // TODO: what if id is wrong?
                            if !details.arguments.torrents.is_empty() {
                                let location = details.arguments.torrents[0].download_dir.clone();
                                let my_loc = location.replace(&config.remote_base_dir, &config.local_base_dir);
                                let me_loc2 = my_loc.clone();
                                let tree = build_tree(&details.arguments.torrents[0].files);
                                let p = my_loc + "/" + &tree[0].path;
                                let l = if tree.len() == 1 && fs::read_dir(&p).is_ok() {
                                    p
                                } else {
                                    me_loc2
                                };
                                let mut cmd_builder = std::process::Command::new(config.terminal.cmd.clone());
                                for a in &config.terminal.args {
                                    let arg = a.replace("{location}", &l);
                                    cmd_builder.arg(&arg);
                                }
                                cmd_builder.spawn().expect("failed to spawn");
                            }
                        }
                        TorrentCmd::QueueMoveUp(ids) => {
                            client.queue_move_up(ids).await.expect("oops3"); // TODO: proper error handling
                        }
                        TorrentCmd::QueueMoveDown(ids) => {
                            client.queue_move_down(ids).await.expect("oops3"); // TODO: proper error handling
                        }
                        TorrentCmd::QueueMoveTop(ids) => {
                            client.queue_move_top(ids).await.expect("oops3"); // TODO: proper error handling
                        }
                        TorrentCmd::QueueMoveBottom(ids) => {
                            client.queue_move_bottom(ids).await.expect("oops3");
                            // TODO: proper error handling
                        }
                        TorrentCmd::Delete(ids, delete_local_data) => {
                            client.torrent_remove(ids, delete_local_data).await.expect("oops3");
                            // TODO: proper error handling
                        }
                        TorrentCmd::Start(ids) => {
                            client.torrent_start(ids).await.expect("oops3"); // TODO: proper error handling
                        }
                        TorrentCmd::StartNow(ids) => {
                            client.torrent_start_now(ids).await.expect("oops3");
                            // TODO: proper error handling
                        }
                        TorrentCmd::Stop(ids) => {
                            client.torrent_stop(ids).await.expect("oops3"); // TODO: proper error handling
                        }
                        TorrentCmd::Verify(ids) => {
                            client.torrent_verify(ids).await.expect("oops3"); // TODO: proper error handling
                        }
                        TorrentCmd::Reannounce(ids) => {
                            client.torrent_reannounce(ids).await.expect("oops3");
                            // TODO: proper error handling
                        }
                        TorrentCmd::Move(ids, location, is_move) => {
                            client.torrent_move(ids, &location, is_move).await.expect("ooph4");
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
                            println!("adding torrent");
                            let res = client.torrent_add(&tadd).await.expect("ooph5");
                            let result = res
                                .as_object()
                                .expect("should return object")
                                .get("result")
                                .expect("must result")
                                .as_str()
                                .unwrap()
                                .to_string();
                            if result == "success" {
                                if notify_on_add {
                                    let _ = notify("Torrent Added!", "").await; // TODO: add name
                                }
                                //    let _ =  notify("Torrent Added!", "").await; // TODO: add name
                            } else {
                                if notify_on_add {
                                    let _ = notify("Error!", "").await;
                                } else {
                                    // TODO: add UI notification
                                }
                                println!("{:?}", res);
                            }
                            //println!("{:?}", res);
                        } //            TorrentCmd::PoisonPill() => {}
                        TorrentCmd::UiTick() => {
                            update_sender.send(TorrentUpdate::UiTick).await.expect("can send");
                        }
                    }
                }
            })
        });
    }
}
