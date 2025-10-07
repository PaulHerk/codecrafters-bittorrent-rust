# Extension Handling Refactor Plan

This plan refactors the extension handling logic to be more modular and extensible. The core idea is to use a trait `ExtensionHandler` to abstract the logic for each extension.

- [ ] **Define `ExtensionAction` and `ExtensionMessage` enums** in `src/extensions/mod.rs`.

  - `ExtensionAction` will represent the outcome of a handler's execution (e.g., `SendMessage`, `RequestToManager`, `Nothing`).
  - `ExtensionMessage` will be a dedicated enum for messages sent from an extension to the `PeerManager`.

- [ ] **Define the `ExtensionHandler` trait** in `src/extensions/mod.rs`.

  - It will include an `async fn handle_message(&mut self, data: &[u8]) -> Option<ExtensionAction>` for reacting to incoming peer messages.
  - It will include an `async fn on_handshake(&mut self) -> Option<ExtensionAction>` to allow proactive actions after the extension handshake.

- [ ] **Create an `ExtensionFactory`** in a new `src/extensions/factory.rs` file.

  - This factory will have a static `build(name: &str) -> Option<Box<dyn ExtensionHandler>>` method.
  - It will be responsible for creating instances of supported extension handlers based on their string name (e.g., "ut_metadata").

- [ ] **Implement the `ExtensionHandler` trait** for the metadata extension (`ut_metadata`).

  - Create a `MetadataRequester` struct with a `new()` function.
  - Implement the trait methods to handle metadata messages and initiate requests.

- [ ] **Update the `Peer` struct** in `src/peer/mod.rs`.

  - Add a `HashMap<u8, Box<dyn ExtensionHandler>>` to store the active extension handlers for the peer.

- [ ] **Refactor the peer event loop** in `src/peer/event_loop.rs`.
  - After receiving the extended handshake, use the `ExtensionFactory` to build and store the handlers for all supported extensions announced by the peer.
  - When an `Extended` message is received, look up the corresponding handler in the `HashMap` and call `handle_message`.
  - Process the returned `ExtensionAction`.
  - After the initial extension handshake, iterate through the handlers and call `on_handshake`.
