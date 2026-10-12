# Overlay Studio

Standalone Rust scoreboard designer with a GPU canvas, live game controls, independent transparent Syphon/NDI outputs, and a customizable phone/iPad controller.

## Run

The connected Slint desktop migration is in [`desktop/slint`](desktop/slint/README.md). It supports canvas editing and styling, adaptive team designs, project/design files, configurable teams/scoring/clocks, transparent Syphon/NDI senders with Program/Preview controls, and Remote with an editable button grid and authenticated QR pairing. The original app remains the full-featured show build until the remaining migration gates pass. Use the optimized Slint release build for live output validation.

The [illustrated instruction manual](docs/manual/output/Overlay%20Studio%20Instruction%20Manual.pdf) has 30 pages with clickable contents, step-by-step actions, actual native screenshots, and a Slint Design/Live/Remote/Animate supplement including event rules, live text, media imports and all nine remote action types. An editable Word copy is available in the same folder. Demonstration QR codes have been revoked; use the fresh QR in the running app.

From this folder:

```sh
cargo run --release
```

From the enclosing Resolume folder:

```sh
cargo run --release --manifest-path overlay-studio/Cargo.toml
```

On macOS, build a double-clickable application:

```sh
cargo build --release
python3 scripts/bundle.py
```

The app is created at `dist/Overlay Studio.app`. Syphon.framework is bundled locally. The NDI runtime is loaded dynamically from an installed runtime; the app also recognizes the existing Resolume Arena runtime. For another location, set `OVERLAY_NDI_LIBRARY` to the library's absolute path. The NDI runtime is not redistributed with this source.

## Canvas Studio

Choose a design or add a blank canvas. Select a creation tool, then click to place or drag to draw text, rectangles, ellipses, triangles, diamonds, and lines directly on the canvas. You can also drag from the palette. V selects the move tool; R, O, T, and L select rectangle, ellipse, text, and line. Escape cancels a creation tool. Import images and short video clips from the Create palette. Shift-click selects several layers, Alt-drag copies while moving, Command-D duplicates, groups move together, arrow keys nudge, Shift resizes proportionally, and middle-drag pans. **Smart arrange** balances selected layers on a local grid. The inspector provides precise geometry, color, stroke, opacity, live bindings, and a searchable installed-font picker with readable weight/style names. Older saved font-face names remain supported.

The checkerboard and editing handles are never transmitted. Text fits its bounding box. Designs use named values, so the same game can drive several visual layouts.

## Live Desk

Add or remove teams, rename them, choose their colors, and define any scoring steps such as `-1`, `+1`, or `+5`. **Create layout** generates a new scoreboard that adapts from one to many teams. Add multiple independent clocks, enter current/reset times as minutes and seconds, choose count up/down, and adjust live time in seconds. Each output has its own stream name, protocol, frame rate, and design. Start an output, then choose a replacement design and press **Take design**. Edits remain in the designer until a design is taken. **Hide** sends transparent frames; **Stop** removes the sender. **Play cue** replays the design's entrance animation. Changing canvas dimensions requires restarting the output.

## Animations and cues

The **Animations & cues** workspace combines a canvas preview, a scrubber, and a layer timeline with event automation. Add keyframes at the playhead and edit time, X/Y, width/height, rotation, opacity, and easing. Enable Loop for repeating animation. Presets remain available for quick None, Fade, Slide, and Pop entrances; keyframes take precedence when present. Play, Pause, Restart, and Exit preview control playback without changing the live program.

Video imports are converted on a background worker to self-contained motion layers (up to 15 seconds) so they remain portable in project and design files; importing requires a local FFmpeg installation, while playback does not. Motion frames are decoded with memory limits and shared between output contexts. Current playback uses a 640×360 preview ceiling and GIF color depth; longer, full-resolution video playback remains a development item.

Rules can react to a score being added/removed or a clock starting, resetting, or reaching zero. A rule can replay an output's animation, take another design (including a video cue design), or set a live value. **Run cue now** plays a cue manually, and remote buttons can use **Run named cue**. Removing a team or clock also removes its associated actions and updates remaining clock references. Undo/redo pauses clocks to avoid replaying elapsed time against restored state.

