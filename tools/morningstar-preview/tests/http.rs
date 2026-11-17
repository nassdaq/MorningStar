//! The preview's HTTP against a real server on this machine: a few lines
//! of `TcpListener` that answer by path. Needs `curl`, which macOS has.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use morningstar_preview::http::send;
use morningstar_runtime::{HttpError, HttpMethod, HttpRequest};

/// What the server saw of one request.
#[derive(Debug)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A server that answers until dropped, and reports each request.
struct Server {
    address: String,
    seen: mpsc::Receiver<Seen>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let (sender, seen) = mpsc::channel();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let sender = sender.clone();
                thread::spawn(move || serve(stream, &sender));
            }
        });
        Self { address, seen }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.address)
    }

    fn next(&self) -> Seen {
        self.seen.recv_timeout(Duration::from_secs(10)).unwrap()
    }
}

fn serve(stream: TcpStream, seen: &mpsc::Sender<Seen>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut stream = stream;
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let path = parts.next().unwrap_or("").to_owned();
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }
    let length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();

    let answer: Vec<u8> = match path.as_str() {
        "/json" => b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Served-By: test\r\n\
              Content-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}"
            .to_vec(),
        "/echo" => {
            let mut answer = format!(
                "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            answer.extend(&body);
            answer
        }
        "/missing" => b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\n\
                        not here\n"
            .to_vec(),
        "/moved" => b"HTTP/1.1 302 Found\r\nLocation: /json\r\nContent-Length: 0\r\n\
                      Connection: close\r\n\r\n"
            .to_vec(),
        "/empty" => b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_vec(),
        "/slow" => {
            thread::sleep(Duration::from_secs(5));
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
        }
        "/huge" => {
            // More than the 10 MB an app may be handed, without saying how
            // much is coming.
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
            let chunk = vec![b'x'; 1024 * 1024];
            for _ in 0..12 {
                if stream.write_all(&chunk).is_err() {
                    break;
                }
            }
            Vec::new()
        }
        "/hangup" => return,
        _ => b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
    };
    let _ = seen.send(Seen {
        method,
        path,
        headers,
        body,
    });
    let _ = stream.write_all(&answer);
}

/// A directory of its own for each test: one of them checks that nothing
/// is left behind.
fn scratch(test: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ms-preview-http-{test}-{}", std::process::id()))
}

#[test]
fn a_get_brings_back_status_headers_and_body() {
    let server = Server::start();
    let response = send(
        &HttpRequest::get(server.url("/json")),
        &scratch("a_get_brings_back_status"),
    )
    .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-type"), Some("application/json"));
    assert_eq!(response.header("x-served-by"), Some("test"));
    assert_eq!(response.text(), Some("{\"ok\":true}"));

    let seen = server.next();
    assert_eq!((seen.method.as_str(), seen.path.as_str()), ("GET", "/json"));
    assert_eq!(seen.header("user-agent"), Some("Morningstar-Preview"));
    assert_eq!(seen.header("accept"), None, "nothing the app did not send");
    assert_eq!(seen.header("expect"), None);
}

