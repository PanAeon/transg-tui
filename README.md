
# Transgression TUI

Transmission remote TUI client.

[![asciicast](https://asciinema.org/a/511535.svg)](https://asciinema.org/a/511535)
[![Rust](https://github.com/PanAeon/transg-tui/actions/workflows/rust.yml/badge.svg?branch=master)](https://github.com/PanAeon/transg-tui/actions/workflows/rust.yml)


## Building

Dependencies:
* rustc   >= 1.62
* cargo   >= 1.62
* openssl  >= 1.1.0

```bash
cargo build --release
```

### NIX
```bash
nix build
```
or
```bash
nix run github:PanAeon/transg-tui
```
alternatively you can grab latest build from [Github Actions](https://github.com/PanAeon/transg-tui/actions)


## Configuration

JSON configuration file is located at `~/.config/transg/transg-tui.json`

Example config:
```json
{
  "connections": [{
      "name": "localhost",
      "url": "http://127.0.0.1:9091/transmission/rpc",
      "username": "",
      "password": "",
      "remote_base_dir": "/var/lib/transmission/torrents",
      "local_base_dir": "/var/mount/torrents"
  }],
  "refresh_interval": 1200,
  "actions": [{
    "description": "open in nautilus",
    "shortcut": "o",
    "cmd": "swaymsg",
    "args": ["exec", "--", "nautilus", "\"{location}\""]
  },
  {
    "description": "terminal",
    "shortcut": "t",
    "cmd": "swaymsg",
    "args": ["exec", "--", "alacritty", "--working-directory", "\"{location}\""]
  }],
}
```
Substitutions:

|  Token            | Description                                                          |
| ----------------- | -------------------------------------------------------------------- |
|  `{location}`     | If torrent contains a folder then this folder else its download dir  |
|  `{id}`           | Torrent's id                                                         |
|  `{download_dir}` | Download directory                                                   |
|  `{name}`         | Torrent's name                                                       |

## Keybindings

| Key       | Description                   |
| :-------: | :---------------------------: |
| `↑ / k`   | Prev item                     | 
| `↓ / j`   | Next item                     |
| `f`       | Filter menu                   |
| `S`       | Sort menu                     |
| `space`   | Action menu                   |
| `/`       | Find next item in list        |
| `?`       | Find prev item in list        |
| `s`       | Search across all torrents    |
| `c`       | Connection menu               |
| `F1`      | Help screen                   |
| `Esc`     | Exit from all menus           |
| `q`       | Quit                          |


## Betterships
* Low memory usage even with thousands of torrents
* VIM-like keys
* Organize your torrents with folders
* Run custom external commands

## Similar apps
* `transmission-remote`
* `Fragments`
* `transmission-remote-gtk`
* `transgui`
* `tremc`

