#![forbid(unsafe_code)]
//! The offline promises, checked against the real sync server.
//!
//! Three things a person is promised when their device has been away, and each of them only a running process can keep:
//! syncing twice does not double their Book, two devices that changed the same record are told about it rather than one
//! of them losing the change silently, and a batch that was interrupted resumes instead of starting again.
//!
//! The process is started the way a deployment starts it and spoken to over a socket, because the point is the binary
//! rather than the router function. The HTTP client is written out by hand - a request, a status line, a body - because
//! the workspace carries no HTTP client crate and a test dependency that cannot be fetched offline would defeat the
//! offline story it is testing.
//!
//! When the binary is not built the tests say so and pass. Preflight builds it first; a machine running `cargo test` on a
//! fresh checkout gets a skip that names what it skipped.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PASSWORD: &str = "Sync-Pass!2026";

fn binary() -> PathBuf {
    if let Ok(path) = std::env::var("VIMBAI_SYNC_BIN") {
        return PathBuf::from(path);
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    here.join("../../target").join(profile).join("vimbai-sync")
}

/// The port the server says it is listening on, read from its own words rather than assumed.
///
/// A port found free by binding and closing it can be taken by anything else before this process binds, and then the test
/// talks to a stranger. Asking for port 0 and reading back the answer removes the window.
fn announced_port(child: &mut Child) -> u16 {
    let stdout = child.stdout.as_mut().expect("a piped standard out");
    let mut reader = BufReader::new(stdout);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            panic!("the sync server stopped before saying which port it listens on");
        }
        if let Some(rest) = line.split("listening on 127.0.0.1:").nth(1) {
            if let Some(port) = rest
                .split_whitespace()
                .next()
                .and_then(|text| text.parse::<u16>().ok())
            {
                return port;
            }
        }
    }
    panic!("the sync server did not say which port it listens on within 20s");
}

struct Sync {
    child: Child,
    port: u16,
}

impl Sync {
    fn start(data_dir: &Path) -> Option<Sync> {
        let binary = binary();
        if !binary.is_file() {
            eprintln!("skipped: {} is not built", binary.display());
            return None;
        }
        std::fs::create_dir_all(data_dir).expect("a data directory");
        let mut child = Command::new(&binary)
            .arg("--port")
            .arg("0")
            .arg("--data-dir")
            .arg(data_dir)
            .stdout(Stdio::piped())
            .spawn()
            .expect("the sync server starts");
        let port = announced_port(&mut child);
        // Keep reading, or a chatty server eventually blocks on a pipe nobody drains.
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    line.clear();
                }
            });
        }
        let server = Sync { child, port };
        server.wait_until_ready();
        Some(server)
    }

    fn wait_until_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Some((200, _)) = self.try_request("GET", "/health", &[], None) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("the sync server did not answer /health within 20s");
    }

    fn try_request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> Option<(u16, String)> {
        self.request(method, path, headers, body).ok()
    }

    /// One request, one response, on a connection that closes: nothing to parse but the status line and the body.
    fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> std::io::Result<(u16, String)> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port))?;
        stream.set_read_timeout(Some(Duration::from_secs(20)))?;
        let mut request =
            format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        if let Some(body) = body {
            request.push_str("Content-Type: application/json\r\n");
            request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        request.push_str("\r\n");
        if let Some(body) = body {
            request.push_str(body);
        }
        stream.write_all(request.as_bytes())?;
        stream.flush()?;
        // Read until the body the headers promised has arrived. Closing our side first made a server answer nothing at
        // all the one time this was tried, so the connection is left open and the promised length is trusted.
        let mut raw = Vec::new();
        let mut expected: Option<usize> = None;
        let mut buffer = [0u8; 8192];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    raw.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&raw).into_owned();
                    if expected.is_none() {
                        if let Some((head, _)) = text.split_once("\r\n\r\n") {
                            let length = head.lines().find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                if name.eq_ignore_ascii_case("content-length") {
                                    value.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            });
                            expected = Some(text.len() - head.len() - 4)
                                .map(|have| have + length.unwrap_or(0));
                        }
                    }
                    if let Some(total) = expected {
                        if raw.len() >= total {
                            break;
                        }
                    }
                }
                Err(problem) if problem.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(problem) if problem.kind() == std::io::ErrorKind::TimedOut => break,
                Err(problem) => return Err(problem),
            }
        }
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .unwrap_or(0);
        let body = text
            .split_once("\r\n\r\n")
            .map(|(_, body)| body.to_string())
            .unwrap_or_default();
        Ok((status, body))
    }

    fn post(&self, path: &str, body: &str, headers: &[(&str, &str)]) -> (u16, String) {
        self.request("POST", path, headers, Some(body))
            .expect("the sync server answers")
    }

    fn get(&self, path: &str, headers: &[(&str, &str)]) -> (u16, String) {
        self.request("GET", path, headers, None)
            .expect("the sync server answers")
    }
}

