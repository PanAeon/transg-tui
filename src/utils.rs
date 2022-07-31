use std::collections::HashMap;

use chrono::{DateTime, NaiveDateTime, Utc};
use tui_tree_widget::TreeItem;

//use std::fmt;
use crate::transmission::{self, TorrentStatus};
//use chrono::{DateTime, NaiveDateTime, Utc};

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub downloaded: u64,
    pub children: Vec<Node>,
}

const DEC_TB: i64 = 1000 * 1000 * 1000 * 1000;
const DEC_GB: i64 = 1000 * 1000 * 1000;
const DEC_MB: i64 = 1000 * 1000;
//const BYTES_TB: i64 = 1024 * 1024 * 1024 * 1024;
//const BYTES_GB: i64 = 1024 * 1024 * 1024;
//const BYTES_MB: i64 = 1024 * 1024;
const F_BYTES_TB: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;
const F_BYTES_GB: f64 = 1024.0 * 1024.0 * 1024.0;
const F_BYTES_MB: f64 = 1024.0 * 1024.0;

pub fn process_folder(s: &str, base_dir: &str) -> String {
    if s == base_dir {
        s.split('/').last().unwrap_or("<root>").to_string()
    } else {
        let mut s = s.replace(base_dir, ""); // TODO: special case, when base_dir is '/'
        if s.starts_with('/') {
            s = s.strip_prefix('/').expect("prefix").to_string();
        }
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() > 1 {
            format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
        } else {
            s.to_string()
        }
    }
}

pub fn format_percent_done(f: f64) -> String {
    if f >= 1.0 {
        "✓".to_string()
    } else {
        format!("{:.0}%", 100.0 * f)
    }
}

pub fn format_size(i: i64) -> String {
    if i == 0 {
        "".to_string()
    } else if i > DEC_TB {
        format!("{:.1}T", i as f64 / F_BYTES_TB)
    } else if i > DEC_GB {
        format!("{:.1}G", i as f64 / F_BYTES_GB)
    } else if i > DEC_MB {
        format!("{:.1}M", i as f64 / F_BYTES_MB)
    } else {
        format!("{:.1}K", i as f64 / 1024.0)
    }
}

pub fn format_download_speed(i: i64, hide_zero: bool) -> String {
    if hide_zero && i == 0 {
        "".to_string()
    } else if i > DEC_MB {
        format!("{: >5.1} M/s", i as f64 / F_BYTES_MB)
    } else {
        format!("{: >5.1} K/s", i as f64 / 1024.0)
    }
}
pub fn format_time(i: u64) -> String {
    let naive = NaiveDateTime::from_timestamp(i.try_into().expect("can't convert from u64 into i64"), 0);
    let datetime: DateTime<Utc> = DateTime::from_utc(naive, Utc);
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn format_eta(secs: i64) -> String {
    if secs == -1 {
        "".to_string()
    } else if secs == -2 {
        "∞".to_string()
    } else {
        let days = secs / 86400;
        let secs = secs - days * 86400;
        let hours = secs / 3600;
        let secs = secs - hours * 3600;
        let minutes = secs / 60;
        let secs = secs - minutes * 60;

        if days > 0 {
            format!("{}d {}h", days, hours)
        } else if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, secs)
        } else {
            format!("{}s", secs)
        }
    }
}

pub fn format_status<'a>(x: &TorrentStatus, err: i64) -> &'a str {
    if err != 0 {
        " ⁈"
    } else {
        match x {
            TorrentStatus::Paused => " ⏸ ",
            TorrentStatus::VerifyQueued => " 🗘",
            TorrentStatus::Verifying => " 🗘",
            TorrentStatus::DownQueued => " ⇩",
            TorrentStatus::Downloading => " ⇣",
            TorrentStatus::SeedQueued => " ⇧",
            TorrentStatus::Seeding => " ⇡",
        }
    }
}

