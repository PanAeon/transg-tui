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
// hmm, now it's public mutable.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub connection_name: String,
    pub username: String,
    pub password: String,
    pub connection_string: String,
//    pub directories: Vec<String>,
    pub remote_base_dir: String,
    pub local_base_dir: String,
    pub refresh_interval: u16,
    pub actions: Vec<Action>,
}

impl Config {
  /*  #![allow(dead_code)]
    pub fn get_directories(&self) -> Vec<DirMapping> {
        self.directories
            .iter()
            .map(|x| DirMapping {
                label: x.to_string(),
                remote_path: format!("{}/{}", self.remote_base_dir, x),
                local_path: format!("{}/{}", self.local_base_dir, x),
            })
            .collect()
    }*/
}

fn empty_config() -> Config {
    Config {
        connection_name: String::from("localhost"),
        username: String::from(""),
        password: String::from(""),
        connection_string: String::from(""),
        remote_base_dir: "".to_string(),
        local_base_dir: "".to_string(),
        refresh_interval: 3,
        actions: vec![]
    }
}
pub fn get_or_create_config() -> Config {
    let home = home_dir().expect("can't obtain user home directory");
    let config_dir = home.join(".config").join("transg");
    if !config_dir.exists() {
        create_dir_all(&config_dir).expect("can't create ~/.config/transg");
    }
    let config_path = config_dir.join("config.json");
    if !config_path.exists() {
        let cfg = serde_json::to_string(&empty_config()).expect("should serialize");
        write(&config_path, cfg).unwrap_or_else(|_| panic!("Failed to create {:?}", &config_path));
    }
    let f = File::open(config_path).expect("can't open config file");
    let buff = BufReader::new(f);
    let config: Config = serde_json::from_reader(buff).expect("can't parse json config");
    config
}
