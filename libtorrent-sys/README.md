libtorrent-sys
==============

Rust API / cxx bindings for [**libtorrent**].

[**libtorrent**]: https://libtorrent.org/

All c++ reference can be found in libtorrent [**reference documentation**]

[**reference documentation**]: https://libtorrent.org/reference.html

Exposed API Reference
---------------------
- session
- add_torrent_params
- add_torrent_params
- torrent_handle
- create_torrent
- parse_magnet_uri()
- bencode()

Install
-------
1/ Download or [**build**] libtorrent (branch C_2_0)

[**build**]: https://libtorrent.org/building.html

2/ Build libtorrent-sys:

```
RUSTFLAGS="-C linker=g++" CXX=g++ cargo build
```
