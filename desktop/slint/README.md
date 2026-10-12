# Slint desktop migration

This is the connected desktop implementation being developed toward the full UI plan in `design/ui-v2/RESEARCH.md`. It uses the real project model and scene painter, not the simulated browser proposal. It is not yet the replacement show app.

## Run

From the main Overlay Studio folder:

```sh
cargo run --release --manifest-path desktop/slint/Cargo.toml
```

For a separate macOS bundle:

```sh
cargo build --release --manifest-path desktop/slint/Cargo.toml
python3 desktop/slint/scripts/bundle.py --profile release
```

The resulting `desktop/slint/dist/Overlay Studio Slint.app` has its own bundle identity. The original app and user projects are not replaced. `CARGO_TARGET_DIR` can point to `experiments/slint-preview/target` to reuse the pinned integration proof's build cache; provide that same variable to the bundle script.

## Connected actions

- In Design, Local AI layout opens a private prompt panel beside the canvas. First setup installs the local Ollama/Qwen runtime; generation arranges the selected visible unlocked layers, or the whole canvas when nothing is selected. A proposal replaces the GPU canvas only for review. Apply commits one validated Undo step, Discard preserves the source, and stale proposals cannot overwrite a changed document. Cancel kills and reaps active download, model-pull and generation subprocesses. Malformed geometry is retried before a safe balanced proposal is offered.
- Use Animate for a compact seconds ruler, layer tracks, keyframe pose/easing edits, entrance presets/duration/loop, preview transport, diamond timing drags, multi-frame selection, copy/paste and delete. Edits use the shared validated document history and persist in projects. Preview playback pauses when leaving Animate. Take design publishes animation edits to senders.
- In Live, scroll below the clocks to LIVE TEXT. Apply or Enter commits a custom value; Revert restores its current value. Drafts survive runtime refreshes and are cleared when a different project replaces the current document.
- In Animate, click Events to edit score/clock rules. Choose When, Source, Then and its output/design or value. New and duplicated rules start disabled. Enabled rules run automatically; Run now also works for disabled rules. Rule deletion removes linked remote buttons, and one Undo restores both without reverting live values. See [Events instructions](../../docs/manual/slint-events.md).
- Click a visual color swatch for layer fill/text, stroke, team colors or remote-button colors. The shared picker has a hue bar, saturation/brightness area, twelve presets and a draft preview. Layer colors include opacity; Apply commits and Cancel discards. Hex entry is no longer required.
- Select existing designs and layers.
- Draw rectangles, ellipses, triangles, diamonds, lines and text directly on the fitted canvas. Click places the default size; drag defines bounds.
- Drag selected layers with optional 10 pixel snapping. Geometry uses canvas coordinates at the current fitted scale.
- Edit position, size, rotation, opacity, text, live field binding, installed font, font size, hidden and locked flags. Press Enter to commit text/numeric fields.
- Search installed fonts, duplicate or delete a selection, undo and redo canvas edits.
- Open existing `.overlay` projects through a backward-compatible migration. New saves add stable design/output identities and reference metadata so output routes, cue targets and remote buttons still point to the intended items after array reordering. Confirm replacement of unsaved work or closing without saving.
- Dirty projects receive versioned background recovery snapshots about every 15 seconds. The launch prompt can Recover, Discard or defer with Later. Restored projects open as unsaved with clocks paused, outputs stopped and remote access stopped; a successful normal save clears recovery.
- Use live score controls through the shared validated game command handler.
- Undo/Redo covers team, clock and output configuration as well as canvas, controller and event rules. It preserves unrelated live values and elapsed time. Restored removed clocks start paused; undoing a current-time or direction edit pauses the affected clock. Stop an affected output before undoing its stream name/protocol/frame rate or removing an added output.
- Use Game setup to add/remove teams and clocks, set team labels/names/colors, and enter 1–12 comma-separated nonzero scoring steps per team. The live buttons use those steps. Pause a clock before editing its current time; enter times as minutes:seconds.
- Start/pause/reset individual clocks, choose count up/down, set current/reset times, and add or remove seconds. Reset confirmation names the clock and duration. Other running clocks retain their state when one is added or removed.
- Configure independently named Syphon/NDI outputs, choose a frame rate and pending design, start/stop, Take design, Show/Hide and replay the taken animation. Choose the output in the monitor selector; Preview follows its pending design while Program retains its taken design until Take.
- Keep live clock/output scheduling active during asynchronous native file/reset dialogs. File parsing/writing runs on workers. Project replacement stops clocks and senders deliberately.
- Create blank designs, duplicate a design, import/export portable designs, edit canvas size/name, and generate a scoreboard for the configured teams. Drag palette items, resize/rotate handles, group selections, align layers, and edit fill/stroke, rounded corners and text alignment. Use the visible controls while the modifier-shortcut hardware checks remain open.
- Use Remote to arrange browser buttons on a 12-column, 48-row grid. Add, duplicate, delete, drag and edit exact placement, size, label, color and confirmation. Build team controls derives buttons from the current scoring steps while preserving custom actions. Layout edits use document Undo/Redo without reverting live scores.
- Configure all remote actions: adjust/set a live field, toggle/reset/adjust a selected clock, show/hide an output, take a design, replay output animation, or run an existing event cue.
- Start remote control on a selected LAN interface and port, scan its authenticated QR, or copy the complete pairing link. Stop revokes access. New/Open and application shutdown stop the server. The existing responsive browser controller works with this Slint host.