#[test]
fn a_post_sends_the_apps_headers_and_its_body_byte_for_byte() {
    let server = Server::start();
    let body: Vec<u8> = (0..=255u8).chain(*b"\r\n--\"quoted\"\\").collect();
    let request = HttpRequest::post(server.url("/echo"))
        .header("X-Device-Id", "abc-123")
        .header("Authorization", "Bearer \"odd\" \\ value")
        .header("Content-Type", "application/octet-stream")
        .body(body.clone());
    let response = send(&request, &scratch("a_post_sends_the_apps_he")).unwrap();
    assert_eq!(response.status, 201);
    assert_eq!(response.body, body);

    let seen = server.next();
    assert_eq!(seen.method, "POST");
    assert_eq!(seen.header("x-device-id"), Some("abc-123"));
    assert_eq!(
        seen.header("authorization"),
        Some("Bearer \"odd\" \\ value")
    );
    assert_eq!(
        seen.header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(seen.body, body);
    assert!(
        std::fs::read_dir(scratch("a_post_sends_the_apps_he"))
            .is_ok_and(|mut left| left.next().is_none()),
        "the body's temporary file is gone"
    );
}

#[test]
fn the_other_methods_arrive_as_themselves() {
    let server = Server::start();
    for (method, name) in [
        (HttpMethod::Put, "PUT"),
        (HttpMethod::Patch, "PATCH"),
        (HttpMethod::Delete, "DELETE"),
    ] {
        let request = HttpRequest::new(method, server.url("/echo")).json_body(b"{}".to_vec());
        let response = send(&request, &scratch("the_other_methods_arrive")).unwrap();
        assert_eq!(response.status, 201, "{name}");
        let seen = server.next();
        assert_eq!(seen.method, name);
        assert_eq!(seen.header("content-type"), Some("application/json"));
        assert_eq!(seen.body, b"{}");
    }

    let head = HttpRequest::new(HttpMethod::Head, server.url("/json"));
    let response = send(&head, &scratch("the_other_methods_arrive")).unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-type"), Some("application/json"));
    assert!(response.body.is_empty(), "a HEAD has no body");
    assert_eq!(server.next().method, "HEAD");
}

#[test]
fn an_error_status_is_an_answer_not_a_failure() {
    let server = Server::start();
    let response = send(
        &HttpRequest::get(server.url("/missing")),
        &scratch("an_error_status_is_an_an"),
    )
    .unwrap();
    assert_eq!(response.status, 404);
    assert_eq!(response.text(), Some("not here\n"));

    let response = send(
        &HttpRequest::get(server.url("/empty")),
        &scratch("an_error_status_is_an_an"),
    )
    .unwrap();
    assert_eq!(response.status, 204);
    assert!(response.body.is_empty());
}

#[test]
fn a_redirect_is_followed_and_only_the_last_answer_is_reported() {
    let server = Server::start();
    let response = send(
        &HttpRequest::get(server.url("/moved")),
        &scratch("a_redirect_is_followed_a"),
    )
    .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.header("location"), None);
    assert_eq!(response.header("x-served-by"), Some("test"));
    assert_eq!(response.text(), Some("{\"ok\":true}"));
}

#[test]
fn no_server_is_offline_and_a_slow_one_times_out() {
    // A port nothing listens on: bound, read, and closed again.
    let free = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let refused = send(
        &HttpRequest::get(format!("http://{free}/")),
        &scratch("no_server_is_offline_and"),
    );
    assert_eq!(refused, Err(HttpError::Offline));

    let server = Server::start();
    let hung_up = send(
        &HttpRequest::get(server.url("/hangup")),
        &scratch("no_server_is_offline_and"),
    );
    assert_eq!(hung_up, Err(HttpError::Offline));

    let started = Instant::now();
    let request = HttpRequest::get(server.url("/slow")).timeout(Duration::from_millis(400));
    assert_eq!(
        send(&request, &scratch("no_server_is_offline_and")),
        Err(HttpError::Timeout)
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the timeout is the request's, not the server's"
    );
}

#[test]
fn what_is_not_an_http_url_never_leaves() {
    for url in ["file:///etc/hosts", "ftp://example.com/", "/v1/health", ""] {
        assert_eq!(
            send(&HttpRequest::get(url), &scratch("what_is_not_an_http_url_")),
            Err(HttpError::InvalidUrl),
            "{url}"
        );
    }
}

#[test]
fn an_answer_larger_than_an_app_may_be_handed_is_refused() {
    let server = Server::start();
    assert_eq!(
        send(
            &HttpRequest::get(server.url("/huge")),
            &scratch("an_answer_larger_than_an")
        ),
        Err(HttpError::TooLarge)
    );
}
