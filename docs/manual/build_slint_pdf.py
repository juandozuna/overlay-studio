#!/usr/bin/env python3
"""Build the Overlay Studio PDF manual (usage + ProPresenter NDI) from screenshots."""
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
OUT = ROOT / "output" / "Overlay Studio Instruction Manual.pdf"

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
        "h2": ParagraphStyle(
            "H2",
            parent=base["Heading2"],
            fontName="Helvetica-Bold",
            fontSize=13,
            textColor="#24505f",
            spaceBefore=10,
            spaceAfter=6,
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
        "note": ParagraphStyle(
            "Note",
            parent=base["Normal"],
            fontName="Helvetica-Oblique",
            fontSize=9.5,
            leading=13,
            textColor="#5a6370",
            spaceAfter=8,
        ),
    }


def fitted_image(path: Path, max_width: float, max_height: float) -> RLImage:
    with Image.open(path) as im:
        w, h = im.size
    scale = min(max_width / w, max_height / h, 1.0)
    return RLImage(str(path), width=w * scale, height=h * scale)


def add_steps(story, s, steps):
    for i, step in enumerate(steps, 1):
        story.append(Paragraph(f"{i}. {step}", s["step"]))


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
        title="Overlay Studio Instruction Manual",
        author="Overlay Studio",
    )
    story = [
        Paragraph("Overlay Studio", s["title"]),
        Paragraph("Instruction manual · macOS desktop", s["subtitle"]),
        Paragraph(
            "Design transparent scoreboards, control a live show from your Mac, phone, or iPad, "
            "and send graphics over NDI or Syphon into tools such as ProPresenter.",
            s["body"],
        ),
        Paragraph(
            "This PDF ships inside the Overlay Studio DMG next to the app. Screenshots show the "
            "current desktop interface. QR codes pictured for remote pairing are examples only—"
            "always scan the fresh code from your running session.",
            s["body"],
        ),
        Spacer(1, 6),
        Paragraph("Contents", s["h1"]),
        Paragraph("1. Install from the DMG and run your first show", s["step"]),
        Paragraph("2–11. Workspace guides (Design, Live, Remote, Animate, media, recovery)", s["step"]),
        Paragraph("12. Send Overlay Studio into ProPresenter with NDI", s["step"]),
        Paragraph("13. Shortcuts, limits, and support links", s["step"]),
    ]

    # Quick start
    story.append(PageBreak())
    story.append(Paragraph("Install from the DMG and run your first show", s["h1"]))
    story.append(
        Paragraph(
            "The installer disk image contains Overlay Studio.app, this instruction manual, "
            "and a shortcut to your Applications folder.",
            s["body"],
        )
    )
    story.append(Paragraph("Install", s["h2"]))
    add_steps(
        story,
        s,
        [
            "Open the Overlay Studio .dmg you received.",
            "Drag Overlay Studio into Applications (use the Applications alias in the window).",
            "Open Overlay Studio from Applications. On first launch macOS may ask to confirm an "
            "unsigned or ad-hoc signed app: right-click the app → Open → Open.",
            "Keep this PDF nearby—copy it out of the DMG or open it from the disk image while learning.",
        ],
    )
    story.append(Paragraph("First show (about five minutes)", s["h2"]))
    add_steps(
        story,
        s,
        [
            "In Design, press Blank or Create scoreboard (from Live → Game setup) to start a canvas.",
            "Bind team names and scores to live fields, or place text/shapes and set their bindings in the inspector.",
            "Switch to Live. Add teams and clocks under Game setup, then Finish setup.",
            "Add or select an output. Set Protocol to NDI, choose a frame rate, pick the design, then Start output.",
            "Press Take design so the running sender shows the current canvas (transparent checkerboard is never transmitted).",
            "Change a score or clock and confirm the NDI receiver updates. Save the project as a .overlay file.",
        ],
    )
    story.append(
        Paragraph(
            "Tip: Hide on an output sends fully transparent frames; Stop removes the NDI/Syphon sender.",
            s["note"],
        )
    )

    for title, shot, intro, steps in PAGES:
        path = SHOTS / shot
        if not path.is_file():
            raise SystemExit(f"Missing screenshot: {path}")
        story.append(PageBreak())
        block = [
            Paragraph(title, s["h1"]),
            Paragraph(intro, s["body"]),
            fitted_image(path, 7.0 * inch, 5.1 * inch),
            Paragraph(
                path.name.replace("-", " ").replace(".png", "").replace(".jpg", ""),
                s["caption"],
            ),
        ]
        for i, step in enumerate(steps, 1):
            block.append(Paragraph(f"{i}. {step}", s["step"]))
        story.append(KeepTogether(block))

    # ProPresenter NDI — based on Renewed Vision documentation
    story.append(PageBreak())
    story.append(Paragraph("Send Overlay Studio into ProPresenter with NDI", s["h1"]))
    story.append(
        Paragraph(
            "Overlay Studio can publish a transparent scoreboard as an NDI source on your local network. "
            "ProPresenter can receive that source as a Video Input and place it under lyrics or slides. "
            "The steps below follow Renewed Vision’s current ProPresenter documentation for video inputs "
            "and NDI discovery.",
            s["body"],
        )
    )
    story.append(
        Paragraph(
            "Sources: Renewed Vision — Setting Up A Video Input in ProPresenter; NDI Input Discovery; "
            "NDI Troubleshooting; ProPresenter Output Layers. "
            "https://support.renewedvision.com/",
            s["note"],
        )
    )

    story.append(Paragraph("A. Start the NDI sender in Overlay Studio", s["h2"]))
    add_steps(
        story,
        s,
        [
            "Put both Macs (or the same Mac) on the same local network subnet when possible (for example 192.168.1.x).",
            "In Overlay Studio open Live, create or select an output, set Protocol to NDI, choose FPS, and select the design.",
            "Press Start output, then Take design after the canvas looks correct.",
            "Note the output name—you will pick that NDI source inside ProPresenter. Leave the sender running.",
        ],
    )

    story.append(Paragraph("B. Add Overlay Studio as a ProPresenter video input", s["h2"]))
    add_steps(
        story,
        s,
        [
            "In ProPresenter go to ProPresenter → Settings → Inputs (Video Inputs tab).",
            "Click + above Add Video Input. A new input appears.",
            "Name it clearly (for example “Overlay Studio Scoreboard”).",
            "Open the Device dropdown and choose the Overlay Studio NDI source. A live preview should appear.",
            "If the device supports auto-detection, resolution and frame rate fill in automatically; otherwise choose a mode.",
            "For audio, choose No Audio Source unless you intentionally want embedded audio from another device.",
            "Optional: capture a thumbnail with the camera icon so the Media Bin is easy to read during the show.",
        ],
    )

    story.append(Paragraph("C. If the NDI source does not appear", s["h2"]))
    add_steps(
        story,
        s,
        [
            "Confirm Overlay Studio’s NDI output is still Started and that both machines share a gigabit-capable network path.",
            "In Settings → Inputs open NDI Input Discovery.",
            "Enable Show Sources running on the local machine if Overlay Studio and ProPresenter run on the same Mac.",
            "If the machines are on different subnets, enter the Overlay Studio computer’s IP under Additional Search IPs "
            "(IPv4 or IPv6).",
            "If you use NDI groups via NDI Access Manager, enter the group name(s) under Receive Groups (comma-separated).",
            "Verify subnet communication as described in Renewed Vision’s NDI Troubleshooting article, then re-open the Device list.",
        ],
    )

    story.append(Paragraph("D. Put the input on your ProPresenter screens", s["h2"]))
    add_steps(
        story,
        s,
        [
            "After setup, the input appears in the Video Input Playlist inside the Media Bin.",
            "Trigger the input from that playlist, add a Video Input Action on a slide or macro, or place a Video Input "
            "element in the Slide Editor.",
            "Remember layer order: the Video Input layer sits near the bottom of the audience screen stack, so slides, "
            "media, and presentation content render above the live scoreboard unless you change Looks.",
            "Use Looks (Screens) if the house projectors should hide the video-input layer while a stream look keeps it visible—or the reverse.",
            "Rehearse Take design in Overlay Studio whenever you edit the canvas so ProPresenter receives the updated graphic.",
        ],
    )
    story.append(
        Paragraph(
            "Alpha: Overlay Studio transmits transparent frames for empty areas. Keep ProPresenter’s audience "
            "background and Looks configured so the scoreboard composites cleanly over camera or media layers.",
            s["note"],
        )
    )

    story.append(PageBreak())
    story.append(Paragraph("Shortcuts, limits, and support links", s["h1"]))
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
    story.append(Paragraph("Renewed Vision references used for the ProPresenter chapter", s["h2"]))
    for url in [
        "https://support.renewedvision.com/hc/en-us/articles/360053482973-Setting-Up-A-Video-Input-in-ProPresenter",
        "https://support.renewedvision.com/hc/en-us/articles/32161719466643-NDI-Input-Discovery",
        "https://support.renewedvision.com/hc/en-us/articles/4403014667283-NDI-Troubleshooting",
        "https://support.renewedvision.com/hc/en-us/articles/13634000690323-ProPresenter-Output-Layers",
    ]:
        story.append(Paragraph(url, s["note"]))

    def footer(canvas, _doc):
        canvas.saveState()
        canvas.setFont("Helvetica", 8)
        canvas.setFillColorRGB(0.35, 0.38, 0.42)
        canvas.drawString(0.7 * inch, 0.4 * inch, "Overlay Studio  ·  Instruction manual")
        canvas.drawRightString(letter[0] - 0.7 * inch, 0.4 * inch, str(canvas.getPageNumber()))
        canvas.restoreState()

    doc.build(story, onFirstPage=footer, onLaterPages=footer)
    # Keep a stable alias used by older docs/scripts
    alias = OUT.parent / "Overlay Studio Slint Manual.pdf"
    if alias.exists() or alias.is_symlink():
        alias.unlink()
    alias.write_bytes(OUT.read_bytes())
    return OUT


if __name__ == "__main__":
    path = build()
    print(path, path.stat().st_size)
