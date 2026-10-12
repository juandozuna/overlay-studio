#!/usr/bin/env python3
"""Build a Slint-focused Overlay Studio PDF manual from captured screenshots."""
from __future__ import annotations

from pathlib import Path

from PIL import Image
from reportlab.lib.pagesizes import letter
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import inch
from reportlab.platypus import (
    Image as RLImage,
    KeepTogether,
    PageBreak,
    Paragraph,
    SimpleDocTemplate,
    Spacer,
)

ROOT = Path(__file__).resolve().parent
SHOTS = ROOT / "screenshots"
OUT = ROOT / "output" / "Overlay Studio Slint Manual.pdf"


PAGES = [
    (
        "Design workspace",
        "11-slint-designer.png",
        "Open a project and stay in Design to build scoreboard canvases. Use Blank or Copy design for new layouts, then place text, shapes, images, and video from the Create palette.",
        [
            "Choose a design in the left list, or create a blank canvas.",
            "Select a creation tool, then click or drag on the canvas to place layers.",
            "Use the inspector for precise geometry, colors, fonts, and live-value bindings.",
            "Save the project with Save or Save as. Export design writes a reusable .scoredesign file.",
        ],
    ),
    (
        "Layers and appearance",
        "18-slint-color-picker.png",
        "Click a color swatch in the inspector to open the visual picker. Draft fills support opacity; team and remote button colors stay opaque.",
        [
            "Select a layer, then edit Fill/Stroke or Text color.",
            "Drag the hue strip and color area, or choose a preset chip.",
            "Confirm to apply, or Cancel to keep the previous color.",
            "Hidden and Locked checkboxes protect layers during live shows.",
        ],
    ),
    (
        "Game setup: teams and clocks",
        "15-slint-clocks.png",
        "In Live, open Game setup to add teams, scoring steps, and independent clocks. Create scoreboard builds an adaptive multi-team layout on the canvas.",
        [
            "Press Game setup, then Add team or Add clock.",
            "Edit names, team colors, and scoring steps such as -1, +1, +5.",
            "Set each clock’s reset time and count up/down mode.",
            "Press Finish setup when operators should only adjust live scores and clocks.",
        ],
    ),
    (
        "Live desk and outputs",
        "14-slint-live.png",
        "Live Desk combines scores, clocks, and output senders. Each output has a name, Syphon or NDI protocol, frame rate, and selected design.",
        [
            "Adjust scores with the step buttons under each team.",
            "Start or pause clocks from the clock cards.",
            "Configure an output, press Start output, then Take design when the canvas is ready.",
            "Hide sends transparent frames; Stop removes the sender.",
        ],
    ),
    (
        "Custom live fields",
        "24-slint-custom-fields.png",
        "Game setup can add custom live text fields. Bind design text layers to those fields so the same show data drives multiple canvases.",
        [
            "In Game setup, enter a field name and add it.",
            "In Design, select a text layer and choose the field in the live-value binding list.",
            "Update the value from Live; bound layers refresh automatically.",
            "Removing a field repairs bindings, cues, and remote buttons in one Undoable action.",
        ],
    ),
    (
        "Remote controller and QR pairing",
        "16-slint-remote.png",
        "Remote layout designs phone and iPad controls. Start remote control, scan the QR code on the same local network, then operate scores and clocks from the browser.",
        [
            "Choose a local network address and press Start remote control.",
            "Scan the QR code with the phone or iPad camera.",
            "Drag buttons in the grid; use Build team controls for a full scoring set.",
            "Press Stop remote / revoke QR when the session ends. Always scan a fresh QR from your own app.",
        ],
    ),
    (
        "Animation timeline",
        "19-slint-timeline.png",
        "Animate provides a canvas preview, scrubber, and layer timeline. Keyframes store X/Y, size, rotation, opacity, and easing.",
        [
            "Select a layer track and add a pose at the playhead.",
            "Edit transform fields or drag frames along the timeline.",
            "Enable Loop for repeating motion; presets remain available for simple entrances.",
            "Play, Pause, Restart, and Exit preview control playback without changing the live program.",
        ],
    ),
    (
        "Event rules",
        "20-slint-events.png",
        "Events automate show responses. Rules can react to score changes or clock milestones and then replay animation, take a design, or set a live value.",
        [
            "In Animate, switch to Events and add a rule.",
            "Choose the trigger, target field or clock, and the resulting action.",
            "Enable the rule for automatic firing, or use Run cue now for a manual take.",
            "Undo/Redo pauses clocks so restored state does not replay elapsed time.",
        ],
    ),
    (
        "Image and video import",
        "22-slint-media.png",
        "Design imports PNG/JPEG images and short video clips. Video conversion uses a local FFmpeg install; playback does not require FFmpeg afterward.",
        [
            "Press Image / logo… or Video clip… in the Create palette.",
            "Wait for preparation to finish; Cancel import stops an active conversion.",
            "Imported assets travel inside .overlay and .scoredesign files.",
            "Motion layers are limited to about 15 seconds with a preview ceiling during development.",
        ],
    ),
    (
        "Recovery after a crash",
        "20-slint-recovery.jpg",
        "Autosave recovery lives in Application Support. On launch, Recover opens the draft as unsaved, Discard removes it, and Later keeps it for the next launch.",
        [
            "Relaunch Overlay Studio if it closed unexpectedly.",
            "Choose Recover to inspect the draft, then Save when it looks correct.",
            "Outputs, clocks, and remote control stay stopped after recovery.",
            "Check crash.log in the Application Support folder if a Rust error was recorded.",
        ],
    ),
]


