use serde::de::DeserializeOwned;
use serde_json::json;
use serde_json::Value;
//use std::cell::RefCell;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct TorrentInfo {
    pub id: i64,
    pub name: String,
    pub status: i64,
    pub percent_done: f64,
    pub error: i64,
    pub error_string: String,
    pub eta: i64,
    pub queue_position: i64,
    pub is_finished: bool,
    pub is_stalled: bool,
    pub metadata_percent_complete: f64,
    pub peers_connected: i64,
    pub rate_download: i64,
    pub rate_upload: i64,
    pub recheck_progress: f64,
    pub size_when_done: i64,
    pub download_dir: String,
    pub uploaded_ever: i64,
    pub upload_ratio: f64,
    pub added_date: i64,
}

impl TorrentInfo {
    pub fn new(json: &Value) -> Self {
        let xs = json.as_array().unwrap();
        if xs.len() < 20 {
            println!("js array too short");
            std::process::exit(-1);
        }
        TorrentInfo {
            id: xs[0].as_i64().unwrap(),
            name: xs[1].as_str().unwrap().to_string(),
            status: xs[2].as_i64().unwrap(),
            percent_done: xs[3].as_f64().unwrap(),
            error: xs[4].as_i64().unwrap(),
            error_string: xs[5].as_str().unwrap().to_string(),
            eta: xs[6].as_i64().unwrap(),
            queue_position: xs[7].as_i64().unwrap(),
            is_finished: xs[8].as_bool().unwrap(),
            is_stalled: xs[9].as_bool().unwrap(),
            metadata_percent_complete: xs[10].as_f64().unwrap(),
            peers_connected: xs[11].as_i64().unwrap(),
            rate_download: xs[12].as_i64().unwrap(),
            rate_upload: xs[13].as_i64().unwrap(),
            recheck_progress: xs[14].as_f64().unwrap(),
            size_when_done: xs[15].as_i64().unwrap(),
            download_dir: xs[16].as_str().unwrap().to_string(),
            uploaded_ever: xs[17].as_i64().unwrap(),
            upload_ratio: xs[18].as_f64().unwrap(),
            added_date: xs[19].as_i64().unwrap(),
        }
    }
    pub fn update(&mut self, xs: &[Value]) {
        self.status = xs[2].as_i64().unwrap();
        self.percent_done = xs[3].as_f64().unwrap();
        self.error = xs[4].as_i64().unwrap();
        self.error_string = xs[5].as_str().unwrap().to_string();
        self.eta = xs[6].as_i64().unwrap();
        self.queue_position = xs[7].as_i64().unwrap();
        self.is_finished = xs[8].as_bool().unwrap();
        self.is_stalled = xs[9].as_bool().unwrap();
        self.metadata_percent_complete = xs[10].as_f64().unwrap();
        self.peers_connected = xs[11].as_i64().unwrap();
        self.rate_download = (self.rate_download + xs[12].as_i64().unwrap()) / 2;
        self.rate_upload = (self.rate_upload + xs[13].as_i64().unwrap()) / 2;
        self.recheck_progress = xs[14].as_f64().unwrap();
        self.size_when_done = xs[15].as_i64().unwrap();
        self.download_dir = xs[16].as_str().unwrap().to_string();
        self.uploaded_ever = xs[17].as_i64().unwrap();
        self.upload_ratio = xs[18].as_f64().unwrap();
        self.added_date = xs[19].as_i64().unwrap();
    }
}

pub struct TransmissionClient {
    client: reqwest::Client,
    session_id: Mutex<String>,
    url: String,
}

