#!/usr/bin/env python3
"""Render the ProPresenter step illustrations (HTML -> PNG with headless Chrome).

These are drawn illustrations of the ProPresenter 20 steps described in Renewed
Vision's documentation, not screenshots of ProPresenter itself.
"""
import subprocess, tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE.parent / "img"
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

CSS = """
*{box-sizing:border-box}body{margin:0;width:1200px;height:700px;background:#0b0d12;font:20px -apple-system,Helvetica,Arial,sans-serif;color:#e8ebf2;position:relative;overflow:hidden}
.win{position:absolute;left:40px;top:40px;width:1120px;height:600px;background:#1b1e26;border:1px solid #2c3242;border-radius:14px;box-shadow:0 20px 60px #0008;overflow:hidden}
.bar{height:44px;background:#252a35;display:flex;align-items:center;padding:0 16px;gap:8px;color:#9aa3b5;font-size:16px}
.dot{width:12px;height:12px;border-radius:50%;background:#ff5f57}.dot:nth-child(2){background:#febc2e}.dot:nth-child(3){background:#28c840}
.menu{height:36px;background:#1f232d;display:flex;align-items:center;padding:0 18px;gap:26px;font-size:16px;color:#b9c0cf}
.tabs{display:flex;gap:2px;padding:14px 24px 0;border-bottom:1px solid #2c3242}
.tab{padding:10px 20px;font-size:17px;color:#9aa3b5}.tab.on{color:#fff;border-bottom:3px solid #4cc2ff}
.body{padding:24px}
.btn{display:inline-block;padding:10px 18px;border:1px solid #3a4152;border-radius:8px;background:#272c38}
.lbl{color:#9aa3b5;font-size:15px;margin:14px 0 6px}
.sel{border:1px solid #3a4152;border-radius:8px;padding:10px 14px;background:#12151c;width:420px}
.hl{outline:4px solid #ffb020;outline-offset:3px;border-radius:8px}
.badge{position:absolute;width:44px;height:44px;border-radius:50%;background:#ffb020;color:#111;font-weight:800;font-size:24px;display:grid;place-items:center;box-shadow:0 4px 14px #0009}
.cap{position:absolute;left:0;right:0;bottom:6px;text-align:center;font-size:13px;color:#7d869a}
.row{display:flex;gap:24px}.prev{width:380px;height:214px;border:1px solid #3a4152;border-radius:10px;background:repeating-conic-gradient(#2a2f3a 0 25%,#222630 0 50%) 0 0/24px 24px;position:relative}
.chip{position:absolute;top:44px;width:62px;height:110px;border-radius:8px;font-weight:700;display:grid;place-items:center;font-size:36px}
.list{border:1px solid #3a4152;border-radius:8px;width:300px}.list div{padding:12px 16px;border-bottom:1px solid #2c3242}.list div:last-child{border:0}
"""

def chips():
    colors=["#2065c0","#c0431f","#1f9670","#7b3bb8","#b58a14"]
    return "".join(f'<div class="chip" style="left:{14+i*70}px;background:{c}">0</div>' for i,c in enumerate(colors))

PAGES = {
"pp-1-inputs": f"""
<div class="win"><div class="bar"><span class="dot"></span><span class="dot"></span><span class="dot"></span>&nbsp; ProPresenter</div>
<div class="menu"><b style="color:#fff">ProPresenter</b><span>File</span><span>Edit</span><span>View</span><span>Screens</span><span>Window</span></div>
<div class="body"><div class="hl" style="display:inline-block;padding:12px 18px;background:#272c38">ProPresenter &rsaquo; Settings&hellip; &rsaquo; <b>Inputs</b></div>
<div class="lbl" style="margin-top:34px">Settings window &mdash; Inputs</div>
<div class="tabs" style="padding-left:0"><div class="tab on">Video Inputs</div><div class="tab">Audio Inputs</div></div>
<div style="margin-top:26px" class="row"><div class="list"><div style="background:#252a35">&nbsp;</div><div style="color:#7d869a">No inputs yet</div></div>
<div><span class="btn hl">+ &nbsp;Add Video Input</span><div class="lbl">Click + above the list to create a new Video Input.</div></div></div></div>
<div class="badge" style="left:388px;top:98px">1</div><div class="badge" style="left:566px;top:290px">2</div></div>
<div class="cap">Illustration of the ProPresenter 20 steps &mdash; not a screenshot</div>""",
"pp-2-device": f"""
<div class="win"><div class="bar"><span class="dot"></span><span class="dot"></span><span class="dot"></span>&nbsp; Settings &mdash; Inputs</div>
<div class="tabs"><div class="tab on">Video Inputs</div><div class="tab">Audio Inputs</div></div>
<div class="body"><div class="row"><div class="list"><div style="background:#252a35">Overlay Studio</div></div>
<div><div class="lbl" style="margin-top:0">Device</div><div class="sel hl">Overlay Studio &bull; Venue &nbsp;(NDI) &nbsp;&#9662;</div>
<div class="lbl">Live preview</div><div class="prev">{chips()}</div>
<div class="lbl">Name</div><div class="sel" style="width:300px">Overlay Studio</div></div></div></div>
<div class="badge" style="left:300px;top:148px">3</div><div class="badge" style="left:712px;top:222px">4</div><div class="badge" style="left:632px;top:468px">5</div></div>
<div class="cap">Illustration of the ProPresenter 20 steps &mdash; not a screenshot</div>""",
"pp-3-trigger": f"""
<div class="win"><div class="bar"><span class="dot"></span><span class="dot"></span><span class="dot"></span>&nbsp; ProPresenter</div>
<div class="body"><div class="row"><div style="width:380px"><div class="lbl" style="margin-top:0">Media Bin &mdash; Video Input Playlist</div>
<div class="list"><div>Camera 1</div><div class="hl" style="background:#1f3b52">&#9654; &nbsp;Overlay Studio</div></div>
<div class="lbl" style="margin-top:26px">Click the input to send it to the screens.</div></div>
<div><div class="lbl" style="margin-top:0">Audience screen</div><div class="prev" style="width:600px;height:340px;background:#05070b"><div class="chip" style="left:18px;top:40px;width:106px;height:190px;background:#2065c0;font-size:60px">0</div><div class="chip" style="left:134px;top:40px;width:106px;height:190px;background:#c0431f;font-size:60px">0</div><div class="chip" style="left:250px;top:40px;width:106px;height:190px;background:#1f9670;font-size:60px">0</div><div class="chip" style="left:366px;top:40px;width:106px;height:190px;background:#7b3bb8;font-size:60px">0</div><div class="chip" style="left:482px;top:40px;width:106px;height:190px;background:#b58a14;font-size:60px">0</div><div style="position:absolute;left:0;right:0;bottom:18px;text-align:center;font-size:26px;color:#9aa3b5">your slides / lyrics / video behind</div></div></div></div></div>
<div class="badge" style="left:344px;top:138px">6</div></div>
<div class="cap">Illustration of the ProPresenter 20 steps &mdash; not a screenshot</div>""",
}

for name, body in PAGES.items():
    html = f"<!doctype html><meta charset=utf-8><style>{CSS}</style>{body}"
    with tempfile.NamedTemporaryFile("w", suffix=".html", delete=False) as f:
        f.write(html); path = f.name
    subprocess.run([CHROME, "--headless=new", "--disable-gpu", "--hide-scrollbars", f"--screenshot={OUT/(name+'.png')}",
                    "--window-size=1200,700", f"file://{path}"], check=True, capture_output=True)
    print("wrote", name)
