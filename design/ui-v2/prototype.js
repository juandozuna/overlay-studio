"use strict";

// Standalone design-review prototype. All values are simulated; no desktop API is called.
const state = {
  workspace: "design",
  selectedLayer: "Home name",
  playing: false,
  teams: [
    { name: "Eagles", score: 12, color: "#4f91c8" },
    { name: "Lions", score: 9, color: "#d68b63" },
    { name: "Falcons", score: 7, color: "#8d86c9" },
    { name: "Knights", score: 11, color: "#62a699" },
    { name: "Royals", score: 8, color: "#c09c5a" },
  ],
  scoreHistory: [],
};
const layers = [
  "Scoreboard group",
  "Home name",
  "Home score",
  "Game clock",
  "Away score",
  "Away name",
  "Background",
];
const workspace = document.getElementById("workspace");
const icon = (name, label = "") =>
  `<img src="assets/icons/${name === "trash-2" ? "trash" : name}.svg" alt="${label}">`;

function scoreboard() {
  return `<div class="scoreboard-art"><div class="home"><span>EAGLES</span><strong>${state.teams[0].score}</strong></div><div class="clock-center">08:42</div><div class="away"><strong>${state.teams[1].score}</strong><span>LIONS</span></div></div>`;
}

function toolRail() {
  const tools = [
    ["mouse-pointer-2", "Select (V)"],
    ["square", "Shapes (R)"],
    ["type", "Text (T)"],
    ["image", "Image / video"],
    ["hand", "Pan (H)"],
  ];
  return `<aside class="tool-rail" aria-label="Canvas tools">${tools.map(([name, label], index) => `<button class="icon-button ${index === 0 ? "active" : ""}" title="${label}" aria-label="${label}" data-tool="${label}">${icon(name)}</button>`).join("")}<span class="divider"></span><button class="icon-button active" title="Snap to guides" aria-label="Snap to guides">${icon("magnet")}</button></aside>`;
}

function assetTiles() {
  return `<div class="asset-grid"><div class="asset asset-winner">${icon("video")}<span>Winner sting <span class="muted">0:06</span></span></div><div class="asset">${icon("image")}<span>Team logo</span></div></div>`;
}

function editorSidebar() {
  return `<aside class="sidebar"><div class="mini-tabs"><button class="active">Layers</button><button>Designs</button><button>Assets</button></div><div class="section"><div class="section-title">Current design<button class="icon-button" title="New design">${icon("plus", "New design")}</button></div><div class="design-thumb"><div class="thumb-score"><span>EAGLES 12</span><span>08:42</span><span>9 LIONS</span></div></div><div class="design-label"><span>Broadcast lower third</span><span class="muted">1920 × 1080</span></div></div><div class="section"><label class="search">${icon("search")}<input aria-label="Find a layer" placeholder="Find a layer"></label></div><div class="layer-list">${layers.map((layer, index) => `<button class="layer-row ${layer === state.selectedLayer ? "active" : ""}" data-layer="${layer}">${index === 0 ? icon("chevron-down") : '<span class="indent"></span>'}${icon(index === 0 ? "layers" : index === 6 ? "square" : "type")}<span>${layer}</span>${icon("eye", "")}</button>`).join("")}</div><div class="section"><div class="section-title">Project assets <span class="muted">2 files</span></div>${assetTiles()}<button class="subtle" style="width:100%;margin-top:10px">${icon("plus")}Import media</button></div></aside>`;
}

function canvas() {
  return `<div class="canvas-toolbar"><span>Broadcast lower third <span class="muted">/ ${state.workspace === "animate" ? "Animation preview" : "Canvas"}</span></span><div class="tools"><button class="icon-button" title="Show guides">${icon("move-3d", "Show guides")}</button><button class="subtle" id="fit-canvas">Fit</button><span class="muted">72%</span></div></div><div class="canvas-stage"><div class="ruler-top"><span>0</span><span>480</span><span>960</span><span>1440</span><span>1920</span></div><div class="canvas-art">${scoreboard()}<div class="canvas-selection"></div></div><div class="canvas-hint">Alt-drag to duplicate · Shift to constrain · Space to pan</div></div>`;
}

