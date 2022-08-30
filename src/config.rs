use home::home_dir;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, write, File};
use std::io::BufReader;
use tui::style::{Color, Style, Modifier};

pub struct Styles {
  pub text: Style,
  pub bold: Style,
  pub highlight: Style,
  pub emphasis: Style,
  pub emphasis_underline: Style,
  pub details_highlight: Style,
  pub details_emphasis: Style,
  pub details_emphasis_underline: Style,
  pub error_text: Style,
  pub blend_in: Style
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColorScheme {
    pub text: Color,
    #[serde(rename = "highlight")]
    pub highlight: Color,
    #[serde(rename = "highlight-text")]
    pub highlight_text: Color,
    #[serde(rename = "text-soft")]
    pub text_soft: Color,
    #[serde(rename = "text-error")]
    pub text_error: Color
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Colors {
   pub main: ColorScheme,
   pub details: ColorScheme
}

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

fn truth() -> bool { true }

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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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
    #[serde(rename = "show-icons")]
    #[serde(default = "truth")]
    pub show_icons: bool,
    pub connections: Vec<Connection>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
    #[serde(default)]
    #[serde(rename = "file-actions")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_actions: Vec<Action>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<Colors>
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
        show_icons: true,
        actions: vec![],
        file_actions: vec![],
        traffic_monitor: TrafficMonitorOptions::Upload,
        colors: None
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
        //println!("{:?}", config);
        Ok(config)
    }
}

pub fn compute_styles(config: &Config) -> Styles {
    let colors = config.colors.as_ref().unwrap_or_else(|| {
        let should_use_light_skin = terminal_light::luma()
        .map_or(false, |luma| luma > 0.6);
        if should_use_light_skin {
            &Colors {
               main: ColorScheme {
                text: Color::Black,
                highlight: Color::Cyan,
                highlight_text: Color::Gray,
                text_soft: Color::DarkGray,
                text_error: Color::Red
            },
               details: ColorScheme { 
                   text: Color::Black, 
                   highlight: Color::Magenta, 
                   highlight_text: Color::Gray, 
                   text_soft: Color::DarkGray, 
                   text_error: Color::Red }
            }
        } else {
            &Colors { 
                main: ColorScheme {
                text: Color::White,
                highlight: Color::Yellow,
                highlight_text: Color::Black,
                text_soft: Color::Gray,
                text_error: Color::Red
            }, 
            details: ColorScheme { text: Color::White, highlight: Color::LightBlue, highlight_text: Color::Gray, text_soft: Color::Gray, text_error: Color::Red }
            }

        }

    });
    Styles { 
        text: Style::default().fg(colors.main.text),
        bold: Style::default().fg(colors.main.text).add_modifier(Modifier::BOLD),
        highlight: Style::default().bg(colors.main.highlight).fg(colors.main.highlight_text).add_modifier(Modifier::BOLD), 
        emphasis: Style::default().fg(colors.main.highlight), 
        emphasis_underline: Style::default().fg(colors.main.highlight).add_modifier(Modifier::UNDERLINED),
        details_highlight: Style::default().bg(colors.details.highlight).fg(colors.details.highlight_text).add_modifier(Modifier::BOLD), 
        details_emphasis: Style::default().fg(colors.details.highlight),
        details_emphasis_underline: Style::default().fg(colors.details.highlight).add_modifier(Modifier::UNDERLINED),
        error_text: Style::default().fg(Color::Red),
        blend_in: Style::default().fg(colors.main.text_soft)
    }
}