impl Drop for Sync {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn scratch(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("vimbai-sync-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    base
}

/// The first string value for `key` anywhere in a JSON document, which is enough to read an id or a token out of an
/// answer whose shape the domain tests already pin.
fn json_string(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":");
    let at = body.find(&marker)? + marker.len();
    let rest = body[at..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// The first number for `key`, for counts the server answers.
fn json_number(body: &str, key: &str) -> Option<i64> {
    let marker = format!("\"{key}\":");
    let at = body.find(&marker)? + marker.len();
    let rest = body[at..].trim_start();
    let digits: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    digits.parse().ok()
}

fn sign_in(server: &Sync, tag: &str) -> Vec<(String, String)> {
    let username = format!("sync_{tag}_{}", std::process::id());
    let register = format!(
        r#"{{"email":"{username}@example.com","username":"{username}","password":"{PASSWORD}","first_name":"Sync","last_name":"{tag}"}}"#
    );
    let (status, body) = server.post("/identity/users/register", &register, &[]);
    assert_eq!(status, 201, "registration answered {status}: {body}");
    let (status, body) = server.post(
        "/identity/users/login",
        &format!(r#"{{"username":"{username}","password":"{PASSWORD}"}}"#),
        &[],
    );
    assert_eq!(status, 200, "login answered {status}: {body}");
    let token = json_string(&body, "access_token").unwrap_or_else(|| panic!("no token in {body}"));
    vec![("Authorization".to_string(), format!("Bearer {token}"))]
}

fn pairs(headers: &[(String, String)]) -> Vec<(&str, &str)> {
    headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect()
}

/// A Book with one registered device, which is the state a phone is in after its first sync.
fn a_book_with_a_device(server: &Sync, tag: &str) -> (Vec<(String, String)>, String, String) {
    let auth = sign_in(server, tag);
    let (status, body) = server.post(
        "/books",
        r#"{"name":"Sync Book","tier":"personal"}"#,
        &pairs(&auth),
    );
    assert_eq!(status, 200, "opening a Book answered {status}: {body}");
    let book_id = json_string(&body, "id").unwrap_or_else(|| panic!("no book id in {body}"));

    let mut headers = pairs(&auth).to_vec();
    headers.push(("x-book-id", &book_id));
    let (status, body) = server.post(
        &format!("/books/{book_id}/devices"),
        r#"{"device_name":"phone"}"#,
        &headers,
    );
    assert_eq!(
        status, 200,
        "registering a device answered {status}: {body}"
    );
    let device_id =
        json_string(&body, "device_id").unwrap_or_else(|| panic!("no device in {body}"));
    (auth, book_id, device_id)
}

fn push(
    server: &Sync,
    auth: &[(String, String)],
    book_id: &str,
    device_id: &str,
    entries: &str,
) -> (u16, String) {
    let auth = pairs(auth).to_vec();
    let mut headers = auth.clone();
    headers.push(("x-book-id", book_id));
    server.post(
        &format!("/books/{book_id}/entries/push"),
        &format!(r#"{{"device_id":"{device_id}","entries":[{entries}]}}"#),
        &headers,
    )
}

fn pull(server: &Sync, auth: &[(String, String)], book_id: &str) -> (u16, String) {
    let auth = pairs(auth).to_vec();
    let mut headers = auth.clone();
    headers.push(("x-book-id", book_id));
    server.get(
        &format!("/books/{book_id}/entries?since=0&limit=1000"),
        &headers,
    )
}

#[test]
fn a_device_that_was_away_syncs_once_and_not_twice() {
    let data = scratch("twice");
    let Some(server) = Sync::start(&data) else {
        return;
    };
    let (auth, book_id, device_id) = a_book_with_a_device(&server, "twice");

    let batch = r#"{"entry_id":"e1","payload":"AAAA"},{"entry_id":"e2","payload":"BBBB"},{"entry_id":"e3","payload":"CCCC"}"#;
    let (status, body) = push(&server, &auth, &book_id, &device_id, batch);
    assert_eq!(status, 200, "the first push answered {status}: {body}");
    assert_eq!(json_number(&body, "accepted"), Some(3), "{body}");
    assert_eq!(json_number(&body, "already_stored"), Some(0), "{body}");

    // the answer was lost on the way back to the phone, so the phone sends the same batch again
    let (status, body) = push(&server, &auth, &book_id, &device_id, batch);
    assert_eq!(status, 200, "the repeat answered {status}: {body}");
    assert_eq!(
        json_number(&body, "accepted"),
        Some(0),
        "nothing new: {body}"
    );
    assert_eq!(
        json_number(&body, "already_stored"),
        Some(3),
        "and the phone is told its batch arrived the first time: {body}"
    );

    let (status, body) = pull(&server, &auth, &book_id);
    assert_eq!(status, 200, "the pull answered {status}: {body}");
    assert_eq!(
        json_number(&body, "count"),
        Some(3),
        "three entries, not six: {body}"
    );
    assert_eq!(json_number(&body, "book_seq"), Some(3), "{body}");
}

#[test]
fn two_devices_that_changed_the_same_ledger_entry_are_told_rather_than_one_losing_quietly() {
    let data = scratch("conflict");
    let Some(server) = Sync::start(&data) else {
        return;
    };
    let (auth, book_id, device_id) = a_book_with_a_device(&server, "conflict");

    // both devices are holding version 0 of the same journal entry
    let from_a =
        r#"{"entry_id":"a1","payload":"AAAA","record":{"key":"journal_entry:42","version":0}}"#;
    let from_b =
        r#"{"entry_id":"b1","payload":"BBBB","record":{"key":"journal_entry:42","version":0}}"#;

    let (status, body) = push(&server, &auth, &book_id, &device_id, from_a);
    assert_eq!(status, 200, "the first writer is accepted: {status} {body}");
    assert_eq!(json_number(&body, "accepted"), Some(1), "{body}");

    let (status, body) = push(&server, &auth, &book_id, &device_id, from_b);
    assert_eq!(status, 409, "the second writer is refused: {status} {body}");
    assert!(
        body.contains("journal_entry:42"),
        "the refusal names the record: {body}"
    );
    assert!(
        body.contains("version 0") && body.contains("at 1"),
        "and both versions, so somebody knows what to reopen: {body}"
    );

    // a refused batch stores nothing, so the phone is not left half-synced
    let (status, body) = pull(&server, &auth, &book_id);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        json_number(&body, "count"),
        Some(1),
        "only the accepted entry is in the Book: {body}"
    );
    assert!(
        !body.contains("\"b1\""),
        "the refused one is absent: {body}"
    );
}

#[test]
fn a_note_may_lose_an_update_but_says_so() {
    let data = scratch("lww");
    let Some(server) = Sync::start(&data) else {
        return;
    };
    let (auth, book_id, device_id) = a_book_with_a_device(&server, "lww");

    let first = r#"{"entry_id":"n1","payload":"AAAA","record":{"key":"note:7","version":0}}"#;
    let stale = r#"{"entry_id":"n2","payload":"BBBB","record":{"key":"note:7","version":0}}"#;
    let (status, body) = push(&server, &auth, &book_id, &device_id, first);
    assert_eq!(status, 200, "{status} {body}");
    let (status, body) = push(&server, &auth, &book_id, &device_id, stale);
    assert_eq!(
        status, 200,
        "a note is not worth refusing a sync over: {status} {body}"
    );
    assert!(
        body.contains("note:7") && body.contains("last_write_wins"),
        "and the answer says the update went over another: {body}"
    );
}

#[test]
fn a_batch_that_was_interrupted_resumes_rather_than_starting_again() {
    let data = scratch("resume");
    let Some(server) = Sync::start(&data) else {
        return;
    };
    let (auth, book_id, device_id) = a_book_with_a_device(&server, "resume");

    // the connection dropped after three of five entries reached the server
    let part = r#"{"entry_id":"e1","payload":"AAAA"},{"entry_id":"e2","payload":"BBBB"},{"entry_id":"e3","payload":"CCCC"}"#;
    let (status, body) = push(&server, &auth, &book_id, &device_id, part);
    assert_eq!(status, 200, "{status} {body}");
    assert_eq!(json_number(&body, "accepted"), Some(3), "{body}");

    // the phone comes back with all five, because it cannot know which three arrived
    let whole = r#"{"entry_id":"e1","payload":"AAAA"},{"entry_id":"e2","payload":"BBBB"},{"entry_id":"e3","payload":"CCCC"},{"entry_id":"e4","payload":"DDDD"},{"entry_id":"e5","payload":"EEEE"}"#;
    let (status, body) = push(&server, &auth, &book_id, &device_id, whole);
    assert_eq!(status, 200, "{status} {body}");
    assert_eq!(
        json_number(&body, "accepted"),
        Some(2),
        "only the two new: {body}"
    );
    assert_eq!(
        json_number(&body, "already_stored"),
        Some(3),
        "and the three it resent are counted, not stored: {body}"
    );

    let (status, body) = pull(&server, &auth, &book_id);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        json_number(&body, "count"),
        Some(5),
        "the Book holds five entries, in order: {body}"
    );
    assert!(
        body.contains("\"seq\":5"),
        "and the last one is the fifth, so nothing was stored out of order: {body}"
    );
}

#[test]
fn a_sync_server_will_not_post_a_journal_entry() {
    let data = scratch("surface");
    let Some(server) = Sync::start(&data) else {
        return;
    };
    let auth = sign_in(&server, "surface");
    let (status, body) = server.post(
        "/accounting/journal-entries",
        r#"{"lines":[]}"#,
        &pairs(&auth),
    );
    assert_eq!(
        status, 404,
        "the surface is narrow on purpose: {status} {body}"
    );
    assert!(
        body.contains("/books"),
        "and the refusal says what this server does serve: {body}"
    );
}