function propertyInspector() {
  return `<aside class="inspector"><div class="panel-title">${state.selectedLayer}<span class="muted">Text</span></div><div class="section"><div class="section-title">Transform ${icon("chevron-down")}</div><div class="two-fields"><label><span class="field-label">Position X</span><input class="field" value="300" aria-label="Position X"></label><label><span class="field-label">Position Y</span><input class="field" value="842" aria-label="Position Y"></label><label><span class="field-label">Width</span><input class="field" value="430" aria-label="Width"></label><label><span class="field-label">Height</span><input class="field" value="92" aria-label="Height"></label></div><div class="two-fields"><label><span class="field-label">Rotation</span><input class="field" value="0°" aria-label="Rotation"></label><label><span class="field-label">Opacity</span><input class="field" value="100%" aria-label="Opacity"></label></div></div><div class="section"><div class="section-title">Typography ${icon("chevron-down")}</div><label><span class="field-label">Font family</span><select aria-label="Font family"><option>Arial</option><option>Helvetica Neue</option><option>Avenir Next</option><option>Georgia</option></select></label><div class="two-fields" style="margin-top:10px"><label><span class="field-label">Style</span><select aria-label="Font style"><option>Semibold</option><option>Regular</option><option>Bold</option></select></label><label><span class="field-label">Size</span><input class="field" aria-label="Font size" value="64"></label></div><div class="align-buttons"><button>Left</button><button class="active">Center</button><button>Right</button></div><div class="inline-field" style="margin-top:12px"><span class="color-swatch"></span><input class="field" value="#F6F9FF" aria-label="Text color"></div></div><div class="section"><div class="section-title">Live data ${icon("link")}</div><div class="binding">Eagles → Team name<span class="muted">Current value: EAGLES</span></div><button class="subtle" style="width:100%;margin-top:9px">Change binding</button></div><div class="section"><div class="section-title">Quick animation</div><select aria-label="Animation preset"><option>Fade in</option><option>Slide up</option><option>Pop</option><option>None</option></select><div class="note" style="margin-top:8px">Use Animate to edit individual keyframes.</div></div></aside>`;
}

function timeline() {
  const names = [
    "Home name",
    "  Opacity",
    "Home score",
    "Game clock",
    "Away name",
  ];
  return `<section class="timeline" aria-label="Animation timeline"><div class="timeline-transport"><div><button class="icon-button" id="restart-demo" title="Restart preview">${icon("skip-back", "Restart preview")}</button><button class="icon-button" id="play-demo" title="${state.playing ? "Pause" : "Play"} preview">${icon(state.playing ? "pause" : "play", state.playing ? "Pause" : "Play")}</button><span style="margin-left:10px;font-variant-numeric:tabular-nums">00:00:01:00</span></div><div><span class="muted">Entrance · 4 seconds</span><button class="subtle" id="keyframe-demo">${icon("diamond")}Add keyframe</button></div></div><div class="timeline-body"><div class="tracks"><div class="track-heading">Layer / property</div>${names.map((name, index) => `<div class="track ${index === 0 ? "active" : ""}">${icon(index === 1 ? "diamond" : "chevron-right")}<span>${name}</span>${index !== 1 ? icon("eye") : ""}</div>`).join("")}</div><div class="time-lanes"><div class="time-ruler"><span>0s</span><span>1s</span><span>2s</span><span>3s</span><span>4s</span></div>${names.map((_, index) => `<div class="lane">${index === 1 ? '<span class="key first"></span><span class="key second"></span>' : '<span class="clip-span"></span>'}</div>`).join("")}<div class="playhead"></div></div></div></section>`;
}

