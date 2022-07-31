
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

[TOML](https://toml.io) configuration file is located at `~/.config/transg/transg-tui.toml`
**Note:** old `transg-tui.json` is deprecated and will be converted to the new format on app start. sorry for the hassle

Example config:
```toml
refresh_interval = 1200
# one of: "upload" "donwload" "none"
traffic-monitor = "upload"

[[connections]]
name = "NAS"
username = ""
password = ""
url = "http://192.168.1.18:9091/transmission/rpc"
#
# optional. explicitly sets transmission's download directory
# download-dir = "/var/lib/transmission/downloads"
#
# optional. Set this if you have remote transmission folder mounted on the local filesystem. 
# If set, {location} token gets replaced with the local folder.
local-download-dir = "/run/mount/transmission"


[[actions]]
description = "open in nautilus"
shortcut = "o"
cmd = "swaymsg"
args = ["exec", "--", "nautilus", "\"{location}\""]

[[actions]]
description = "terminal"
shortcut = "t"
cmd = "swaymsg"
args = ["exec", "--", "alacritty", "--working-directory", "\"{location}\""]
```

Substitutions:

|  Token            | Description                                                          |
| ----------------- | -------------------------------------------------------------------- |
|  `{location}`     | If torrent contains a folder then this folder else its download dir  |
|  `{id}`           | Torrent's id                                                         |
|  `{download_dir}` | Download directory                                                   |
|  `{name}`         | Torrent's name                                                       |

## Keybindings

| Key       | Description                         |
| :-------: | :---------------------------:       |
| `↑ / k`   | Prev item                           |
| `↓ / j`   | Next item                           |
| `f`       | Filter menu                         |
| `S`       | Sort menu                           |
| `space`   | Action menu                         |
| `d`       | Details screen (under construction) |
| `/`       | Find next item in list              |
| `?`       | Find prev item in list              |
| `s`       | Search across all torrents          |
| `c`       | Connection menu                     |
| `F1`      | Help screen                         |
| `Esc`     | Exit from all menus                 |
| `q`       | Quit                                |


## Betterships
* Low memory usage even with thousands of torrents
* VIM-like keys
* Organize your torrents with folders
* Run custom external commands

## Similar apps
* [tremc](https://github.com/tremc/tremc)
* [stig](https://github.com/rndusr/stig)
* [Original transmission-remote client](https://github.com/transmission/transmission)
* [Fragments](https://gitlab.gnome.org/World/Fragments)
* [transmission-remote-gtk](https://github.com/transmission-remote-gtk/transmission-remote-gtk)
* [transgui](https://github.com/transmission-remote-gui/transgui)

