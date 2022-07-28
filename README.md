# Transgression TUI

Transmission remote TUI client.

[![asciicast](https://asciinema.org/a/511535.svg)](https://asciinema.org/a/511535)


## Building

Dependencies:
* rustc   >= 1.62
* cargo   >= 1.62
* openssl  >= 1.1.0

```bash
cargo build --release
```

## Configuration

Configuration file uses JSON and is located at `~/.config/transg/config.json`

Example config:
```json
{
  "connection_name": "localhost",
  "connection_string": "http://127.0.0.1:9091/transmission/rpc",
  "remote_base_dir": "/var/lib/transmission/torrents",
  "local_base_dir": "/var/mount/torrents",
  "refresh_interval": 1200,
  "file_manager": {
    "cmd": "swaymsg",
    "args": ["exec", "--", "nautilus", "\"{location}\""]
  },
  "terminal": {
    "cmd": "swaymsg",
    "args": ["exec", "--", "alacritty", "--working-directory", "\"{location}\""]
  }
}
```

## Keybindings

`Transg` tries to be both compatible with VIM, but also utilize easy to remember mnemonics when possible:
* `hjkl` or `cursor keys`  - navigation
* `F1`                     - Help screen
*  `f`                     - Filter select
* `space`                  - action menu
* `/` and `?`              - find in list ff/rw
* `s`                      - Search all torrents
* `q`                      - Quit

## Betterships
* Low memory usage even with thousands of torrents
* VIM-compatible keys

## Disclaimer
I've got a bit of specific workflow were I got like 3k+ torrents on my NAS computer, sorted into folders under the common root, debian iso mirrors, you know.
Essentially I want to filter my torrents by its folder name, but most of the clients don't provide such luxuries.
Also most of the existing clients waste ~1G of mem with my set-up. 
This is an outrage! I just want a bloody list of torrents filtered by directory, is this too much to ask?


## Prior art:
* `transmission-remote`
* `Fragments` (quite good)
* `transmission-qt` 
* `transmission-gtk` 
* `transmission-remote-gtk`
* `transgui` (very lightweight, but uses outdated gtk2 looks)
* `tremc` (really good, except doesn't filter by directories)

