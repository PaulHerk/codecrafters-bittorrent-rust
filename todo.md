- remove `have` and `extensions` from the `PeerStateInner`
- rather have the peer the extensions
- the `PeerManager` inits a `PieceSelector`
- implement missing `ReqMessage` msgs: `GotHave(u32)`, `GotBitfield(Vec<bool>)`
- if a bitfield or a have message is sent, tell the PeerManager
  - he then calls the appropiate method for the piece_selector

---

This is the most critical part. Here is how the `prepare_next_blocks` function for a specific peer (`Peer A`) should work:

1. **Fill the `DownloadQueue`:** The `PieceManager` first checks if the central `DownloadQueue` is full. If not, it asks the `PieceSelector` for the best new pieces to add (that aren't already in the queue or `in_flight`) and adds their `PieceState`s to the `DownloadQueue`.
2. **Iterate and Match:** The `prepare_next_blocks` function then iterates through every `PieceState` in the central `DownloadQueue`.
3. **Check Peer Availability:** For each piece in the queue, it asks a crucial question: **"Does `Peer A` have this piece?"** This check is done by querying the `PieceSelector`, which stores all the peer bitfields.
4. **Find Available Blocks:**

   * If `Peer A` *does not* have the piece, we simply skip it and move to the next piece in the queue.
   * If `Peer A` *does* have the piece, we then scan that piece's `blocks` vector to find a block that is in the `BlockState::None` state.
5. **Generate Request:** If an available block is found, we generate a `RequestPiecePayload` for it, add it to our list of requests for `Peer A`, and immediately update that block's state to `BlockState::InProcess(timestamp)`.
