# Overlay Studio: professional UI research and architecture proposal

Status: research/design review completed; the user selected Slint UI + Rust engine. The clickable concept is a standalone design preview. Implementation has started with command/domain extraction and an isolated GPU integration target; the complete desktop replacement and full architecture refactor remain in progress.

## Recommendation

Keep the Rust game engine and wgpu/Metal render pipeline. Replace the current egui desktop shell with a deliberately designed, declarative Slint interface. Use Slint for panels, controls, layout, interaction, and accessibility; keep the scoreboard graphics themselves in the existing native renderer.

This is a tooling recommendation, not a claim that Rust inherently produces poor interfaces. The current screen is dominated by generic widgets and lacks a consistent component system and workflow hierarchy. A different toolkit also needs intentional interaction and visual design.

Slint is the best fit **for this application's GPU constraints**, subject to an integration proof before migration. Tauri with TypeScript is the strongest alternative when web component reuse and sharing UI code with the phone controller are the higher priorities. Qt/QML is also viable, but introduces a larger native build/FFI surface.

## Research evidence

| Product | Evidence inspected | Patterns to adapt |
| --- | --- | --- |
| ProPresenter | Official interface guide and its full application screenshot, visually inspected in the browser | Clear Show/Edit separation; compact icon toolbar; organized libraries; thumbnail media bin; dedicated output monitor and show controls |
| ProScoreboard | Official user guide, particularly overview, scoring controls, editor, data links, macros, and remote controller sections | Large scores with adjacent increment/decrement buttons; time edited as displayed time; dedicated operator controls; separate configuration/editor; labels linked to team data |
| Resolume Arena | Official Interface Layouts and Dashboard documentation plus its complete custom-layout screenshot, visually inspected | Distinct output/preview monitors; media cue grid; contextual composition/layer/clip properties; hideable/resizable panels; saved workspace layouts; semantic color coding |
| After Effects | Official workspace, Properties panel, and layer-property documentation; an official illustrated timeline reference, visually inspected | Layer/property tree aligned with a time ruler; diamond keyframes; contextual transform/type controls; property-specific animation; fixed column alignment |
| DaVinci Resolve | Official Edit page, media-pool screenshot, and complete Edit workspace screenshot, visually inspected | Task-based workspaces; media bins, viewer and inspector with a common hierarchy; compact transport controls; aligned timeline tracks; visible selection |
| Additional editor references | Lazyweb search for professional animation editors and live media controls; Rive, Artboard Studio, and VEED references | Useful corroboration for layer/canvas/inspector separation. These are supplementary to the named professional products. |

The ProScoreboard PDF was readable through web extraction, but the browser blocked its PDF viewer and the web screenshot tool returned no inspectable image. Its proposed patterns are based on its documented workflows, not a claim that its screenshots were visually inspected.

Official sources:

