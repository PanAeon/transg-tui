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
Note: I've tested it only on __Linux__  so far.

### NIX
```bash
nix build
```
or
```bash
nix run github:PanAeon/transg-tui
```


## Configuration

Configuration file uses JSON and is located at `~/.config/transg/config.json`

Example config:
```json
{
  "connection_name": "localhost",
  "connection_string": "http://127.0.0.1:9091/transmission/rpc",
  "username": "",
  "password": "",
  "remote_base_dir": "/var/lib/transmission/torrents",
  "local_base_dir": "/var/mount/torrents",
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

|  Token            | Description                                                        |
| ----------------- | ------------------------------------------------------------------ |
|  `{location}`     | If torrent contains folder then this folder else its download dir  |
|  `{id}`           | Torrent's id                                                       |
|  `{download_dir}` | Download directory                                                 |
|  `{name}`         | Torrent's name                                                     |

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
* VIM-like keys
* Organize your torrents with folders

## Similar apps:
* `transmission-remote`
* `Fragments`
* `transmission-qt` 
* `transmission-gtk` 
* `transmission-remote-gtk`
* `transgui` (very lightweight, but uses outdated gtk2 looks)
* `tremc` (really good, except doesn't filter by directories)

