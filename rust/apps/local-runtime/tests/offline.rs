//! The on-device promises, checked against the real binary.
//!
//! These are the promises that make the local runtime worth having, and every one of them is a claim about behaviour that
//! only a running process can keep: the platform is served on a machine with nothing else installed, a caller without a
//! token gets nowhere, a Book that is somebody else's stays somebody else's, and yesterday's bookkeeping is still there
//! after the process is gone.
//!
//! The process is started the way a machine starts it and spoken to over a socket, because the point is the binary rather
//! than the router function. The HTTP client is written out by hand - a request, a status line, a body - because the
//! workspace carries no HTTP client crate and a test dependency that cannot be fetched offline would defeat the offline
//! runtime it is testing.
//!
//! When the binary is not built the tests say so and pass. Preflight builds it first; a machine running `cargo test` on a
//! fresh checkout gets a skip that names what it skipped.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PASSWORD: &str = "Local-Pass!2026";

fn binary() -> PathBuf {
    if let Ok(path) = std::env::var("VIMBAI_LOCAL_BIN") {
        return PathBuf::from(path);
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    here.join("../../target").join(profile).join("vimbai-local")
}

/// The port the runtime says it is listening on, read from its own words rather than assumed.
fn announced_port(child: &mut Child) -> u16 {
    let stdout = child.stdout.as_mut().expect("a piped standard out");
    let mut reader = BufReader::new(stdout);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            panic!("the runtime stopped before saying which port it listens on");
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
    panic!("the runtime did not say which port it listens on within 20s");
}

struct Runtime {
    child: Child,
    port: u16,
}

impl Runtime {
    /// Start the runtime on a port the operating system chose for it.
    ///
    /// Asking for port 0 and reading back the port the runtime reports is what makes these tests honest under load. A
    /// port found free by binding and closing it can be taken by anything else in the window before the runtime binds,
    /// and then a test talks to somebody else's process - which is exactly how one of these failed once while the whole
    /// workspace was running beside it.
    fn start(data_dir: &Path) -> Option<Runtime> {
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
            .expect("the runtime starts");
        let port = announced_port(&mut child);
        // Keep reading, or a chatty runtime eventually blocks on a pipe nobody drains.
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    line.clear();
                }
            });
        }
        let runtime = Runtime { child, port };
        runtime.wait_until_ready();
        Some(runtime)
    }

    fn wait_until_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Some((200, _)) = self.try_get("/health") {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("the runtime did not answer /health within 20s");
    }

    fn try_get(&self, path: &str) -> Option<(u16, String)> {
        self.request("GET", path, &[], None).ok()
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
        // Read until the body the headers promised has arrived. Closing our side of the socket first made the server
        // answer nothing at all, which cost an hour to find, so the connection is left open and the length is trusted.
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
                        if let Some((headers, _)) = text.split_once("\r\n\r\n") {
                            let length = headers.lines().find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                if name.eq_ignore_ascii_case("content-length") {
                                    value.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            });
                            expected = Some(text.len() - headers.len() - 4)
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
            .expect("the runtime answers")
    }

    fn get(&self, path: &str, headers: &[(&str, &str)]) -> (u16, String) {
        self.request("GET", path, headers, None)
            .expect("the runtime answers")
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop();
    }
}

