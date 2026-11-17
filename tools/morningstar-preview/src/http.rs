//! HTTP for the previewed app, carried out by the system's `curl`.
//!
//! On a phone the host makes an app's requests with the platform's own
//! stack (ADR 0019). The Mac's is reached here through `/usr/bin/curl`,
//! which every Mac has: it brings the system's TLS and certificates,
//! proxies, compression and redirects, and costs this repository no
//! dependency (an HTTP client with TLS is some thirty crates). The price is
//! a process per request, a few milliseconds each, which a development
//! tool can pay. Each request runs on its own thread; nothing here is ever
//! called on the thread that draws.
//!
//! The request is handed to `curl` as a configuration on its standard
//! input, not as arguments, so that an `Authorization` header never shows
//! in the process list. A body goes through a private temporary file,
//! removed when the request is over.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use morningstar_runtime::{
    HttpError, HttpMethod, HttpRequest, HttpResponse, MAX_HTTP_RESPONSE_BYTES,
};

/// How much of a response's head is read on top of the body's limit.
const MAX_HEAD_BYTES: usize = 256 * 1024;

static NEXT_BODY: AtomicU64 = AtomicU64::new(1);

/// Makes the request and waits for the whole answer. Blocks: call it off
/// the drawing thread. `scratch` is a directory for the body's temporary
/// file.
pub fn send(request: &HttpRequest, scratch: &Path) -> Result<HttpResponse, HttpError> {
    if !is_http_url(&request.url) {
        return Err(HttpError::InvalidUrl);
    }
    let body = if request.body.is_empty() {
        None
    } else {
        Some(BodyFile::write(scratch, &request.body).map_err(|error| {
            HttpError::Failed(format!("could not prepare the request's body: {error}"))
        })?)
    };
    let config = curl_config(request, body.as_ref().map(|body| body.path.as_path()))?;

    let mut child = Command::new("curl")
        .args(["--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| HttpError::Failed(format!("could not run curl: {error}")))?;

    // The configuration is small: it fits the pipe, so writing it before
    // reading cannot block.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }

    let limit = MAX_HTTP_RESPONSE_BYTES + MAX_HEAD_BYTES;
    let mut output = Vec::new();
    let mut too_large = false;
    if let Some(stdout) = child.stdout.take() {
        let read = stdout.take(limit as u64 + 1).read_to_end(&mut output);
        if read.is_ok() && output.len() > limit {
            too_large = true;
            let _ = child.kill();
        }
    }
    let mut complaint = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut complaint);
    }
    let status = child
        .wait()
        .map_err(|error| HttpError::Failed(format!("curl did not finish: {error}")))?;
    drop(body);

    if too_large {
        return Err(HttpError::TooLarge);
    }
    match status.code() {
        Some(0) => parse_output(&output),
        code => Err(error_for_exit(code, &complaint)),
    }
}

/// True for an absolute `http` or `https` URL without spaces or control
/// characters. Anything else (`file:`, a relative path) is refused before
/// `curl` sees it.
pub fn is_http_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"));
    rest.is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/'))
        && !url.chars().any(|ch| ch.is_control() || ch == ' ')
}

