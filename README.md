This project started out on codecrafters: [link to the challenge](https://app.codecrafters.io/courses/bittorrent/introduction)

# a BitTorrent implementation in Rust
I somehow found pretty big interest in peer-to-peer communication. I found a challenge where you implement BitTorrent by yourself from scratch on a pretty cool coding website (see above). Now I completed the main part (it's only partial on their website) and want to implement the rest of the [protocol](https://bittorrent.org/beps/bep_0003.html) (no extensions).

## features (done / want to add)
- [x] downloading a file from the peers of the tracker found in the .torrent file
- [x] ok, I added the ut_metadata extension to support magnet_links
- [ ] seeding a file (partially done if we're already connected to a peer and he asks for a block)
- [x] storing the state of a file to disk so that you can stop a download and continue later
- [ ] retrying if none of the peers seed (currently it just iters through the peer-list once and if no one's there, no file for you)
- [ ] choking algorithm
- [ ] actually usable CLI or something

## current CLI usage
"current" because I will probably change some of it since the CLI commands were from the original challenge and are more of a sort of guidance.

```
Commands:
  decode
  info
  peers
  handshake
  download_piece
  download
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

Now you'll probably want to use the "download" command which looks like that:
```
Usage: codecrafters-bittorrent download [OPTIONS] <TORRENT>

Arguments:
  <TORRENT>

Options:
  -o <OUTPUT>
  -h, --help       Print help
```
If no output is provided, it will use the name found in the .torrent file.

### example
`codecrafters-bittorrent download sample.torrent -o test.txt`