#[derive(Deserialize, Debug)]
pub struct RpcResponse<T> {
    pub arguments: T,
    pub result: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FreeSpace {
    pub path: String,
    #[serde(rename = "size-bytes")]
    pub size_bytes: u64,
    //    pub total_size: u64
}

#[derive(Deserialize, Debug, Clone)]
pub struct Stats {
    #[serde(rename = "uploadedBytes")]
    pub upload_bytes: u64,
    #[serde(rename = "downloadedBytes")]
    pub download_bytes: u64,
    #[serde(rename = "filesAdded")]
    pub files_added: u64,
    #[serde(rename = "sessionCount")]
    pub session_count: u64,
    #[serde(rename = "secondsActive")]
    pub seconds_active: u64,
}

impl Stats {
    pub fn empty() -> Self {
        Stats {
            upload_bytes: 0,
            download_bytes: 0,
            files_added: 0,
            session_count: 0,
            seconds_active: 0,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SessionStats {
    #[serde(rename = "activeTorrentCount")]
    pub active_torrent_count: u64,
    #[serde(rename = "downloadSpeed")]
    pub download_speed: u64,
    #[serde(rename = "pausedTorrentCount")]
    pub paused_torrent_count: u64,
    #[serde(rename = "torrentCount")]
    pub torrent_count: u64,
    #[serde(rename = "uploadSpeed")]
    pub upload_speed: u64,
    #[serde(rename = "current-stats")]
    pub current_stats: Stats,
    #[serde(rename = "cumulative-stats")]
    pub cumulative_stats: Stats,
}

impl SessionStats {
    pub fn empty() -> Self {
        SessionStats {
            active_torrent_count: 0,
            download_speed: 0,
            paused_torrent_count: 0,
            torrent_count: 0,
            upload_speed: 0,
            current_stats: Stats::empty(),
            cumulative_stats: Stats::empty(),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct File {
    pub name: String,
    pub length: u64,
    #[serde(rename = "bytesCompleted")]
    pub bytes_completed: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FileStats {
    pub wanted: bool,
    pub priority: i8,
    #[serde(rename = "bytesCompleted")]
    pub bytes_completed: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TrackerStats {
    #[serde(rename = "leecherCount")]
    pub leecher_count: i64,
    pub id: u64,
    pub host: String,
    pub scrape: String,
    #[serde(rename = "seederCount")]
    pub seeder_count: i64,
    #[serde(rename = "lastAnnouncePeerCount")]
    pub last_announce_peer_count: u64,
    #[serde(rename = "lastAnnounceResult")]
    pub last_announce_result: String,
    #[serde(rename = "lastAnnounceTime")]
    pub last_announce_time: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Tracker {
    pub id: u64,
    pub announce: String,
    pub scrape: String,
    pub tier: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Peer {
    pub address: String,
    #[serde(rename = "clientName")]
    pub client_name: String,
    pub progress: f64,
    #[serde(rename = "rateToClient")]
    pub rate_to_client: u64,
    #[serde(rename = "rateToPeer")]
    pub rate_to_peer: u64,
    #[serde(rename = "flagStr")]
    pub flag_str: String,
}
static TORRENT_DETAILS_FIELDS: &[&str] = &[
    "id",
    "name",
    "eta",
    "sizeWhenDone",
    "seederCount",
    "leecherCount",
    "downloadDir",
    "comment",
    "hashString",
    "rateDownload",
    "rateUpload",
    "uploadRatio",
    "seedRatioLimit",
    "priority",
    "doneDate",
    "percentDone",
    "downloadedEver",
    "uploadedEver",
    "corruptEver",
    "status",
    "labels",
    "pieceCount",
    "pieces",
    "files",
    "fileStats",
    "priorities",
    "wanted",
    "peers",
    "peer",
    "trackers",
    "trackerStats",
    "error",
    "errorString",
];
#[derive(Deserialize, Debug, Clone)]
pub struct Torrents {
    pub torrents: Vec<TorrentDetails>,
}
#[derive(Deserialize, Debug, Clone)]
pub struct TorrentDetails {
    pub id: u64,
    pub name: String,
    pub eta: i64,
    #[serde(rename = "sizeWhenDone")]
    pub size_when_done: u64,
    #[serde(rename = "seederCount")]
    pub seeder_count: i64,
    #[serde(rename = "leecherCount")]
    pub leecher_count: i64,
    pub status: u64,
    #[serde(rename = "downloadDir")]
    pub download_dir: String,
    #[serde(rename = "comment")]
    pub comment: String,
    #[serde(rename = "hashString")]
    pub hash_string: String,
    #[serde(rename = "rateDownload")]
    pub rate_download: u64,
    #[serde(rename = "rateUpload")]
    pub rate_upload: u64,
    #[serde(rename = "uploadRatio")]
    pub upload_ratio: f64,
    #[serde(rename = "seedRatioLimit")]
    pub seed_ratio_limit: u64,
    #[serde(rename = "priority")]
    pub priority: u64,
    #[serde(rename = "doneDate")]
    pub done_date: u64,
    #[serde(rename = "percentDone")]
    pub percent_complete: f64,
    #[serde(rename = "downloadedEver")]
    pub downloaded_ever: u64,
    #[serde(rename = "uploadedEver")]
    pub uploaded_ever: u64,
    #[serde(rename = "corruptEver")]
    pub corrupt_ever: u64,
    pub labels: Vec<String>,
    #[serde(rename = "pieceCount")]
    pub piece_count: u64,
    pub pieces: String, // base64 encoded bitstring
    pub files: Vec<File>,
    #[serde(rename = "fileStats")]
    pub file_stats: Vec<FileStats>,
    pub priorities: Vec<u8>,
    pub wanted: Vec<u8>,
    pub peers: Vec<Peer>,
    pub trackers: Vec<Tracker>,
    #[serde(rename = "trackerStats")]
    pub tracker_stats: Vec<TrackerStats>,
    pub error: i64,
    #[serde(rename = "errorString")]
    pub error_string: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct TorrentAdd {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookies: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "download-dir")]
    pub download_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metainfo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "peer-limit")]
    pub peer_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "bandwidthPriority")]
    pub bandwith_priority: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "files-wanted")]
    pub files_wanted: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "files-unwanted")]
    pub files_unwanted: Option<Vec<i64>>,
    #[serde(rename = "priority-high")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_high: Option<Vec<i64>>,
    #[serde(rename = "priority-high")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_low: Option<Vec<i64>>,
    #[serde(rename = "priority-high")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_normal: Option<Vec<i64>>,
}

// FIXME: how to work with http errors? async errors?
// від заумі інтелігентськой, митця пожалуста спасі, щоб естетичний код продукту розшифрувать могли
// усі
impl TransmissionClient {
    pub fn new(url: &str) -> TransmissionClient {
        TransmissionClient {
            client: reqwest::Client::new(),
            session_id: Mutex::new("".to_string()),
            url: url.to_string(),
        }
    }