/// The request as a `curl` configuration file.
pub fn curl_config(request: &HttpRequest, body: Option<&Path>) -> Result<String, HttpError> {
    let mut config = String::from(
        "silent\nshow-error\ninclude\ncompressed\nlocation\nmax-redirs = 5\n\
         proto = \"=http,https\"\nproto-redir = \"=http,https\"\n",
    );
    config.push_str(&format!(
        "max-time = {:.3}\n",
        request.timeout.as_secs_f64().max(0.001)
    ));
    config.push_str(&format!("max-filesize = {MAX_HTTP_RESPONSE_BYTES}\n"));
    // After the body: the status and how many bytes of what came before it
    // are heads (one per redirect followed, and the answer's own).
    config.push_str("write-out = \"\\n%{http_code} %{size_header}\"\n");

    match request.method {
        HttpMethod::Head => config.push_str("head\n"),
        method => config.push_str(&format!("request = {}\n", quoted(method.as_str())?)),
    }

    let has = |name: &str| request.header_value(name).is_some();
    for (name, value) in &request.headers {
        if name.is_empty() || name.contains(':') || name.chars().any(char::is_whitespace) {
            return Err(HttpError::Failed(
                "a header's name is empty or has a colon or a space in it".to_owned(),
            ));
        }
        // `Name;` is how curl is told to send a header with no value.
        let line = if value.is_empty() {
            format!("{name};")
        } else {
            format!("{name}: {value}")
        };
        config.push_str(&format!("header = {}\n", quoted(&line)?));
    }
    // What curl would add by itself and a phone's stack does not.
    config.push_str("header = \"Expect:\"\n");
    if !has("accept") {
        config.push_str("header = \"Accept:\"\n");
    }
    if !has("user-agent") {
        config.push_str("user-agent = \"Morningstar-Preview\"\n");
    }
    if let Some(body) = body {
        if !has("content-type") {
            config.push_str("header = \"Content-Type:\"\n");
        }
        let path = body.to_str().ok_or_else(|| {
            HttpError::Failed("the temporary directory's path is not UTF-8".to_owned())
        })?;
        config.push_str(&format!("data-binary = {}\n", quoted(&format!("@{path}"))?));
    }
    config.push_str(&format!("url = {}\n", quoted(&request.url)?));
    Ok(config)
}

/// A value in double quotes as curl's configuration reads them. A value
/// that would break out of its line is refused.
fn quoted(text: &str) -> Result<String, HttpError> {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                return Err(HttpError::Failed(
                    "a header or the URL has a line break or a control character in it".to_owned(),
                ));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    Ok(out)
}

/// Reads what `curl` printed: the heads, the body, and the line asked for
/// with `write-out`.
pub fn parse_output(output: &[u8]) -> Result<HttpResponse, HttpError> {
    let malformed = || HttpError::Failed("curl's output could not be read".to_owned());
    let split = output
        .iter()
        .rposition(|byte| *byte == b'\n')
        .ok_or_else(malformed)?;
    let trailer = std::str::from_utf8(&output[split + 1..]).map_err(|_| malformed())?;
    let (status, head_bytes) = trailer.trim().split_once(' ').ok_or_else(malformed)?;
    let status: u16 = status.parse().map_err(|_| malformed())?;
    let head_bytes: usize = head_bytes.parse().map_err(|_| malformed())?;
    if head_bytes > split || status == 0 {
        return Err(malformed());
    }
    let body = &output[..split][head_bytes..];
    if body.len() > MAX_HTTP_RESPONSE_BYTES {
        return Err(HttpError::TooLarge);
    }

    // The last head is the answer's; those before it are of redirects
    // followed and of interim `100 Continue` answers.
    let heads = String::from_utf8_lossy(&output[..head_bytes]);
    let head = heads
        .rsplit("\r\n\r\n")
        .find(|block| !block.trim().is_empty())
        .unwrap_or("");
    let mut response = HttpResponse::new(status);
    for line in head.lines().skip(1) {
        if let Some((name, value)) = line.split_once(':') {
            response = response.with_header(name.trim(), value.trim());
        }
    }
    Ok(response.with_body(body))
}

/// What a failed `curl` means to the app. The numbers are curl's exit
/// codes, which are part of its interface.
pub fn error_for_exit(code: Option<i32>, complaint: &str) -> HttpError {
    match code {
        // Unsupported protocol, malformed URL.
        Some(1 | 3) => HttpError::InvalidUrl,
        // The proxy or the host did not resolve, the connection was
        // refused, the server said nothing, sending or receiving broke.
        Some(5 | 6 | 7 | 52 | 55 | 56) => HttpError::Offline,
        Some(28) => HttpError::Timeout,
        Some(63) => HttpError::TooLarge,
        _ => {
            let complaint = complaint.trim();
            HttpError::Failed(if complaint.is_empty() {
                match code {
                    Some(code) => format!("curl exited with {code}"),
                    None => "curl was stopped".to_owned(),
                }
            } else {
                complaint.to_owned()
            })
        }
    }
}