## Local AI layout

Press **Local AI layout…** in the Slint designer (or **AI layout…** in the original UI), then **Set up local AI**. Setup downloads the official Ollama runtime and Qwen3 0.6B model into `~/Library/Application Support/Overlay Studio/local-ai/`; it does not require a cloud account. Setup and inference run on a worker thread, so the scoreboard continues updating. After installation, describe the desired layout and press **Suggest layout**. Select specific layers first, or leave the selection empty to organize the canvas. The suggestion is validated and rendered as a temporary GPU canvas preview; **Apply layout** creates one undoable edit, while **Discard** preserves the source. Suggestions cannot overwrite a source design that changed while the model was working.

The assistant uses Ollama's [structured generation API](https://docs.ollama.com/api/generate) and the [Qwen3 0.6B model](https://ollama.com/library/qwen3:0.6b). Only layer descriptions are sent to a loopback service on this computer; the engine started by the app exits with the app. Model files remain installed for the next launch. Cancel terminates and reaps the active setup or generation subprocess. Invalid model geometry is retried and then replaced by a validated balanced proposal, so the source design is never partially changed. The original UI's grid-based Smart arrange tool remains available without installing AI.

## Phone / iPad controller

1. Open **Remote layout** and choose the computer's local network address.
2. Press **Start remote control**. Scan the QR code using the phone/iPad camera. Both devices must have access to the same local network.
3. The browser shows live scores, clock and output status. Buttons run the configured desktop actions.
4. Arrange buttons by dragging them in the desktop layout grid. **Build team controls** creates a responsive control set for every configured team, including five-team competition layouts. Use the inspector for labels, colors, positions, sizes, optional confirmation, and actions: adjust a value by any step, start/pause/reset any clock, add/remove clock time, show/hide an output, take a design, play an animation, or set a custom value.
5. Save the project to preserve the controller layout. Press **Stop remote / revoke QR** to stop access; restarting generates a new pairing link.

The controller is served directly by the desktop app without a cloud service or phone app. Pairing uses a random per-session token; unauthenticated clients cannot read live state or issue commands. It supports concurrent paired devices and disables controls when disconnected. Commands carry request IDs to prevent duplicate application. An unconfirmed action is reported explicitly rather than silently retried.

## Files and recovery

`.overlay` projects store game values, designs, embedded image assets, output settings, and remote-button layouts. New saves include stable identities for designs, outputs and clocks so cue and remote-button targets survive array reordering; older project files are migrated when loaded. `.scoredesign` files export reusable designs and their images. Installed fonts are referenced by face name rather than embedded. Missing fonts fall back for preview and are reported so a replacement can be selected. Saves use a temporary file followed by atomic replacement.

Autosave recovery is stored in `~/Library/Application Support/Overlay Studio/`. The Slint build writes a separate versioned snapshot on a worker about every 15 seconds while the document is dirty. Its launch prompt offers Recover, Discard and Later. Recovering opens the draft as unsaved, and reopening or recovering keeps timers, outputs, and the remote server stopped. A successful normal save clears the snapshot. Rust runtime errors are recorded in `crash.log` in that directory.

## Validation

```sh
cargo test
node --check web/controller.js
```

Tests cover project serialization, shared value bindings, malformed-project rejection, timer timing, rotation hit testing, alpha conversion, NDI C structure layout, controller-grid validation, QR generation, HTTP authentication/action dispatch, and a real Metal render/readback regression checking transparency.

With outputs running, verify actual reception and alpha:

```sh
python3 scripts/verify_ndi.py
cargo run -- --verify-syphon
```

The NDI diagnostic includes loopback discovery so it can distinguish successful transmission from a separate mDNS/source-discovery problem in a receiving application.

Run `cargo run -- --verify-local-ai` to exercise runtime/model setup and actual local generation. This diagnostic installs the same runtime/model used by the UI. Performance targets, longer video playback, and alpha compositing in external receiving apps require further testing on the target setup. The application bundle uses local ad-hoc signing; it is not a notarized distribution release.
