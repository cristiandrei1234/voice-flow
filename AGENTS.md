# Voice Flow: guide for working in the code

Local dictation for macOS. Two parts:

- **The Rust engine** (`crates/`): the `voice-flow` binary. The app starts it as `voice-flow daemon` and they
  talk JSON, one object per line: commands on stdin (`{"cmd":"set_sounds","enabled":true}`), events on stdout
  (`{"type":"state","state":"recording"}`). It is all in `crates/app/src/daemon.rs` (`Command`, `Event`, `Msg`,
  `Daemon::command`, `Daemon::handle`).
- **The SwiftUI app** (`macos/Sources/VoiceFlow/`): the menu bar popover, the Voice Flow window (sidebar plus
  sections) and the recording pill. `Engine.swift` starts the engine and turns events into state.

## Rust layout

- `crates/domain`: pure types, no system I/O (settings, dictionary, model catalog, styles, snippets, history).
  Testable logic goes here.
- `crates/application`: the pipeline: recording -> transcription -> dictionary -> corrections (`Dictation::process`).
- `crates/adapters/*`: sherpa-onnx, microphone (cpal), global key (handy-keys), paste (arboard + enigo), LLM (ureq).
- `crates/app`: the binary; `daemon.rs` (the protocol), `worker.rs` (the transcription thread), and one module
  per feature (`models.rs`, `llm.rs`, `styles.rs`, `snippets.rs`, `history.rs`, `command.rs`, ...).

Settings are in `~/Library/Application Support/VoiceFlow/config.toml` (`crates/domain/src/settings.rs`; every
field has `#[serde(default)]` so an older config keeps loading).

## Adding a feature

1. **Logic in new files.** Rust: `crates/domain/src/<feature>.rs` (pure logic, with tests) and
   `crates/app/src/<feature>.rs` (I/O). Swift: `<Feature>Store.swift` + `<Feature>View.swift`.
2. **Touch shared files as little as possible**, only adding:
   - `daemon.rs`: new variants in `Command` / `Event` / `Msg` and a short `match` arm calling into your module.
   - `settings.rs`: one new field in `Settings` (ideally your own sub-struct with `Default`), at the end.
   - `Features.swift`: one line in `install` and one in `sections`; the sidebar order is `sidebarOrder` in
     `SettingsView.swift`.
3. In Swift, a feature's state lives in its own store (`@MainActor @Observable final class XStore`,
   `static let shared`), which listens with `engine.on("event_type") { obj in ... }` and sends commands with
   `engine.send([...])`.
4. **UI**: dark Liquid Glass. Use the pieces in `Components.swift` and `Glass.swift` (`SectionTitle`,
   `SettingRow`, `GlassSwitch`, `GlassField`, `GlassTextEditor`, `GlassSearchField`, `Chip`, `EmptyStateCard`,
   `ErrorBanner`, `NoteText`, `GlassPillButton`, `GlassIconButton`, `GlassSegmented`, `IconTile`, `HotkeyView`,
   `ChipFlow`, `.glassCard()`). A screen is the content of a section: return a `Group` or `VStack`, with no
   `ScrollView` and no background of its own.
5. **Everything the user sees is in English**: UI text, error and state messages sent by the engine, log lines
   and CLI output. Prompts sent to the LLM are English and language-neutral (the dictated text can be in any
   language and must stay in that language).

Many code comments are still in Romanian, from before the project was opened up. New comments should be in
English; translating old ones as you touch the code is welcome.

## Safety rules

- Anything that deletes or overwrites user data asks for confirmation in the UI and never touches what the app
  did not create. Files are written through a temporary file plus rename.
- User data (history, notes, snippets, dictionary, models) lives in `~/Library/Application Support/VoiceFlow/`.
- Nothing leaves the Mac: no cloud services, no telemetry. The only network access is downloading models from
  Hugging Face when the user asks for it.

## Verifying

```bash
cargo test --release
cargo clippy --release --all-targets      # no warnings
(cd macos && swift build -c release && swift test)
```

- `VF_NO_MIC=1 ./target/release/voice-flow --config <test config> daemon` starts the engine without the
  microphone; send it JSON commands on stdin and read the events. Always use a test `--config` with temporary
  paths, never your real config.
- `macos/.build/release/VoiceFlow --snapshot <folder>` renders the UI to PNG files with sample data
  (`Snapshot.swift`). It opens windows on screen for a few seconds.
- `scripts/build-app.sh --install` builds and installs the app in `~/Applications`.