/// A request's body in a file only this user can read, removed on drop.
struct BodyFile {
    path: PathBuf,
}

impl BodyFile {
    fn write(dir: &Path, body: &[u8]) -> std::io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        fs::create_dir_all(dir)?;
        let path = dir.join(format!(
            "body-{}-{}",
            std::process::id(),
            NEXT_BODY.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        // From here the file exists: remove it whatever happens next.
        let file_guard = Self { path };
        file.write_all(body)?;
        Ok(file_guard)
    }
}

impl Drop for BodyFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn only_absolute_http_urls_pass() {
        assert!(is_http_url("http://127.0.0.1:8789/v1/health"));
        assert!(is_http_url("HTTPS://example.com"));
        assert!(!is_http_url("file:///etc/passwd"));
        assert!(!is_http_url("/v1/health"));
        assert!(!is_http_url("http://"));
        assert!(!is_http_url("http:///nohost"));
        assert!(!is_http_url("http://a b/"));
        assert!(!is_http_url("http://a/\nurl = \"file:///etc/passwd\""));
        assert!(!is_http_url(""));
    }

    #[test]
    fn the_configuration_carries_the_whole_request() {
        let request = HttpRequest::post("http://127.0.0.1:8789/v1/rooms")
            .header("X-Device-Id", "abc \"quoted\" \\ end")
            .header("X-Empty", "")
            .json_body(br#"{"a":1}"#.to_vec())
            .timeout(Duration::from_millis(2500));
        let config = curl_config(&request, Some(Path::new("/tmp/ms/body-1"))).unwrap();
        assert!(config.contains("request = \"POST\"\n"));
        assert!(config.contains("max-time = 2.500\n"));
        assert!(config.contains("header = \"X-Device-Id: abc \\\"quoted\\\" \\\\ end\"\n"));
        assert!(config.contains("header = \"X-Empty;\"\n"));
        assert!(config.contains("header = \"Content-Type: application/json\"\n"));
        assert!(config.contains("data-binary = \"@/tmp/ms/body-1\"\n"));
        assert!(config.ends_with("url = \"http://127.0.0.1:8789/v1/rooms\"\n"));
        assert!(
            !config.contains("header = \"Content-Type:\"\n"),
            "the app's own content type is not removed"
        );
    }

    #[test]
    fn what_curl_would_add_by_itself_is_taken_out() {
        let config = curl_config(&HttpRequest::get("https://example.com/"), None).unwrap();
        assert!(config.contains("request = \"GET\"\n"));
        assert!(config.contains("header = \"Expect:\"\n"));
        assert!(config.contains("header = \"Accept:\"\n"));
        assert!(config.contains("user-agent = \"Morningstar-Preview\"\n"));
        assert!(!config.contains("data-binary"));

        let head = HttpRequest::new(HttpMethod::Head, "https://example.com/");
        let config = curl_config(&head, None).unwrap();
        assert!(config.contains("\nhead\n"));
        assert!(!config.contains("request ="));

        let body = HttpRequest::post("https://example.com/").body(vec![1, 2, 3]);
        let config = curl_config(&body, Some(Path::new("/b"))).unwrap();
        assert!(
            config.contains("header = \"Content-Type:\"\n"),
            "no form content type is invented"
        );
    }

    #[test]
    fn a_header_cannot_add_lines_to_the_configuration() {
        // `HttpRequest::header` already removes line breaks; a request
        // whose fields were filled in directly gets no such help.
        let mut request = HttpRequest::get("https://example.com/");
        request.headers.push((
            "X-A".to_owned(),
            "value\nurl = \"file:///etc/passwd\"".to_owned(),
        ));
        assert!(matches!(
            curl_config(&request, None),
            Err(HttpError::Failed(_))
        ));
        let cleaned = HttpRequest::get("https://example.com/")
            .header("X-A", "value\nurl = \"file:///etc/passwd\"");
        let config = curl_config(&cleaned, None).unwrap();
        assert!(config.contains("header = \"X-A: valueurl = \\\"file:///etc/passwd\\\"\"\n"));
        let request = HttpRequest::get("https://example.com/").header("Bad Name", "x");
        assert!(curl_config(&request, None).is_err());
        let request = HttpRequest::get("https://example.com/").header("Bad:Name", "x");
        assert!(curl_config(&request, None).is_err());
    }

    #[test]
    fn the_last_head_is_the_answers() {
        let first = "HTTP/1.1 302 Found\r\nLocation: /b\r\n\r\n";
        let second = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Two:  spaced \r\n\r\n";
        let body = "{\"ok\":true}\nsecond line";
        let trailer = format!("\n200 {}", first.len() + second.len());
        let output = format!("{first}{second}{body}{trailer}");

        let response = parse_output(output.as_bytes()).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.header("content-type"), Some("application/json"));
        assert_eq!(response.header("x-two"), Some("spaced"));
        assert_eq!(response.header("location"), None);
        assert_eq!(response.text(), Some(body));
    }

