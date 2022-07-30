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
    pub remote_path: String,
    pub local_path: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Connection {
    pub name: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub remote_base_dir: String,
    pub local_base_dir: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub connections: Vec<Connection>,
    pub refresh_interval: u16,
    pub actions: Vec<Action>,
}

fn empty_config() -> Config {
    Config {
        connections: vec![Connection {
        name: String::from("localhost"),
        username: String::from(""),
        password: String::from(""),
        url: String::from("http://127.0.0.1:9091/transmission/rpc"),
        remote_base_dir: "".to_string(),
        local_base_dir: "".to_string(),
        }],
        refresh_interval: 1200,
        actions: vec![]
    }
}
pub fn get_or_create_config() -> Config {
    let home = home_dir().expect("can't obtain user home directory");
    let config_dir = home.join(".config").join("transg");
    if !config_dir.exists() {
        create_dir_all(&config_dir).expect("can't create ~/.config/transg");
    }
    let config_path = config_dir.join("transg-tui.json");
    if !config_path.exists() {
        let cfg = serde_json::to_string(&empty_config()).expect("should serialize");
        write(&config_path, cfg).unwrap_or_else(|_| panic!("Failed to create {:?}", &config_path));
    }
    let f = File::open(config_path).expect("can't open config file");
    let buff = BufReader::new(f);
    let config: Config = serde_json::from_reader(buff).expect("can't parse json config");
    config
}
