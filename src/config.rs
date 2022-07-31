use home::home_dir;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, write, File};
use std::io::BufReader;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Action {
    pub description: String,
    pub shortcut: String,
    pub cmd: String,
    pub args: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DirMapping {
    pub label: String,
    #[serde(alias = "remote_path")]
    #[serde(rename = "remote-path")]
    pub remote_path: String,
    #[serde(alias = "local_path")]
    #[serde(rename = "local-path")]
    pub local_path: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Connection {
    pub name: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    pub url: String,
    #[serde(alias = "remote_base_dir")]
    #[serde(alias = "remote-base-dir")]
    #[serde(rename = "download-dir")]
    #[serde(default)]
    pub download_dir: String,
    #[serde(alias = "local_base_dir")]
    #[serde(alias = "local-base-dir")]
    #[serde(rename = "local-download-dir")]
    #[serde(default)]
    pub local_download_dir: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TrafficMonitorOptions {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "download")]
    Download,
    #[serde(rename = "upload")]
    Upload,
}

impl Default for TrafficMonitorOptions {
    fn default() -> Self {
        TrafficMonitorOptions::Upload
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(alias = "refresh_interval")]
    #[serde(rename = "refresh-interval")]
    pub refresh_interval: u16,

    #[serde(rename = "traffic-monitor")]
    #[serde(default)]
    pub traffic_monitor: TrafficMonitorOptions,
    pub connections: Vec<Connection>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
}

fn empty_config() -> Config {
    Config {
        connections: vec![Connection {
            name: String::from("localhost"),
            username: String::from(""),
            password: String::from(""),
            url: String::from("http://127.0.0.1:9091/transmission/rpc"),
            download_dir: "".to_string(),
            local_download_dir: "".to_string(),
        }],
        refresh_interval: 1200,
        actions: vec![],
        traffic_monitor: TrafficMonitorOptions::Upload,
    }
}
pub fn get_or_create_config() -> Result<Config, Box<dyn std::error::Error>> {
    let home = home_dir().expect("can't obtain user home directory");
    let config_dir = home.join(".config").join("transg");
    if !config_dir.exists() {
        create_dir_all(&config_dir)?;
    }
    // created a bit of a hussle for meself
    let config_path = config_dir.join("transg-tui.toml");
    let config_path_json = config_dir.join("transg-tui.json");

    if !config_path.exists() {
        let config = if config_path_json.exists() {
            let f = File::open(config_path_json)?;
            let buff = BufReader::new(f);
            let config: Config = serde_json::from_reader(buff)?;
            config
        } else {
            empty_config()
        };
        let toml = toml::to_string(&config).unwrap();
        write(&config_path, toml)?; //.unwrap_or_else(|_| panic!("Failed to create {:?}", &config_path));

        Ok(config)
    } else {
        let bytes = std::fs::read(&config_path)?; //.unwrap_or_else(|_| panic!("Can't open {:?}", &config_path));
        let config: Config = toml::from_slice(&bytes)?;
        Ok(config)
    }
}
