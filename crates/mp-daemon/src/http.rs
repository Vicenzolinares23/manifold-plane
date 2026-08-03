//! A minimal threaded HTTP/1.1 server.
//!
//! Hand-rolled on `std::net` rather than pulled from an async framework. The
//! daemon serves a handful of fixed routes with small bodies, and the whole
//! request path is supposed to be auditable — an async runtime plus its
//! transitive tree would be several hundred thousand lines of dependency
//! underneath a component whose job is to be trustworthy.
//!
//! **TLS is not implemented here.** Kubernetes admission webhooks require
//! HTTPS, so a real deployment terminates TLS at a sidecar or ingress and
//! forwards plaintext over loopback. `deploy/` shows that arrangement. Saying
//! this plainly is better than shipping a half-built TLS stack.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}

pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Response {
            status,
            content_type: "application/json",
            body: body.into(),
        }
    }
    pub fn text(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Response {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.into(),
        }
    }
    pub fn not_found() -> Self {
        Response::text(404, "not found\n")
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

/// Read one HTTP/1.1 request, enforcing a body cap.
///
/// The cap is enforced against the declared `Content-Length` *before* reading,
/// so an oversized body is rejected without ever being buffered. An admission
/// controller that can be exhausted by a large POST is a denial-of-service
/// switch for the cluster it is supposed to protect.
fn read_request(stream: &mut TcpStream, max_body: usize) -> Result<Request, Response> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|_| Response::text(500, "io"))?);

    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return Err(Response::text(400, "bad request line\n"));
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    loop {
        let mut h = String::new();
        match reader.read_line(&mut h) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => return Err(Response::text(400, "bad headers\n")),
        }
        let t = h.trim_end();
        if t.is_empty() {
            break;
        }
        if let Some((k, v)) = t.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }

    if content_length > max_body {
        return Err(Response::text(413, "body too large\n"));
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 && reader.read_exact(&mut body).is_err() {
        return Err(Response::text(400, "short body\n"));
    }

    Ok(Request { method, path, body })
}

fn write_response(stream: &mut TcpStream, r: &Response) {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        r.status,
        reason(r.status),
        r.content_type,
        r.body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&r.body);
    let _ = stream.flush();
}

/// Serve until the process is killed.
///
/// One thread per connection, capped. Fine for admission-webhook traffic, which
/// is low-rate by nature — the Kubernetes API server does not fan out thousands
/// of concurrent admission reviews.
pub fn serve<F>(
    listen: &str,
    max_body: usize,
    max_threads: usize,
    handler: F,
) -> std::io::Result<()>
where
    F: Fn(Request) -> Response + Send + Sync + 'static,
{
    let listener = TcpListener::bind(listen)?;
    let handler = Arc::new(handler);
    let inflight = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        let current = inflight.load(std::sync::atomic::Ordering::SeqCst);
        if current >= max_threads {
            // Shed load rather than spawning unboundedly. 503 is the honest
            // answer; the caller's own failure policy then decides what to do,
            // which is the right place for that decision to live.
            write_response(&mut stream, &Response::text(503, "overloaded\n"));
            continue;
        }

        let handler = Arc::clone(&handler);
        let inflight = Arc::clone(&inflight);
        inflight.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        std::thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
            let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(10)));

            let resp = match read_request(&mut stream, max_body) {
                Ok(req) => handler(req),
                Err(e) => e,
            };
            write_response(&mut stream, &resp);
            inflight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reasons_cover_what_we_emit() {
        for s in [200u16, 400, 404, 413, 500, 503] {
            assert_ne!(reason(s), "Unknown", "missing reason for {s}");
        }
    }

    #[test]
    fn responses_carry_their_content_type() {
        assert_eq!(Response::json(200, "{}").content_type, "application/json");
        assert_eq!(Response::not_found().status, 404);
    }

    #[test]
    fn an_end_to_end_request_round_trips() {
        use std::io::Write as _;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        std::thread::spawn(move || {
            serve(&addr.to_string(), 1024, 4, |req| {
                Response::text(200, format!("{} {}", req.method, req.path))
            })
        });
        std::thread::sleep(std::time::Duration::from_millis(200));

        let mut s = std::net::TcpStream::connect(addr).unwrap();
        s.write_all(b"GET /healthz HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        assert!(out.contains("200 OK"), "got: {out}");
        assert!(out.contains("GET /healthz"), "got: {out}");
    }

    #[test]
    fn an_oversized_body_is_rejected_before_it_is_read() {
        use std::io::Write as _;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        std::thread::spawn(move || serve(&addr.to_string(), 16, 4, |_| Response::text(200, "ok")));
        std::thread::sleep(std::time::Duration::from_millis(200));

        let mut s = std::net::TcpStream::connect(addr).unwrap();
        s.write_all(b"POST /x HTTP/1.1\r\nHost: x\r\nContent-Length: 99999\r\n\r\n")
            .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        assert!(out.contains("413"), "got: {out}");
    }
}
