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

#include "src/lt.h"

namespace libtorrent {

std::unique_ptr<lt::session> lt_create_session()
{
	return std::make_unique<lt::session>();
}

std::unique_ptr<lt::add_torrent_params> lt_parse_magnet_uri(rust::Str uri, rust::Str path)
{
	std::string s(uri);
	std::string r(path);
	lt::add_torrent_params p;

	p = lt::parse_magnet_uri(s.c_str());
	p.save_path = r.c_str();

	return std::make_unique<lt::add_torrent_params>(std::move(p));
}

std::unique_ptr<lt::torrent_handle> lt_session_add_torrent(lt::session &ses, lt::add_torrent_params &params)
{
	lt::torrent_handle hdl;

	hdl = ses.add_torrent(params);

	return std::make_unique<lt::torrent_handle>(std::move(hdl));
}

void lt_session_remove_torrent(lt::session &ses, const lt::torrent_handle &hdl)
{
	ses.remove_torrent(hdl);
}

void lt_session_pause(lt::session &ses)
{
	ses.pause();
}

bool lt_torrent_has_metadata(const lt::torrent_handle &hdl)
{
	return hdl.status().has_metadata;
}

rust::Str lt_torrent_get_name(const lt::torrent_handle &hdl)
{
	auto infos = hdl.torrent_file();
	return infos->name();
}

rust::Slice<const uint8_t> lt_torrent_bencode(const lt::torrent_handle &hdl)
{
	auto infos = hdl.torrent_file();
  auto entry = lt::create_torrent(*infos).generate();
	std::vector<char> vec;
	lt::bencode(std::back_inserter(vec), entry);
	rust::Slice<const uint8_t> slice{reinterpret_cast<const unsigned char *>(vec.data()), vec.size()};
	return slice;
}

} // namespace libtorrent