Preview imports an RGBA texture on Slint's device/queue and uses the native scene painter. It has no CPU pixel readback. NDI/Syphon rendering remains in the existing native output adapter. Selection bounds rotate with the artwork, using Slint's [common transform properties](https://docs.slint.dev/latest/docs/slint/reference/common/).

## Architecture

`src/application/document_session.rs` owns canvas transactions, selection and undo/redo. A gesture records one before/after design, without copying embedded image/video bytes on each movement. Invalid geometry and missing live bindings are rejected before history is committed. `desktop/slint/src/main.rs` translates callbacks into commands and projects state into view models; `renderer.rs` adapts the native scene painter to Slint's GPU texture. Scene, fonts and asset caches stay outside declarative UI code.

`game_session.rs` coordinates running clocks and enabled cue rules; `game_setup.rs` validates configuration before committing it. The desktop `outputs.rs` owns native senders on the Mac's main thread and runs from an 8 ms timer independent of canvas redraws. Absolute frame deadlines avoid lowering the requested average rate through timer rounding. NDI alpha conversion runs on the bounded network worker rather than blocking the UI scheduler. `files.rs` handles asynchronous native dialogs and file jobs. `view_models.rs` preserves row identity and editable drafts during runtime updates. `remote_panel.rs` projects controller state and dispatches the authenticated server queue into the same game session; a bounded request-result cache prevents retry IDs from applying an action twice. Browser button IDs are decimal strings to preserve all 64-bit IDs; older numeric requests remain accepted. Document generation and preview identity invalidate renderer caches when a project or monitored design changes.

## Verified checkpoint

The connected native timeline created frames at 0 and 2 seconds, changed the second pose to X=600, copied it to 4 seconds, and saved/reopened all three in the optimized build. Dragging the second frame to 3.01 seconds and one Undo restored 2 seconds. Playback advanced and stopped at the endpoint; Play restarted it. Show base pose changed a sampled native preview pixel from the animated panel color (19,28,43) to the static checkerboard (55,59,71), confirming the cache transition. `design/ui-v2/slint-connected-animation.png` shows the reopened timeline. Canvas pose handles, richer timeline interaction, and the event builder still need their full migration/verification pass.

The color-picker native check applied a purple fill at 50% alpha and a teal stroke, reopened the saved project, and confirmed both persisted. A green team color and orange remote-button color were also applied and inspected in the saved project. Clicking swatches, preset selection, hue/color-area dragging and Cancel were exercised. The isolated capture is `design/ui-v2/slint-color-picker.png`; the manual now shows the visual dialog and updated remote inspector.

On October 11, the native Slint bundle was launched, inspected and used to draw a rectangle, edit a numeric transform, move with snapping and restore the position with one Undo. A new verification project was saved into the separate documentation capture folder and reopened through the native file dialog; its added layer and geometry were preserved. Live +1 changed the shared score from 12 to 13. The inspector overflow and stale binding dropdown discovered during this check were corrected and rechecked. Layer IDs cross the Slint model as decimal strings to avoid narrowing existing 64 bit IDs.

The shared library passes 61 tests and the Slint target passes nine tests. They cover transactions, rejected edits, project cache invalidation, clock completion/limits, five-team setup, preserved clock runtime, coarse-timer frame pacing, controller history, full-width browser IDs, animation transactions, cue deletion/undo, enabled versus manual cue execution, live text changes, stable view models, atomic layout acceptance, subprocess cancellation, bounded fallback geometry, recovery validation, safe stopped-runtime restoration and stable references across reordered project arrays. These checks do not prove the remaining full-show migration gates complete.

The native Slint assistant opened and prepared the already-installed private model. A full local-model diagnostic then generated validated positions for all five deliberately overlapping text layers. An initially invalid native proposal left the source untouched; the repaired path now retries those responses and has a tested balanced fallback. Apply/Discard and sustained-output-under-generation still need the final native stress pass before replacement.

The optimized native Slint build sent actual 1920 × 1080 Syphon and NDI frames with alpha 0–255. With both senders running, an eight-second NDI capture measured 30.1 fps; during an open native Save as sheet it measured 30.0 fps. Hide produced alpha 0–0 at 30 fps. A sampled transmitted pixel remained transparent while Corner score was pending, then became opaque after Take. The original debug build measured approximately 12 fps and is not the performance baseline. Full media/AI/interaction/minimized-window stress and all supported frame rates still need measurement.

The native UI also created five teams and Clock 2. Team 5 was configured with −1, +1, +5 and +10; its +5 control produced score 5. Clock 2 ran from 02:30 to 01:28 and paused while the primary remained paused at 02:15. The resulting five-team/two-clock configuration was saved and inspected. The output dropdown was rechecked against design-tile selection after switching the preview monitor. The current screenshot is `design/ui-v2/slint-connected-live.png`; it shows these controls with the existing two-team Game night artwork, not an adaptive five-team graphic.

Receiver diagnostics can target a specific sender:

```sh
python3 scripts/verify_ndi.py --source 'Slint Validation NDI' --seconds 8
python3 scripts/verify_ndi.py --source 'Slint Validation NDI' --alpha hidden
cargo run -- --verify-syphon --source 'Slint Validation Syphon'
```

## Remaining migration gates

The full goal remains open. Still required: full modifier-shortcut hardware verification, broader configuration-history and compatibility stress, sustained output during local-AI generation, the remaining timeline/canvas interactions and full native event/output verification, and complete UI/error shutdown checks. The remote workspace still needs its full action/output and phone/tablet stress pass, resize handles and live drag feedback. Rulers, assets/thumbnails and other approved UI details remain to be completed.

Before replacing the existing app, verify old project/design compatibility, five-team and multiple-clock workflows, actual NDI/Syphon alpha reception, sustained output during UI/media/AI work, memory/resource cleanup, different DPI/zoom sizes, and all actions listed in the manual. The illustrated manual includes the original app and a Slint Design/Live/Remote/Animate supplement with Events and live text; continue recapturing it as the remaining workspaces reach parity.

The isolated native remote check changed the first button to Eagles +5, and its paired browser raised the score from 12 to 17. The same browser started and paused the primary clock. Dragging the button to row 10 and one Undo returned it to row 0. The controller action and score survived Save/Open, and Open stopped the old server; the browser reported Disconnected and disabled its buttons. These are specific checkpoint results, not proof of every remote action or physical-device workflow.

The final protocol check dispatched button ID 9007199254740993 as a string. Repeating its request ID applied +5 only once, raising 17 to 22; unauthenticated state access returned 401. Native Duplicate/Delete/Undo restored a removed copy. The inspector width and tall-grid origin discovered during verification were fixed and the final grid was recaptured in `design/ui-v2/slint-connected-remote.png`. That screenshot’s QR has been revoked.

Native configuration history check: output name Undo/Redo retained score 13 and a running clock; a running Syphon sender blocked stream-setting Undo without consuming history, and Stop output allowed it. Shared tests also cover clock removal/restoration and references restored with team deletion.

Design now imports PNG/JPEG images and FFmpeg-converted video through asynchronous native dialogs and worker preparation. Embedded assets travel in projects/design files; layer Undo/Redo preserves live values. New imports preserve aspect ratio and have no entrance preset. Animate previews full clip duration independently of entrance duration. Decoder pixels are shared across preview and senders. Native verification imported both fixtures, restored an image through Undo/Redo, played a two-second clip, and saved both embedded assets.

Media preparation now exposes Cancel import. Video conversion has a 120-second watchdog and bounded stdout/stderr readers; cancel kills and reaps the converter. Closing during preparation requests cancellation, waits for cleanup and continues the normal unsaved-work close flow. Native checks cancelled a 78 MB source without adding a layer, then closed a second active conversion; both the isolated app and FFmpeg were absent afterward. The shared real-video regression imports and saves a clip longer than ten seconds.

Game setup now adds and removes custom live fields. Removal repairs bindings/cues/controller references in one history action; repeated Undo/Redo preserves the latest removed live values. Running programs protect their referenced fields, including team/clock removal and history. Binding choices use friendly labels and an index adapter, so a field named Custom text remains distinct from Literal text. Native checks created/bound/removed/restored that field, saved and reopened BONUS ROUND, rejected its removal while Syphon used it, and confirmed a one-second clock reached 00:00 / Paused with no sender or remote server running. Setup card heights were increased to prevent overlapping controls.
