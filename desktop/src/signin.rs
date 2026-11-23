//! Catching the person's browser as it comes back from signing in to an MCP server
//! (PLAN.md, "Signing in to MCP servers"): a listener on 127.0.0.1 for one request to
//! `/callback`, whose query goes to the instance as it came, with `mcp.signed_in`. The
//! instance holds the PKCE verifier, so the code in it is of no use to anything else.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The port tried first, so the address an authorization server knows loom by stays
/// the same from one sign-in to the next.
const PORT: u16 = 47251;
/// How long the browser has to come back.
const WAIT: Duration = Duration::from_secs(600);

const PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><title>loom</title><p style=\"font: 16px system-ui, sans-serif; margin: 48px\">You can close this tab and go back to loom.</p>";

/// A listener waiting for the browser: the address to send it back to, and what it
/// brings, once it does.
pub struct Catcher {
    pub redirect: String,
    pub back: mpsc::Receiver<String>,
}

/// Listens on loopback, on its own thread, until the browser comes back or ten
/// minutes pass.
pub fn listen() -> std::io::Result<Catcher> {
    let listener =
        TcpListener::bind(("127.0.0.1", PORT)).or_else(|_| TcpListener::bind(("127.0.0.1", 0)))?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let until = Instant::now() + WAIT;
        while Instant::now() < until {
            match listener.accept() {
                Ok((stream, _)) => {
                    if let Some(query) = answer(stream) {
                        let _ = tx.send(query);
                        return;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(_) => return,
            }
        }
    });
    Ok(Catcher {
        redirect: format!("http://127.0.0.1:{port}/callback"),
        back: rx,
    })
}

/// Answers one request: a page saying to go back to loom for `/callback`, which gives
/// its query, and 404 for anything else, such as the browser's icon.
fn answer(mut stream: TcpStream) -> Option<String> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut head = Vec::new();
    let mut chunk = [0u8; 4096];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 65536 {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        head.extend_from_slice(&chunk[..n]);
    }
    let query = callback_query(&String::from_utf8_lossy(&head));
    let (status, body) = match query {
        Some(_) => ("200 OK", PAGE),
        None => ("404 Not Found", ""),
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    query
}

/// The query of a request for `/callback`, from its first line, or none.
fn callback_query(head: &str) -> Option<String> {
    let target = head.lines().next()?.split(' ').nth(1)?;
    let rest = target.strip_prefix("/callback")?;
    if rest.is_empty() {
        return Some(String::new());
    }
    rest.strip_prefix('?').map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_only_the_callback() {
        assert_eq!(
            callback_query("GET /callback?code=a&state=b HTTP/1.1\r\nHost: x\r\n\r\n"),
            Some("code=a&state=b".into())
        );
        assert_eq!(callback_query("GET /favicon.ico HTTP/1.1\r\n\r\n"), None);
        assert_eq!(
            callback_query("GET /callbackx?code=a HTTP/1.1\r\n\r\n"),
            None
        );
        assert_eq!(callback_query(""), None);
    }

    #[test]
    fn hands_back_what_the_browser_brought() {
        let c = listen().unwrap();
        let addr = c.redirect.strip_prefix("http://").unwrap();
        let host = addr.split('/').next().unwrap().to_string();
        let mut icon = TcpStream::connect(&host).unwrap();
        icon.write_all(b"GET /favicon.ico HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut said = String::new();
        icon.read_to_string(&mut said).unwrap();
        assert!(said.starts_with("HTTP/1.1 404"));
        let mut s = TcpStream::connect(&host).unwrap();
        s.write_all(b"GET /callback?code=c1&state=s1 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        let mut page = String::new();
        s.read_to_string(&mut page).unwrap();
        assert!(page.starts_with("HTTP/1.1 200") && page.contains("go back to loom"));
        assert_eq!(
            c.back.recv_timeout(Duration::from_secs(5)).unwrap(),
            "code=c1&state=s1"
        );
    }
}
