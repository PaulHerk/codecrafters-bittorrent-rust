- [ ] make the peer_manager run a seperate loop before the actual one to download the metadata
  (something like while self.torrent_info.is_none() { download torrent & check hash})
- [ ] change torrent occurrences to torrent_info
- [ ] how to get the announce? maybe make a functions for peer: from_torrent(), from_magnet()
- [ ] in the DB safe the torrent_info instead of the torrent path
- [ ] make an extension msg checker or sth in the event_loop of the peer