function eventInspector() {
  return `<aside class="inspector event-inspector"><div class="panel-title">Events & cues<button class="icon-button" id="event-demo" title="New event rule">${icon("plus", "New event rule")}</button></div><div class="section"><div class="rule"><strong>Score celebration</strong><div class="rule-caption">WHEN</div><select aria-label="Event"><option>Points are added</option><option>Points are removed</option><option>Clock starts</option><option>Clock resets</option><option>Clock finishes</option></select><div class="rule-caption">FOR</div><select aria-label="Event source"><option>Any team</option><option>Eagles</option><option>Lions</option></select><div class="rule-caption">PLAY CUE</div><select aria-label="Event cue"><option>Score pulse</option><option>Winner sting</option><option>Round intro</option></select><div class="rule-caption">ON OUTPUT</div><select aria-label="Event output"><option>Broadcast overlay</option><option>Venue screen</option></select><button class="subtle" id="preview-cue" style="margin-top:12px;width:100%">${icon("play")}Preview cue</button></div><div class="note">An event uses the same cue as the Live workspace or a remote button.</div></div><div class="section"><div class="section-title">Selected keyframe</div><div class="two-fields"><label><span class="field-label">Time</span><input class="field" value="1.00 s" aria-label="Keyframe time"></label><label><span class="field-label">Opacity</span><input class="field" value="100%" aria-label="Keyframe opacity"></label></div><span class="field-label">Easing</span><select aria-label="Keyframe easing"><option>Ease out</option><option>Linear</option><option>Ease in/out</option></select></div></aside>`;
}

function liveWorkspace() {
  return `<section class="live-layout"><aside class="sidebar"><div class="panel-title">Show library<button class="icon-button" title="New cue">${icon("plus", "New cue")}</button></div><div class="section"><div class="section-title">Designs</div><button class="layer-row active">${icon("monitor")}Broadcast lower third</button><button class="layer-row">${icon("monitor")}Five-team competition</button><button class="layer-row">${icon("monitor")}Winner card</button></div><div class="section"><div class="section-title">Cues</div><button class="layer-row">${icon("diamond")}Score pulse</button><button class="layer-row">${icon("video")}Winner sting</button><button class="layer-row">${icon("diamond")}Round intro</button></div><div class="section"><div class="section-title">Show setup</div><div class="note">5 teams · 2 clocks · 2 outputs</div><button class="subtle" id="configure-game" style="margin-top:12px;width:100%">${icon("settings")}Configure game</button></div></aside><div class="center"><div class="live-monitor"><div class="monitor-label"><span>Broadcast overlay</span><button class="subtle" id="take-demo">${icon("send")}Take preview</button></div><div class="monitor-pair"><div><div class="monitor-label"><span class="badge">PROGRAM · CONCEPT</span><span class="muted">Broadcast</span></div><div class="monitor-art">${scoreboard()}</div></div><div><div class="monitor-label"><span class="badge preview">PREVIEW</span><span class="muted">Ready to take</span></div><div class="monitor-art pending"><div class="pending-card">COMMUNITY CUP<strong>EAGLES</strong><small>WINNER CARD</small></div></div></div></div></div><div class="panel-title">Team scores<span class="muted">Custom scoring buttons</span></div><div class="score-grid">${state.teams.map((team, index) => `<article class="team-card" style="--team:${team.color}"><header><span>${team.name}</span>${icon("ellipsis")}</header><div class="team-number">${team.score}</div><div class="point-actions"><button data-score-team="${index}" data-delta="-1">−1</button><button data-score-team="${index}" data-delta="1">+1</button><button data-score-team="${index}" data-delta="3">+3</button></div></article>`).join("")}</div></div><aside class="inspector"><div class="panel-title">Clocks</div><div class="clock-card"><div class="section-title">Game clock<span class="badge preview">PAUSED</span></div><button class="clock-value" id="edit-time">08:42</button><div class="clock-buttons"><button class="primary" id="clock-start">${icon("play")}Start</button><button id="clock-reset">Reset</button><button>−10s</button><button>+10s</button></div></div><div class="clock-card"><div class="section-title">Round timer<span class="muted">Count down</span></div><span class="clock-value">02:00</span><div class="clock-buttons"><button>${icon("play")}Start</button><button>Reset</button><button>+30s</button></div></div><div class="panel-title">Trigger cues<span class="muted">3 cues</span></div><div class="cue-grid"><button class="cue-tile" data-cue="Score pulse">${icon("diamond")}Score pulse<span>Animation</span></button><button class="cue-tile" data-cue="Winner sting">${icon("video")}Winner sting<span>Video · 0:06</span></button><button class="cue-tile" data-cue="Round intro">${icon("diamond")}Round intro<span>Animation</span></button></div><div class="section"><div class="section-title">Outputs</div><div class="note">Broadcast · Syphon<br>Venue · NDI<br><br>Monitor values shown here are simulated.</div></div></aside></section>`;
}