- [ProPresenter interface guide](https://support.renewedvision.com/hc/en-us/articles/360041345954-Understanding-The-ProPresenter-User-Interface)
- [ProScoreboard user guide](https://renewedvision.com/downloads/ScoreboardUserGuide.pdf)
- [Resolume interface layouts](https://resolume.com/support/en/layouts)
- [Resolume Dashboard](https://resolume.com/support/en/dashboard)
- [After Effects workspaces](https://helpx.adobe.com/after-effects/desktop/get-started/get-familiar-with-the-interface/workspaces-panels-viewers.html)
- [After Effects contextual Properties panel](https://helpx.adobe.com/after-effects/desktop/animate-in-after-effects/animate-using-the-properties-panel/properties-panel.html)
- [After Effects layer properties](https://helpx.adobe.com/after-effects/desktop/work-with-layers/layer-properties/layer-properties.html)
- [DaVinci Resolve Edit](https://www.blackmagicdesign.com/products/davinciresolve/edit)

## What the attached screenshot reveals

Each layer occupies a tall row of loosely aligned buttons, menus and sliders. There is no labeled horizontal time scale, and animation properties compete with layer navigation. Font and shape tools are exposed as generic fields instead of a familiar editor. Controls have similar visual weight, making important actions difficult to identify. The solution is a workflow and component redesign across the app, rather than a darker theme or an icon replacement alone.

## Toolkit comparison

| Option | UI strengths | Fit with the existing engine | Main tradeoff |
| --- | --- | --- | --- |
| **Slint declarative UI + Rust** | Purpose-built reusable components, styled controls, compiled declarative layout, native input and accessibility facilities | Documented wgpu 30 integration can use a shared device/queue and import an external GPU texture | Version-specific GPU integration is explicitly unstable; custom editor/docking/timeline behavior still needs implementation |
| **Tauri + TypeScript (React or Svelte) + Rust** | Broad HTML/CSS component ecosystem, flexible styling, SVG icons, strong web tooling, potential reuse with mobile controller | Rust core can own all state and native NDI/Syphon rendering; frontend communicates through commands/events | The webview is a separate rendering environment. A native canvas preview needs a proved bridge or another render implementation; naïve full-frame IPC/readback is unsuitable |
| **Qt Quick/QML + Rust through CXX-Qt** | Mature desktop toolkit, declarative UI, GPU scene graph, desktop input and panel patterns | Native Metal/scene-graph integration is possible, with Rust behind an FFI boundary | More complex C++/Qt build, packaging, interop, and licensing choices |
| **Redesign egui in place** | Smallest migration; renderer/context already connected | Easiest preservation of current canvas/output behavior | Requires custom controls and a strict design system; retains immediate-mode UI/state coupling unless the architecture is refactored |

Sources: [Slint wgpu integration](https://docs.slint.dev/latest/docs/rust/slint/wgpu_30/), [Slint texture requirements](https://docs.slint.dev/latest/docs/rust/slint/wgpu_30/enum.TextureImportError), [Tauri process model](https://v2.tauri.app/concept/process-model/), [Tauri IPC](https://v2.tauri.app/concept/inter-process-communication/), [Qt Quick scene graph](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html), [CXX-Qt](https://kdab.github.io/cxx-qt/book/).

Slint's free desktop license has an attribution requirement; its GPU API should be pinned to the tested toolkit/wgpu versions. [Official license overview](https://github.com/slint-ui/slint/blob/master/LICENSE.md). The user selected Slint. An isolated target pins 1.18.1 and wgpu 30, compiled from the official crates.io packages; it has its own app identity and does not replace the working production UI.

### Preserving rendering

The current output texture is `Bgra8Unorm`. Slint's documented image-import API accepts `Rgba8Unorm` or `Rgba8UnormSrgb`, not BGRA. Therefore the actual NDI/Syphon texture must stay BGRA, while the desktop preview uses a separate RGBA render target or a GPU conversion pass. Both are GPU operations; neither requires sending the output frames through a webview or a CPU image stream. Premultiplied alpha, texture ownership, resource lifetimes, and color-space handling require an actual integration test.

Keep the existing scoreboard painter initially, including its internal egui context and installed-font handling. Replacing the desktop controls does not require replacing the output renderer. Abstract its scene inputs and GPU resources behind a render adapter. UI chrome never enters the transmitted texture.

No framework documentation proves unchanged frame timing on this Mac. Before migrating, measure the current engine and the candidate UI under the same two-output animated project, with UI idle, active dragging, resizing, media import, AI work, minimized windows and remote commands. Accept the candidate only after alpha/font/animation parity and frame-time/CPU/GPU/readback behavior have been measured.

## Proposed workspaces

### Design

- Narrow creation tool rail with Select, Rectangle/shape menu, Text, Image/Video, Pan and alignment tools. Icons have tooltips, shortcuts, visible selected state, and accessible names.
- Left sidebar has Designs, Layers, and Assets tabs. Thumbnail designs, named/grouped layers, visibility and lock controls. The active selection is synchronized with the canvas and inspector.
- Center canvas gets the most space. Rulers, zoom/fit controls, snapping feedback, selection handles and inline text editing. Alt-drag preserves group relationships; ordinary click and drag have predictable behavior.
- Right inspector is contextual: transform fields in aligned pairs; text family, style, size and alignment together; shape fill/stroke/corners; media source and fit; live-data binding with friendly team/clock names.
- Data binding shows “Team 3 → Score” and its live value, rather than making users infer a raw internal key.

### Animate

- Same canvas and selection model as Design.
- Bottom panel has fixed 28–32 px layer rows, a labeled seconds/frame ruler, a movable playhead, visible diamond keyframes and aligned tracks. Expand a selected layer to reveal properties; avoid showing every numerical field for every layer.
- Transport is Play/Pause, restart, current time, loop range and zoom. Edit a selected keyframe in the inspector. Support keyframe drag, multi-select, copy/paste, delete, easing and undo as first-class interactions.
- Events panel uses a readable rule builder: **When → Source → Play cue → Output**, with enabled state, preview, run now and conflict/error feedback.

### Live

- A clearly labeled PROGRAM monitor displays the actual active output; a PREVIEW monitor represents the pending design. “Take” is explicit and the active cue/output status is always visible.
- Team score cards use consistent colors, names, prominent scores and grouped point actions. Scoring configuration is in a separate setup panel; live operation does not expose destructive configuration fields.
- Named clock cards show MM:SS, running/paused state, Start/Pause, reset and time adjustment. Clicking time opens a friendly time editor; reset confirmation names the clock and target time.
- Cue tiles show a thumbnail, cue name, media duration/animation type, active state and trigger shortcut. Show/hide/clear output actions are distinct and labeled.

### Remote

- Device preview with phone/tablet modes, a button palette, drag handles, grid snapping and an action inspector. Add/duplicate/remove actions have clear affordances.
- QR pairing appears in a connection panel, with current address, paired/access state and revoke controls.
- Live phone view has touch-sized controls and remains simple. Desktop precision controls and compact timeline density do not carry over to mobile.

Outputs, project setup, team configuration and application preferences have their own focused panels/dialogs. They do not consume permanent canvas space.

## Visual and asset system

- Restrained charcoal surfaces with small differences between canvas, panels and fields; visible borders and focus rings.
- One cool selection accent; green indicates live/running, amber indicates queued/caution, red indicates stop/error. State also has text/icon cues.
- Desktop typography: 12 px secondary labels, 13–14 px controls, 16–18 px section titles. Live scores and timers receive larger typography. Touch controls are at least 44 px; desktop controls are usually 28–32 px tall.
- Spacing tokens based on 4/8/12/16 px; aligned property columns; consistent radii; flexible/resizable panels; tested on the user's desktop scale.
- Use bundled **Lucide SVG icons**, downloaded from a pinned official revision with license notices. Material Symbols is another valid option, but use one coherent family instead of mixing systems. Avoid emoji and ad hoc Unicode tool symbols.
- The review prototype uses real local SVG icons and code-native geometric scoreboard/thumbnail assets. Product screenshots are research references, not copied artwork for the app.

## Architecture audit and refactor

Current evidence: `app.rs` is 2,793 lines and `Studio` mixes UI state, project mutations, undo, storage dialogs, clocks, output control, network command dispatch, AI setup and media workers. `model.rs` depends on `remote::Action`/`ControllerLayout`, putting a transport concern inside the domain. Output scheduling is called from the desktop UI update loop. Project copies include media bytes and are used for per-frame edit detection/history. These are concrete maintenance and performance boundaries to address.

Target boundaries:

```text
presentation/         Slint components, editor gestures, view models, desktop composition root
application/          Command handlers, transactions, undo/redo, cue coordination, session snapshots
domain/               Project, designs, teams, clocks, cues, stable IDs, validation, animation math
render/               Scene painter adapter, font/resource cache, offscreen canvas, GPU preview bridge
infrastructure/       Project repository/migrations, media conversion, local AI, remote HTTP
outputs/              Output manager, Syphon adapter, NDI adapter, sender lifecycle and pacing
```

Dependency rules:

1. Domain types contain no UI, web server, filesystem, subprocess, or GPU dependencies. Actions/controller configuration belong to domain/application types, not the HTTP module.
2. Every desktop, remote and AI-assisted operation passes through the same validated application command API. UI callbacks translate input; they do not mutate project fields or transmit graphics directly.
3. Application emits domain events and output intents; output/media/AI/file adapters perform side effects. Inject ports only at meaningful boundaries, rather than creating an interface for every struct.
4. Stable typed IDs replace indices for cross-document references. Persistence has an explicit schema migration, with old projects and portable designs retained as compatibility fixtures.
5. Separate `EditorState`, `GameSession`, `DocumentSession`, `OutputManager`, `RemoteSession` and `JobCoordinator`. The composition root wires them together. Split by responsibility, not by arbitrarily moving large functions into new files.
6. Use structured errors with useful user messages; remove unchecked externally influenced indexing/unwraps. Jobs support progress, cancellation, bounded queues and clean shutdown. Memory/GPU resources have explicit ownership.
7. Undo stores commands/deltas or inexpensive snapshots sharing immutable assets. Idle UI frames do not copy every image/video asset. Distinguish committed document edits, ephemeral selection, running clocks and frozen program state.
8. Output pacing and game time continue independently of UI rendering and modal dialogs. Respect Syphon/macOS thread-affinity requirements when isolating the render scheduler; do not blindly move native APIs onto a worker.
9. Extract a shared renderer once; keep preview and output semantics aligned, including fonts, alpha and keyframes. Test it independently of the desktop toolkit.
10. Keep code formatted, functions focused, names explicit, contracts documented and dependency direction enforced. File-count reduction or green lint alone is not proof of clean architecture.

## Implementation sequence after the research/toolkit review

1. Establish existing project compatibility fixtures and a rendering/performance baseline.
2. Extract command/state/port boundaries behind the existing UI, preserving behavior and output reception.
3. Prove the proposed UI's GPU preview bridge and input coordinate mapping in an isolated target.
4. Build shared design tokens and controls, then Design and Animate workspaces, then Live and Remote. Keep the existing production build available through the transition.
5. Replace the old desktop shell only after end-to-end parity and usability checks pass; update architecture docs and graph.

Completion gates include save/open/undo, installed-font replacement, Alt-copy/groups, selection/resize at different zoom/DPI values, keyframe selection and playback, five-team and multi-clock live operation, manual/event cue triggers, QR pairing/remote control, actual NDI/Syphon alpha reception, and sustained output while UI/media/AI work is active. Every requested feature remains in scope.

## First implementation checkpoint

### Live engine checkpoint

The Slint target now has configurable team steps, independent clock setup/operation and native sender lifecycle controls. `game_session` coordinates events, `game_setup` validates configuration, a main-thread timer schedules outputs independently of window redraws, and native file dialogs/file jobs are asynchronous. Program renders the frozen taken design and its assets; Preview follows the selected output's pending design. View models retain identity and preserve editable drafts during runtime refresh. Document generation and preview identity invalidate stale texture/resource caches.

Native validation on October 11 received actual 1920 × 1080 Syphon and NDI with alpha 0–255. The optimized build measured 30.1 fps over eight seconds with both senders running, and 30.0 fps with a Save as sheet open. NDI Hide sent fully transparent frames at 30 fps. Transmitted pixel sampling verified that queueing another design did not take it until Take was pressed. Debug measured roughly 12 fps; alpha conversion was moved off the UI thread and pacing now advances absolute deadlines. This is measured evidence for those cases, not full performance parity under media/AI/drag/minimized-window load.

Five teams, custom fifth-team steps and two independent clocks were operated and persisted through the native UI. The main project passes 40 tests, the Slint projection target passes two tests, and both targets pass strict Clippy. Adaptive layout generation, complete Design/Animate/Remote feature migration and the other completion gates remain open.

### Connected desktop checkpoint

`desktop/slint/` now contains the actual Slint presentation target, separate from the isolated GPU proof. Design selection, native GPU canvas, direct shape creation, layer selection/movement, transform/text/binding/font inspector, font search, duplicate/delete, undo/redo, file open/save and live score commands are wired to the shared engine. Its document transaction module validates edits and stores design history without cloning media assets for each pointer update. The native UI was launched and inspected; initial paired transform fields overflowed the inspector and were corrected before the checkpoint.

An additional Lazyweb search on October 11 found a Rive editor marketing screenshot, which was inspected visually. It supports the already approved central canvas plus narrow layer/inspector hierarchy; the small embedded editor image is insufficient for precise typography measurements. Reference: https://rive.app/editor. The prior detailed product reference research and full completion gates still apply. The current target remains an explicitly labeled migration build; it has not passed full workflow or output performance parity.

- Controller commands and layout definitions now live in `src/domain/controls.rs`; the HTTP adapter retains compatibility exports. Serialized project field names and action variants are preserved.
- `src/application/game_commands.rs` validates and applies game/cue commands, returns domain events and explicit output requests, and has no desktop/network/GPU calls. Desktop and remote actions use this handler.
- `src/lib.rs` exposes the core and adapters to alternate desktop targets. The original desktop executable remains available.
- The isolated target at `experiments/slint-preview/` compiles Slint 1.18.1 with the existing scoreboard painter and imports an RGBA GPU texture on the shared device/queue. Native score controls dispatch through the extracted command API. This is an integration proof, not the final redesigned app.
- The first extraction passes all 29 existing/new tests, including native Metal transparency and actual video import. Full refactor, UI feature migration, output timing and final rendering parity remain open gates.
- The native Slint proof was launched and its +1 control updated both its displayed score and native GPU scoreboard pixels. A separate Metal test verifies transparent background pixels, opaque RGBA channel order, and no repeated preview render for an idle document. The proof and main app both pass strict Clippy checks. These checks do not establish full NDI/Syphon frame-timing parity under UI load.