fn scratch(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("vimbai-offline-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    base
}

fn sign_up(runtime: &Runtime, tag: &str) -> Vec<(String, String)> {
    let username = format!("local_{tag}_{}", std::process::id());
    let register = format!(
        r#"{{"email":"{username}@example.com","username":"{username}","password":"{PASSWORD}","first_name":"Local","last_name":"{tag}"}}"#
    );
    let (status, body) = runtime.post("/identity/users/register", &register, &[]);
    assert_eq!(status, 201, "registration answered {status}: {body}");
    let (status, body) = runtime.post(
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

fn json_ids(body: &str) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut rest = body;
    while let Some(at) = rest.find("\"id\":") {
        let after = &rest[at + "\"id\":".len()..];
        let trimmed = after.trim_start();
        if let Some(quoted) = trimmed.strip_prefix('"') {
            if let Some(end) = quoted.find('"') {
                ids.insert(quoted[..end].to_string());
            }
        }
        rest = &after[1..];
    }
    ids
}

fn create_book(runtime: &Runtime, auth: &[(String, String)]) -> String {
    let (status, body) = runtime.post(
        "/book-sync/books",
        r#"{"name":"Local Book","tier":"business"}"#,
        &pairs(auth),
    );
    assert_eq!(status, 200, "a first book is created by its owner: {body}");
    json_string(&body, "id").unwrap_or_else(|| panic!("no book id in {body}"))
}

fn create_department(
    runtime: &Runtime,
    auth: &[(String, String)],
    book: &str,
    tag: &str,
) -> String {
    let mut headers = pairs(auth);
    // The header has to outlive the borrow, so it is held here rather than built inline.
    let body = format!(
        r#"{{"id":"loc-{tag}-1","name":"Sales","department_code":"LOC-{tag}","department_name":"Sales","department_type":"revenue","manager_id":"mgr-1","manager_name":"Sam"}}"#
    );
    headers.push(("X-Book-ID", book));
    let (status, answer) = runtime.post("/departmental-accounting/departments", &body, &headers);
    assert!(
        status == 200 || status == 201,
        "the department is created: {status} {answer}"
    );
    // A department create answers with the department and the cost centre it opened in the chart of accounts.
    json_string(&answer, "id").unwrap_or_else(|| panic!("no department id in {answer}"))
}

#[test]
fn the_runtime_serves_the_core_profile_by_itself() {
    let dir = scratch("health");
    let Some(runtime) = Runtime::start(&dir) else {
        return;
    };
    let (status, body) = runtime.get("/health", &[]);
    assert_eq!(status, 200, "{body}");
    assert!(
        body.contains("\"healthy\""),
        "health does not say healthy: {body}"
    );
    assert!(
        body.contains("\"core\""),
        "health does not name the profile: {body}"
    );
    assert!(
        body.contains("\"services_skipped\":0"),
        "the core profile skipped a service: {body}"
    );
}

#[test]
fn a_caller_without_a_token_gets_nowhere() {
    let dir = scratch("auth");
    let Some(runtime) = Runtime::start(&dir) else {
        return;
    };
    let (status, body) = runtime.get("/book-sync/books", &[]);
    assert_eq!(
        status, 401,
        "an unauthenticated read answered {status}: {body}"
    );
    // A plausible-looking token signed by something else is not a token.
    let (status, body) = runtime.get(
        "/book-sync/books",
        &[(
            "Authorization",
            "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZG1pbiJ9.forged",
        )],
    );
    assert_eq!(status, 401, "a forged token answered {status}: {body}");
}

#[test]
fn a_book_stays_the_property_of_its_owner() {
    let dir = scratch("books");
    let Some(runtime) = Runtime::start(&dir) else {
        return;
    };
    let mine = sign_up(&runtime, "owner");
    let book = create_book(&runtime, &mine);
    let theirs = sign_up(&runtime, "other");
    let mut borrowed = pairs(&theirs);
    borrowed.push(("X-Book-ID", book.as_str()));
    let (status, body) = runtime.get("/departmental-accounting/departments", &borrowed);
    assert_eq!(
        status, 403,
        "somebody else's Book answered {status}: {body}"
    );
    // The refusal may echo the Book the caller asked for - it is the caller's own id - but it may not carry records.
    assert!(
        !body.contains("departments"),
        "the refusal carries records: {body}"
    );
}

#[test]
fn a_published_prefix_reaches_the_engines_own_address() {
    let dir = scratch("prefix");
    let Some(runtime) = Runtime::start(&dir) else {
        return;
    };
    let auth = sign_up(&runtime, "prefix");
    let book = create_book(&runtime, &auth);
    // The gateway publishes /book-sync/books; the engine answers /books. A client that calls the published address must
    // be answered by the same handler, which is the whole point of the mount table.
    let (status, body) = runtime.get("/book-sync/books", &pairs(&auth));
    assert_eq!(
        status, 200,
        "the published prefix answered {status}: {body}"
    );
    assert!(
        json_ids(&body).contains(&book),
        "the created Book is missing from {body}"
    );
}

#[test]
fn the_bookkeeping_is_still_there_after_a_full_restart() {
    let dir = scratch("restart");
    let book;
    let username;
    {
        let Some(runtime) = Runtime::start(&dir) else {
            return;
        };
        let auth = sign_up(&runtime, "restart");
        username = json_string(&sign_up_body("restart2"), "username");
        book = create_book(&runtime, &auth);
        let department = create_department(&runtime, &auth, &book, "RST");
        let mut headers = pairs(&auth);
        headers.push(("X-Book-ID", book.as_str()));
        let (status, listing) = runtime.get("/departmental-accounting/departments", &headers);
        assert_eq!(status, 200, "{listing}");
        assert!(
            json_ids(&listing).contains(&department),
            "the department is not listed: {listing}"
        );
    }
    let _ = username;
    // A new process, the same directory: the store is a file, so the account and the Book survive.
    let Some(restarted) = Runtime::start(&dir) else {
        return;
    };
    let (status, _) = restarted.get("/health", &[]);
    assert_eq!(status, 200);
    let auth = sign_up_existing(&restarted, "restart");
    let (status, body) = restarted.get("/book-sync/books", &pairs(&auth));
    assert_eq!(
        status, 200,
        "after a restart the Book list answered {status}: {body}"
    );
    assert!(
        json_ids(&body).contains(&book),
        "the Book did not survive the restart: {body}"
    );
}

/// The username the `sign_up` helper would have registered, so a restart can log in rather than register again.
fn sign_up_body(tag: &str) -> String {
    format!("{{\"username\":\"local_{tag}_{}\"}}", std::process::id())
}

fn sign_up_existing(runtime: &Runtime, tag: &str) -> Vec<(String, String)> {
    let username = format!("local_{tag}_{}", std::process::id());
    let (status, body) = runtime.post(
        "/identity/users/login",
        &format!(r#"{{"username":"{username}","password":"{PASSWORD}"}}"#),
        &[],
    );
    assert_eq!(
        status, 200,
        "logging in after a restart answered {status}: {body}"
    );
    let token = json_string(&body, "access_token").unwrap_or_else(|| panic!("no token in {body}"));
    vec![("Authorization".to_string(), format!("Bearer {token}"))]
}