function remoteWorkspace() {
  return `<section class="remote-layout"><aside class="sidebar"><div class="panel-title">Control palette</div><div class="section"><div class="section-title">Game actions</div>${[
    ["plus", "Add points"],
    ["undo-2", "Remove points"],
    ["clock", "Clock action"],
    ["video", "Play cue"],
    ["monitor", "Show / hide output"],
  ]
    .map(
      ([name, label]) =>
        `<button class="layer-row">${icon(name)}${label}</button>`,
    )
    .join(
      "",
    )}<button class="subtle" style="margin-top:12px;width:100%">${icon("plus")}Add button</button></div><div class="section"><div class="section-title">Layout</div><button class="subtle" id="generate-remote">Build team controls</button><p class="note">Each button keeps its own action, label, size and confirmation behavior.</p></div></aside><div class="device-stage"><div class="mini-tabs"><button class="active">Phone</button><button>Tablet</button></div><div class="phone"><div class="phone-head">OVERLAY STUDIO <span style="float:right">Paired · concept</span></div><strong>Community Cup</strong><div class="phone-score" style="margin-top:15px"><div>Eagles<strong>${state.teams[0].score}</strong></div><div>Lions<strong>${state.teams[1].score}</strong></div></div><div class="phone-clock">08:42</div><div class="phone-buttons"><button data-score-team="0" data-delta="1">Eagles +1</button><button data-score-team="1" data-delta="1">Lions +1</button><button data-score-team="0" data-delta="-1">Eagles −1</button><button data-score-team="1" data-delta="-1">Lions −1</button><button class="wide">Start / pause game clock</button><button class="wide" data-cue="Winner sting">Play winner sting</button></div></div><span class="note">Touch controls · minimum 44 px targets · desktop layout stays separate</span></div><aside class="inspector"><div class="panel-title">Button properties</div><div class="section"><span class="field-label">Label</span><input class="field" aria-label="Button label" value="Eagles +1"><span class="field-label" style="margin-top:12px">Action</span><select aria-label="Button action"><option>Add / remove points</option><option>Clock action</option><option>Play cue</option></select><span class="field-label" style="margin-top:12px">Team</span><select aria-label="Button team"><option>Eagles</option><option>Lions</option><option>Falcons</option><option>Knights</option><option>Royals</option></select><span class="field-label" style="margin-top:12px">Point change</span><input class="field" value="+1" aria-label="Point change"><div class="align-buttons"><button>${icon("copy")}Copy</button><button>${icon("trash-2")}Remove</button></div></div><div class="section"><div class="section-title">Connect a device</div><div class="connection-card"><strong>Pair by QR code</strong><p class="note">The app shows a current-session QR and local address here. This design concept does not create a pairing code.</p><button class="subtle" id="pairing-demo">${icon("smartphone")}Connection details</button></div></div></aside></section>`;
}

function render() {
  document
    .querySelectorAll("[data-workspace]")
    .forEach((button) =>
      button.setAttribute(
        "aria-selected",
        String(button.dataset.workspace === state.workspace),
      ),
    );
  workspace.setAttribute("aria-label", `${state.workspace} workspace`);
  if (state.workspace === "live") workspace.innerHTML = liveWorkspace();
  else if (state.workspace === "remote")
    workspace.innerHTML = remoteWorkspace();
  else
    workspace.innerHTML =
      toolRail() +
      editorSidebar() +
      `<section class="center">${canvas()}${timeline()}</section>` +
      (state.workspace === "animate" ? eventInspector() : propertyInspector());
}

