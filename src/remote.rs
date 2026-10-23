use crate::model::Project;
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Duration,
};

pub use crate::domain::controls::{Action, Control, ControllerLayout};

#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub project: String,
    pub values: std::collections::BTreeMap<String, String>,
    #[serde(serialize_with = "serialize_web_layout")]
    pub layout: ControllerLayout,
    pub teams: Vec<TeamSnapshot>,
    pub clock_running: bool,
    pub clocks: Vec<ClockSnapshot>,
    pub outputs: Vec<String>,
}
#[derive(Clone, Serialize)]
pub struct TeamSnapshot {
    pub label: String,
    pub name: String,
    pub score: String,
    pub color: [u8; 3],
}
#[derive(Clone, Serialize)]
pub struct ClockSnapshot {
    pub name: String,
    pub value: String,
    pub running: bool,
}
impl Snapshot {
    pub fn from_project(p: &Project, running: &[bool], outputs: Vec<String>) -> Self {
        Self {
            project: p.name.clone(),
            values: p.values.clone(),
            layout: p.controller.clone(),
            teams: p
                .teams
                .iter()
                .map(|team| TeamSnapshot {
                    label: team.label.clone(),
                    name: p.values.get(&team.name_key).cloned().unwrap_or_default(),
                    score: p.values.get(&team.score_key).cloned().unwrap_or_default(),
                    color: team.color,
                })
                .collect(),
            clock_running: running.first().copied().unwrap_or(false),
            clocks: p
                .clocks
                .iter()
                .enumerate()
                .map(|(i, c)| ClockSnapshot {
                    name: c.name.clone(),
                    value: crate::model::clock_text(c.seconds),
                    running: running.get(i).copied().unwrap_or(false),
                })
                .collect(),
            outputs,
        }
    }
}
// Browser numbers cannot represent all persisted u64 IDs. Send decimal strings,
// while accepting numeric IDs from older controllers for compatibility.
fn serialize_web_layout<S: serde::Serializer>(
    layout: &ControllerLayout,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::Error;
    let mut value = serde_json::to_value(layout).map_err(S::Error::custom)?;
    if let Some(controls) = value["controls"].as_array_mut() {
        for (control, original) in controls.iter_mut().zip(&layout.controls) {
            control["id"] = serde_json::Value::String(original.id.to_string());
        }
    }
    value.serialize(serializer)
}
fn deserialize_button_id<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Text(String),
        Number(u64),
    }
    match Id::deserialize(deserializer)? {
        Id::Text(value) => value.parse().map_err(serde::de::Error::custom),
        Id::Number(value) => Ok(value),
    }
}
#[derive(Deserialize)]
struct Click {
    #[serde(deserialize_with = "deserialize_button_id")]
    button: u64,
    request_id: String,
}
pub struct Command {
    pub button: u64,
    pub request_id: String,
    pub reply: SyncSender<Result<(), String>>,
}
pub struct Remote {
    pub url: String,
    pub address: String,
    pub commands: Receiver<Command>,
    pub snapshot: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
pub fn addresses() -> Vec<String> {
    let mut result = vec![];
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for i in interfaces {
            if let IpAddr::V4(ip) = i.ip()
                && !ip.is_loopback()
                && ip.is_private()
            {
                result.push(ip.to_string());
            }
        }
    }
    result.sort();
    result.dedup();
    result.push("127.0.0.1".into());
    result
}
impl Remote {
    pub fn start(
        ip: &str,
        port: u16,
        snapshot: Snapshot,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        let addr: Ipv4Addr = ip.parse().map_err(|_| "Choose a valid IPv4 address")?;
        if !addr.is_private() && !addr.is_loopback() {
            return Err("Remote control must bind to a local/private address".into());
        }
        let server = tiny_http::Server::http((addr, port)).map_err(|e| e.to_string())?;
        let address = server
            .server_addr()
            .to_ip()
            .ok_or("No listening address")?
            .to_string();
        let token: String = rand::random::<[u8; 32]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let url = format!("http://{address}/#token={token}");
        let (tx, commands) = mpsc::sync_channel(32);
        let snapshot = Arc::new(Mutex::new(snapshot));
        let state = snapshot.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = stop.clone();
        let worker = std::thread::spawn(move || {
            while !stop_worker.load(Ordering::Relaxed) {
                let request = match server.recv_timeout(Duration::from_millis(100)) {
                    Ok(Some(r)) => r,
                    _ => continue,
                };
                handle(request, &token, &state, &tx, &ctx);
            }
        });
        Ok(Self {
            url,
            address,
            commands,
            snapshot,
            stop,
            worker: Some(worker),
        })
    }
}
impl Drop for Remote {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn respond(request: tiny_http::Request, status: u16, body: String, content_type: &str) {
    let mut r = tiny_http::Response::from_string(body).with_status_code(status);
    for (k, v) in [
        ("Content-Type", content_type),
        ("Cache-Control", "no-store"),
        ("Referrer-Policy", "no-referrer"),
        ("X-Content-Type-Options", "nosniff"),
        (
            "Content-Security-Policy",
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'",
        ),
    ] {
        r.add_header(tiny_http::Header::from_bytes(k, v).unwrap());
    }
    let _ = request.respond(r);
}
fn handle(
    mut request: tiny_http::Request,
    token: &str,
    state: &Arc<Mutex<Snapshot>>,
    commands: &SyncSender<Command>,
    ctx: &egui::Context,
) {
    let path = request.url().to_string();
    let method = request.method().as_str().to_string();
    if method == "GET" && path == "/" {
        respond(
            request,
            200,
            include_str!("../web/controller.html").into(),
            "text/html; charset=utf-8",
        );
        return;
    }
    if method == "GET" && path == "/controller.js" {
        respond(
            request,
            200,
            include_str!("../web/controller.js").into(),
            "text/javascript; charset=utf-8",
        );
        return;
    }
    let auth = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str());
    if auth != Some(format!("Bearer {token}").as_str()) {
        respond(
            request,
            401,
            "Pair by scanning the desktop QR code.".into(),
            "text/plain",
        );
        return;
    }
    if method == "GET" && path == "/api/state" {
        let body = state
            .lock()
            .ok()
            .and_then(|s| serde_json::to_string(&*s).ok());
        match body {
            Some(body) => respond(request, 200, body, "application/json"),
            None => respond(request, 503, "State unavailable".into(), "text/plain"),
        };
        return;
    }
    if method == "POST" && path == "/api/action" {
        if request.body_length().is_some_and(|n| n > 4096)
            || !request.headers().iter().any(|h| {
                h.field.equiv("Content-Type") && h.value.as_str().starts_with("application/json")
            })
        {
            respond(request, 400, "Invalid request".into(), "text/plain");
            return;
        }
        let mut body = String::new();
        use std::io::Read;
        if request
            .as_reader()
            .take(4097)
            .read_to_string(&mut body)
            .is_err()
            || body.len() > 4096
        {
            respond(request, 400, "Invalid request".into(), "text/plain");
            return;
        }
        let click: Click = match serde_json::from_str(&body) {
            Ok(c) => c,
            Err(_) => {
                respond(request, 400, "Invalid action".into(), "text/plain");
                return;
            }
        };
        if click.request_id.len() > 96 || click.request_id.is_empty() {
            respond(request, 400, "Invalid request ID".into(), "text/plain");
            return;
        }
        if !state
            .lock()
            .is_ok_and(|s| s.layout.controls.iter().any(|b| b.id == click.button))
        {
            respond(request, 404, "Button no longer exists".into(), "text/plain");
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        if commands
            .try_send(Command {
                button: click.button,
                request_id: click.request_id,
                reply: tx,
            })
            .is_err()
        {
            respond(request, 503, "Controller busy".into(), "text/plain");
            return;
        }
        ctx.request_repaint();
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => respond(request, 200, "{\"ok\":true}".into(), "application/json"),
            Ok(Err(e)) => respond(request, 409, e, "text/plain"),
            Err(_) => respond(
                request,
                504,
                "No confirmation. Check the live score before trying again.".into(),
                "text/plain",
            ),
        };
        return;
    }
    respond(request, 404, "Not found".into(), "text/plain");
}
pub fn qr_image(url: &str) -> Result<egui::ColorImage, String> {
    let code = qrcode::QrCode::new(url).map_err(|e| e.to_string())?;
    let size = code.width() + 8;
    let mut pixels = vec![egui::Color32::WHITE; size * size];
    for y in 0..code.width() {
        for x in 0..code.width() {
            if code[(x, y)] == qrcode::Color::Dark {
                pixels[(y + 4) * size + x + 4] = egui::Color32::BLACK;
            }
        }
    }
    Ok(egui::ColorImage::new([size, size], pixels))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(address: &str, method: &str, path: &str, token: &str, body: &str) -> String {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        write!(stream,"{method} {path} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }
    #[test]
    fn layout_validation() {
        let mut p = Project::fresh();
        p.validate().unwrap();
        p.controller.controls[0].col = 10;
        assert!(p.validate().is_err());
    }
    #[test]
    fn generated_five_team_layout_keeps_custom_steps_and_has_no_overlaps() {
        let mut project = Project::fresh();
        for _ in 0..3 {
            project.add_team();
        }
        project.teams[4].steps = vec![-5, 1, 5, 10];
        let layout = ControllerLayout::for_game(&project).unwrap();
        assert!(layout.controls.iter().any(|c|matches!(&c.action, Action::Adjust { field, step: 10 } if field == &project.teams[4].score_key)));
        for (i, a) in layout.controls.iter().enumerate() {
            for b in &layout.controls[i + 1..] {
                let overlaps = a.col < b.col + b.width
                    && b.col < a.col + a.width
                    && a.row < b.row + b.height
                    && b.row < a.row + a.height;
                assert!(!overlaps, "{} overlaps {}", a.label, b.label);
            }
        }
    }
    #[test]
    fn browser_roundtrip_keeps_full_width_button_ids() {
        let mut p = Project::fresh();
        let id = 9_007_199_254_740_993;
        p.controller.controls[0].id = id;
        let value = serde_json::to_value(Snapshot::from_project(&p, &[false], vec![])).unwrap();
        assert_eq!(value["layout"]["controls"][0]["id"], id.to_string());
        let click: Click = serde_json::from_value(
            serde_json::json!({"button":id.to_string(),"request_id":"test"}),
        )
        .unwrap();
        assert_eq!(click.button, id);
        let old: Click = serde_json::from_str(r#"{"button":1,"request_id":"legacy"}"#).unwrap();
        assert_eq!(old.button, 1);
    }
    #[test]
    fn wheel_actions_roundtrip_validate_and_snapshot_winner() {
        let mut p = Project::fresh();
        let actions = [Action::SpinWheel, Action::RemoveWheelWinner, Action::RestoreWheel];
        for (i, action) in actions.iter().enumerate() {
            p.controller.controls.push(Control {
                id: 900 + i as u64,
                label: format!("Wheel {i}"),
                col: 0,
                row: 40 + 2 * i as u8,
                width: 6,
                height: 2,
                color: [19, 95, 123],
                confirm: matches!(action, Action::RemoveWheelWinner),
                action: action.clone(),
            });
        }
        p.validate().unwrap();
        p.values.insert("wheel.winner".into(), "Ada".into());
        let value = serde_json::to_value(Snapshot::from_project(&p, &[false], vec![])).unwrap();
        assert_eq!(value["values"]["wheel.winner"], "Ada");
        let mut layout = value["layout"].clone();
        for control in layout["controls"].as_array_mut().unwrap() {
            let id: u64 = control["id"].as_str().unwrap().parse().unwrap();
            control["id"] = id.into();
        }
        let back: ControllerLayout = serde_json::from_value(layout).unwrap();
        let wheel: Vec<_> = back.controls.iter().filter(|c| c.id >= 900).collect();
        assert_eq!(wheel.len(), 3);
        for (c, a) in wheel.iter().zip(&actions) {
            assert_eq!(&c.action, a);
        }
        assert!(wheel[1].confirm);
    }
    #[test]
    fn qr_has_quiet_zone() {
        let image = qr_image("http://127.0.0.1:9876/#token=example").unwrap();
        assert_eq!(image.pixels[0], egui::Color32::WHITE);
        assert!(image.pixels.contains(&egui::Color32::BLACK));
    }
    #[test]
    fn remote_http_auth_and_dispatch() {
        let p = Project::fresh();
        let remote = Remote::start(
            "127.0.0.1",
            0,
            Snapshot::from_project(&p, &[false], vec![]),
            egui::Context::default(),
        )
        .unwrap();
        let token = remote.url.split("#token=").nth(1).unwrap().to_string();
        assert!(
            request(&remote.address, "GET", "/api/state", "wrong", "").starts_with("HTTP/1.1 401")
        );
        let state = request(&remote.address, "GET", "/api/state", &token, "");
        assert!(state.starts_with("HTTP/1.1 200"));
        assert!(state.contains("EAGLES"));
        let address = remote.address.clone();
        let worker = std::thread::spawn(move || {
            request(
                &address,
                "POST",
                "/api/action",
                &token,
                "{\"button\":1,\"request_id\":\"test:1\"}",
            )
        });
        let command = remote
            .commands
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(command.button, 1);
        assert_eq!(command.request_id, "test:1");
        command.reply.send(Ok(())).unwrap();
        assert!(worker.join().unwrap().starts_with("HTTP/1.1 200"));
        let address = remote.address.clone();
        drop(remote);
        assert!(std::net::TcpStream::connect(&address).is_err());
    }
}