    pub async fn get_session_stats(&self) -> Result<RpcResponse<SessionStats>> {
        self
            .execute(json!({
                 "method": "session-stats"
            }))
            .await
    }

    pub async fn get_free_space(&self, path: &str) -> Result<RpcResponse<FreeSpace>> {
        self
            .execute(json!({
                 "method": "free-space",
                 "arguments": {
                     "path": &path
                 }
            }))
            .await
    }

    #[allow(dead_code)]
    pub async fn get_torrent_details(&self, ids: Vec<i64>) -> Result<RpcResponse<Torrents>> {
        self.execute(json!({
             "method": "torrent-get",
             "arguments": {
               "ids": &ids,
               "fields": TORRENT_DETAILS_FIELDS,
               "format": "objects"
             }
        }))
        .await
    }

    #[allow(dead_code)]
    pub async fn get_torrents(&self, ids: Vec<i64>, fields: &Vec<&str>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-get",
                 "arguments": {
                   "ids": &ids,
                   "fields": &fields,
                   "format": "table"
                 }
            }))
            .await
    }

    pub async fn queue_move_top(&self, ids: Vec<i64>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "queue-move-top",
                 "arguments": {
                   "ids": &ids
                 }
            }))
            .await
    }

    pub async fn queue_move_up(&self, ids: Vec<i64>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "queue-move-top",
                 "arguments": {
                   "ids": &ids
                 }
            }))
            .await
    }

    pub async fn queue_move_bottom(&self, ids: Vec<i64>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "queue-move-bottom",
                 "arguments": {
                   "ids": &ids
                 }
            }))
            .await
    }

    pub async fn queue_move_down(&self, ids: Vec<i64>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "queue-move-down",
                 "arguments": {
                   "ids": &ids
                 }
            }))
            .await
    }

    pub async fn torrent_start(&self, ids: Vec<i64>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-start",
                 "arguments": {
                   "ids": &ids
                 }
            }))
            .await
    }

    pub async fn torrent_start_now(&self, ids: Vec<i64>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-start-now",
                 "arguments": {
                   "ids": &ids
                 }
            }))
            .await
    }

    pub async fn torrent_stop(&self, ids: Vec<i64>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-stop",
                 "arguments": {
                   "ids": &ids
                 }
            }))
            .await
    }

    pub async fn torrent_verify(&self, ids: Vec<i64>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-verify",
                 "arguments": {
                   "ids": &ids
                 }
            }))
            .await
    }

    pub async fn torrent_reannounce(&self, ids: Vec<i64>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-reannounce",
                 "arguments": {
                   "ids": &ids
                 }
            }))
            .await
    }

    pub async fn torrent_remove(&self, ids: Vec<i64>, delete_local_data: bool) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-remove",
                 "arguments": {
                   "ids": &ids,
                   "delete-local-data": delete_local_data
                 }
            }))
            .await
    }

    pub async fn torrent_move(&self, ids: Vec<i64>, location: &str, move_data: bool) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-set-location",
                 "arguments": {
                   "ids": &ids,
                   "location": &location,
                   "move": move_data
                 }
            }))
            .await
    }

    // returnes also removed array of torrent-id numbers of recently-removed torrents.
    pub async fn get_recent_torrents(&self, fields: &Vec<&str>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-get",
                 "arguments": {
                   "ids": "recently-active",
                   "fields": &fields,
                   "format": "table"
                 }
            }))
            .await
    }

    pub async fn get_all_torrents(&self, fields: &Vec<&str>) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-get",
                 "arguments": {
                   "fields": &fields,
                   "format": "table"
                 }
            }))
            .await
    }

    pub async fn torrent_add(&self, torrent_add: &TorrentAdd) -> Result<Value> {
        self
            .execute(json!({
                 "method": "torrent-add",
                 "arguments": &torrent_add
            }))
            .await
    }

    pub fn set_session_id(&self, session_id: &str) {
        let mut s = self.session_id.lock().expect("can't get hold of the mutex(");
        *s = session_id.to_string();
    }
    pub fn get_session_id(&self) -> String {
        let s = self.session_id.lock().expect("can't get hold of the mutex(");
        s.to_string()
    }

    pub async fn execute<R>(&self, json: Value) -> Result<R>
    where
        R: DeserializeOwned + std::fmt::Debug,
    {
        // TODO: well, it doesn't matter here because TorrentClient is behind a channel, so it's
        // not really concurrent. But if so, how to tell rust it is OK to mutate hmm

        let response = self
            .client
            .post(&self.url)
            .header("X-Transmission-Session-Id", self.get_session_id())
            .json(&json)
            .send()
            .await?;

        let response = match response.status() {
            reqwest::StatusCode::CONFLICT => {
                //println!("getting new CSRF token");
                let sid = response
                    .headers()
                    .get("x-transmission-session-id")
                    .expect("server returned no CSRF token.")
                    .to_str()
                    .expect("wrong CSRF token.")
                    .to_string();
                self.set_session_id(&sid);
                self.client
                    .post(&self.url)
                    .header("X-Transmission-Session-Id", sid.to_string())
                    .json(&json)
                    .send()
                    .await?
            }
            _ => response,
        };
        let json = response.json().await?;
        //println!("Response body: {:#?}", json);
        serde_json::from_value(json).map_err(From::from)
    }
}
