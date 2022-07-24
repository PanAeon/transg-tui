// Copyright (c) 2022 Nicolas Chevalier
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

#pragma once

#include "rust/cxx.h"

#include <libtorrent/version.hpp>
#include <libtorrent/create_torrent.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/magnet_uri.hpp>

#include <memory>

char const* version();

namespace libtorrent {

std::unique_ptr<lt::session> lt_create_session();
std::unique_ptr<lt::add_torrent_params> lt_parse_magnet_uri(rust::Str uri, rust::Str path);
std::unique_ptr<lt::torrent_handle> lt_session_add_torrent(lt::session &ses, lt::add_torrent_params &params);
void lt_session_remove_torrent(lt::session &ses, const lt::torrent_handle &hdl);
void lt_session_pause(lt::session &ses);
bool lt_torrent_has_metadata(const lt::torrent_handle &hdl);
rust::Str lt_torrent_get_name(const lt::torrent_handle &hdl);
rust::Slice<const uint8_t> lt_torrent_bencode(const lt::torrent_handle &hdl);

}