function feedback(message) {
  document.getElementById("feedback").textContent =
    `${message} · Design concept, simulated data`;
}

function explain(title, text) {
  document.getElementById("dialog-title").textContent = title;
  document.getElementById("dialog-text").textContent = text;
  document.getElementById("proposal-dialog").showModal();
}

document.addEventListener("click", (event) => {
  const button = event.target.closest("button");
  if (!button) return;
  if (button.dataset.workspace) {
    state.workspace = button.dataset.workspace;
    render();
    return;
  }
  if (button.dataset.layer) {
    state.selectedLayer = button.dataset.layer;
    render();
    return;
  }
  if (button.dataset.tool) {
    document
      .querySelectorAll("[data-tool]")
      .forEach((tool) => tool.classList.toggle("active", tool === button));
    feedback(`${button.dataset.tool} selected`);
    return;
  }
  if (button.dataset.scoreTeam !== undefined) {
    const index = Number(button.dataset.scoreTeam);
    state.scoreHistory.push(state.teams.map((team) => team.score));
    state.teams[index].score += Number(button.dataset.delta);
    render();
    feedback(`${state.teams[index].name} score updated`);
    return;
  }
  if (button.dataset.cue) {
    feedback(`${button.dataset.cue} selected for preview`);
    return;
  }
  const actions = {
    "play-demo": () => {
      state.playing = !state.playing;
      render();
      feedback(
        state.playing
          ? "Animation preview playing"
          : "Animation preview paused",
      );
    },
    "restart-demo": () => feedback("Animation preview returned to the start"),
    "keyframe-demo": () =>
      explain(
        "Add keyframe",
        "The implemented timeline will add a diamond on the selected property at the playhead. Transform values and easing are edited in the contextual inspector.",
      ),
    "undo-demo": () => {
      const scores = state.scoreHistory.pop();
      if (scores) {
        state.teams.forEach((team, index) => {
          team.score = scores[index];
        });
        render();
        feedback("Simulated score change undone");
      }
    },
    "outputs-button": () =>
      explain(
        "Output manager",
        "A focused panel will show each NDI/Syphon stream, its active design, frame status, visibility, and output controls. GPU output remains managed by the Rust engine.",
      ),
    "save-concept": () =>
      explain(
        "Design review only",
        "This prototype demonstrates the proposed interface. It does not modify your project files or connect to the running app.",
      ),
    "configure-game": () =>
      explain(
        "Configure game",
        "Team names, colors, point increments, multiple clocks and rules belong in a separate setup panel, leaving the Live workspace focused on operating the show.",
      ),
    "edit-time": () =>
      explain(
        "Set game clock",
        "A time editor will accept MM:SS directly, with separate minutes and seconds fields, reset duration and count direction.",
      ),
    "clock-reset": () =>
      explain(
        "Reset game clock?",
        "The app will identify the selected clock and reset duration before confirming a reset. Other running clocks remain independent.",
      ),
    "clock-start": () => feedback("Game clock start selected"),
    "preview-cue": () => feedback("Score celebration cue preview selected"),
    "take-demo": () => feedback("Preview design selected for Take"),
    "generate-remote": () =>
      feedback("Remote controls generated for the configured teams"),
    "pairing-demo": () =>
      explain(
        "Local pairing",
        "The real app will display a session QR code and LAN address, connection state and revoke controls. No network access is enabled by this prototype.",
      ),
    "event-demo": () =>
      explain(
        "New event rule",
        "Choose when an event occurs, which team or clock it applies to, the cue to play, and its output. A readable summary and a Run now action remain visible.",
      ),
    "dialog-close": () => document.getElementById("proposal-dialog").close(),
    "fit-canvas": () => feedback("Canvas fitted to the available workspace"),
  };
  if (actions[button.id]) actions[button.id]();
});

render();