def styles():
    base = getSampleStyleSheet()
    return {
        "title": ParagraphStyle(
            "TitleBig",
            parent=base["Title"],
            fontName="Helvetica-Bold",
            fontSize=28,
            spaceAfter=10,
            textColor="#17222c",
        ),
        "subtitle": ParagraphStyle(
            "Subtitle",
            parent=base["Normal"],
            fontName="Helvetica",
            fontSize=12,
            textColor="#3c424c",
            spaceAfter=18,
        ),
        "h1": ParagraphStyle(
            "H1",
            parent=base["Heading1"],
            fontName="Helvetica-Bold",
            fontSize=18,
            textColor="#17222c",
            spaceBefore=0,
            spaceAfter=10,
        ),
        "body": ParagraphStyle(
            "Body",
            parent=base["Normal"],
            fontName="Helvetica",
            fontSize=10.5,
            leading=14,
            textColor="#22252b",
            spaceAfter=8,
        ),
        "step": ParagraphStyle(
            "Step",
            parent=base["Normal"],
            fontName="Helvetica",
            fontSize=10.5,
            leading=14,
            leftIndent=14,
            textColor="#22252b",
            spaceAfter=4,
        ),
        "caption": ParagraphStyle(
            "Caption",
            parent=base["Normal"],
            fontName="Helvetica-Oblique",
            fontSize=9,
            textColor="#5a6370",
            spaceBefore=4,
            spaceAfter=12,
        ),
    }


def fitted_image(path: Path, max_width: float, max_height: float) -> RLImage:
    with Image.open(path) as im:
        w, h = im.size
    scale = min(max_width / w, max_height / h, 1.0)
    return RLImage(str(path), width=w * scale, height=h * scale)


def build() -> Path:
    OUT.parent.mkdir(parents=True, exist_ok=True)
    s = styles()
    doc = SimpleDocTemplate(
        str(OUT),
        pagesize=letter,
        leftMargin=0.7 * inch,
        rightMargin=0.7 * inch,
        topMargin=0.65 * inch,
        bottomMargin=0.65 * inch,
        title="Overlay Studio Slint Manual",
        author="Overlay Studio",
    )
    story = [
        Paragraph("Overlay Studio", s["title"]),
        Paragraph("Slint desktop instruction manual", s["subtitle"]),
        Paragraph(
            "This manual documents the current Slint desktop build: Design, Live, Remote, Animate, "
            "media import, custom fields, and recovery. Screenshots were captured from the native app. "
            "QR codes shown for pairing are examples only—always scan the fresh code from your running session.",
            s["body"],
        ),
        Paragraph(
            "Run the app with <font face='Courier'>cargo run --release --manifest-path desktop/slint/Cargo.toml</font>. "
            "Validate with <font face='Courier'>cargo test</font> and the Slint package tests.",
            s["body"],
        ),
        Spacer(1, 8),
        Paragraph("Contents", s["h1"]),
    ]
    for index, (title, *_rest) in enumerate(PAGES, 1):
        story.append(Paragraph(f"{index}. {title}", s["step"]))

    for title, shot, intro, steps in PAGES:
        path = SHOTS / shot
        if not path.is_file():
            raise SystemExit(f"Missing screenshot: {path}")
        story.append(PageBreak())
        block = [
            Paragraph(title, s["h1"]),
            Paragraph(intro, s["body"]),
            fitted_image(path, 7.0 * inch, 5.1 * inch),
            Paragraph(path.name.replace("-", " ").replace(".png", "").replace(".jpg", ""), s["caption"]),
        ]
        for i, step in enumerate(steps, 1):
            block.append(Paragraph(f"{i}. {step}", s["step"]))
        story.append(KeepTogether(block))

    story.append(PageBreak())
    story.append(Paragraph("Shortcuts and limits", s["h1"]))
    story.append(
        Paragraph(
            "Design tools: V select, R rectangle, O ellipse, T text, L line, Escape cancels a creation tool. "
            "Command-S saves, Command-Z / Shift-Command-Z undo and redo, Command-D duplicates. "
            "Command-1…4 switch Design, Live, Remote, and Animate.",
            s["body"],
        )
    )
    story.append(
        Paragraph(
            "Current limits include 32 teams, 16 clocks, 8 outputs, 64 designs, 100 remote buttons, and 1000 layers per canvas. "
            "Rehearse on the actual receiver hardware before a live event.",
            s["body"],
        )
    )

    def footer(canvas, _doc):
        canvas.saveState()
        canvas.setFont("Helvetica", 8)
        canvas.setFillColorRGB(0.35, 0.38, 0.42)
        canvas.drawString(0.7 * inch, 0.4 * inch, "Overlay Studio  ·  Slint manual")
        canvas.drawRightString(letter[0] - 0.7 * inch, 0.4 * inch, str(canvas.getPageNumber()))
        canvas.restoreState()

    doc.build(story, onFirstPage=footer, onLaterPages=footer)
    return OUT


if __name__ == "__main__":
    path = build()
    print(path, path.stat().st_size)