    #[test]
    fn an_empty_body_and_a_binary_body_both_come_through() {
        let head = "HTTP/1.1 204 No Content\r\n\r\n";
        let output = format!("{head}\n204 {}", head.len());
        let response = parse_output(output.as_bytes()).unwrap();
        assert_eq!(response.status, 204);
        assert!(response.body.is_empty());

        let head = "HTTP/1.1 200 OK\r\n\r\n";
        let mut output = head.as_bytes().to_vec();
        output.extend([0u8, 255, b'\n', 10, 13]);
        output.extend(format!("\n200 {}", head.len()).as_bytes());
        let response = parse_output(&output).unwrap();
        assert_eq!(response.body, vec![0u8, 255, b'\n', 10, 13]);
    }

    #[test]
    fn output_that_is_not_curls_is_an_error_not_a_panic() {
        for bad in [
            &b""[..],
            b"no newline",
            b"body\nnot numbers",
            b"body\n200 9999",
            b"body\n000 0",
        ] {
            assert!(matches!(parse_output(bad), Err(HttpError::Failed(_))));
        }
    }

    #[test]
    fn curls_exit_codes_become_the_apps_errors() {
        assert_eq!(
            error_for_exit(Some(7), "curl: (7) refused"),
            HttpError::Offline
        );
        assert_eq!(error_for_exit(Some(6), ""), HttpError::Offline);
        assert_eq!(error_for_exit(Some(52), ""), HttpError::Offline);
        assert_eq!(error_for_exit(Some(28), ""), HttpError::Timeout);
        assert_eq!(error_for_exit(Some(3), ""), HttpError::InvalidUrl);
        assert_eq!(error_for_exit(Some(63), ""), HttpError::TooLarge);
        assert_eq!(
            error_for_exit(Some(35), " curl: (35) TLS handshake failed\n"),
            HttpError::Failed("curl: (35) TLS handshake failed".to_owned())
        );
        assert_eq!(
            error_for_exit(Some(2), ""),
            HttpError::Failed("curl exited with 2".to_owned())
        );
        assert_eq!(
            error_for_exit(None, ""),
            HttpError::Failed("curl was stopped".to_owned())
        );
    }

    #[test]
    fn a_body_file_is_private_and_removed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ms-preview-body-{}", std::process::id()));
        let path = {
            let body = BodyFile::write(&dir, b"secret").unwrap();
            let mode = fs::metadata(&body.path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
            assert_eq!(fs::read(&body.path).unwrap(), b"secret");
            body.path.clone()
        };
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