#[allow(dead_code)]
pub fn utf8_truncate(input: &mut String, maxsize: usize) {
    let mut utf8_maxsize = input.len();
    if utf8_maxsize >= maxsize {
        {
            let mut char_iter = input.char_indices();
            while utf8_maxsize >= maxsize {
                utf8_maxsize = match char_iter.next_back() {
                    Some((index, _)) => index,
                    _ => 0,
                };
            }
        } // Extra {} wrap to limit the immutable borrow of char_indices()
        input.truncate(utf8_maxsize);
    }
}

pub fn utf8_split(input: &str, at: usize) -> (String, String) {
    let mut it = input.chars();
    let fst = it.by_ref().take(at).collect();
    let snd = it.collect();
    (fst, snd)
}

// TODO: write something better..
pub fn do_build_tree(parent_path: &str, level: usize, xs: Vec<(u64, u64, Vec<String>)>) -> Vec<Node> {
    let mut ns: Vec<Node> = vec![];

    let mut parents: Vec<String> = xs
        .iter()
        .filter(|x| x.2.len() > level)
        .map(|x| x.2[level].clone())
        .collect();
    parents.sort();
    parents.dedup();

    for name in parents {
        let children: Vec<(u64, u64, Vec<String>)> = xs
            .iter()
            .filter(|x| x.2.len() > level && x.2[level] == name)
            .cloned()
            .collect();
        let path = if parent_path.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", parent_path, name)
        };
        let size = children.iter().map(|x| x.0).sum();
        let downloaded = children.iter().map(|x| x.1).sum();
        let cs = if children.len() > 1 {
            do_build_tree(&path, level + 1, children)
        } else {
            vec![]
        };
        ns.push(Node {
            name,
            path,
            children: cs,
            size,
            downloaded,
        });
    }
    ns
}

pub fn build_tree(files: &[transmission::File]) -> Vec<Node> {
    let mut xs: Vec<(u64, u64, Vec<String>)> = files
        .iter()
        .map(|f| {
            (
                f.length,
                f.bytes_completed,
                f.name.split('/').map(String::from).collect(),
            )
        })
        .collect();
    xs.sort_by(|a, b| a.2[0].partial_cmp(&b.2[0]).unwrap());
    do_build_tree("", 0, xs)
}
pub fn do_build_file_tree<'a>(
    level: usize,
    xs: Vec<(u64, u64, Vec<u64>)>,
    strings: &HashMap<u64, &str>,
) -> Vec<TreeItem<'a>> {
    let mut ns: Vec<TreeItem> = vec![];

    let mut parents: Vec<u64> = xs.iter().filter(|x| x.2.len() > level).map(|x| x.2[level]).collect();
    parents.sort();
    parents.dedup();

    for name in parents {
        let children: Vec<(u64, u64, Vec<u64>)> = xs
            .iter()
            .filter(|x| x.2.len() > level && x.2[level] == name)
            .cloned()
            .collect();
        let size: u64 = children.iter().map(|x| x.0).sum();
        //let downloaded = children.iter().map(|x| x.1).sum();
        let cs = if children.len() > 1 {
            do_build_file_tree(level + 1, children, strings)
        } else {
            vec![]
        };
        ns.push(TreeItem::new(
            format!(
                "{} - {}",
                strings.get(&name).expect("should be name"),
                crate::utils::format_size(size as i64)
            ), //   path,
               //  size,
               //  downloaded,
            cs,
        ));
    }
    ns
}
pub fn build_file_tree<'a>(files: &[transmission::File]) -> Vec<TreeItem<'a>> {
    let mut id: u64 = 0;
    let mut strings: HashMap<&str, u64> = HashMap::new();
    let mut xs: Vec<(u64, u64, Vec<u64>)> = files
        .iter()
        .map(|f| {
            (
                f.length,
                f.bytes_completed,
                f.name
                    .split('/')
                    .map(|s| {
                        if let Some(id) = strings.get(s) {
                            *id
                        } else {
                            id += 1;
                            strings.insert(s, id);
                            id
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    xs.sort_by(|a, b| a.2[0].partial_cmp(&b.2[0]).unwrap());
    let strings: HashMap<u64, &str> = strings.iter().map(|x| (*x.1, *x.0)).collect();
    do_build_file_tree(0, xs, &strings)
}
