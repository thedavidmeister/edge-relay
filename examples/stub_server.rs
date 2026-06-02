//! Tiny std-only stub of the Lovense + Telegram HTTP APIs for the integration
//! test (tests/integration.sh). Records each request (method, path, body) as a
//! JSON line to `$REQ_LOG` and returns canned responses so the worker's
//! outbound calls succeed. Listens on `$STUB_PORT` (default 8788).
//!
//! One thread per connection, so back-to-back outbound calls from the worker
//! never queue behind each other.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::{env, fs, thread};

fn main() {
    let port = env::var("STUB_PORT").unwrap_or_else(|_| "8788".into());
    let log_path = env::var("REQ_LOG").unwrap_or_else(|_| "/tmp/stub-requests.log".into());
    let log = Arc::new(Mutex::new(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .expect("open REQ_LOG"),
    ));

    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind stub port");
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let log = Arc::clone(&log);
        thread::spawn(move || handle(stream, &log));
    }
}

fn handle(mut stream: TcpStream, log: &Mutex<fs::File>) {
    // Read until the end of headers.
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    let header_end = loop {
        let n = match stream.read(&mut tmp) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > 65_536 {
            return;
        }
    };

    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut req_line = head.split("\r\n").next().unwrap_or("").split_whitespace();
    let method = req_line.next().unwrap_or("").to_string();
    let path = req_line.next().unwrap_or("").to_string();

    let content_len = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
        })
        .unwrap_or(0);

    // Read the rest of the body.
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_len {
        let n = match stream.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_len);
    let body = String::from_utf8_lossy(&body).into_owned();

    let line = format!(
        "{{\"method\": \"{method}\", \"path\": \"{}\", \"body\": \"{}\"}}\n",
        escape(&path),
        escape(&body),
    );
    if let Ok(mut f) = log.lock() {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }

    let payload = if path.ends_with("/getQrCode") {
        "{\"code\":0,\"data\":{\"qr\":\"http://stub/qrcode.png\"}}"
    } else if path.ends_with("/command") {
        "{\"code\":200}"
    } else if path.ends_with("/sendMessage") {
        "{\"ok\":true}"
    } else {
        "{}"
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len(),
    );
    let _ = stream.write_all(resp.as_bytes());
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
