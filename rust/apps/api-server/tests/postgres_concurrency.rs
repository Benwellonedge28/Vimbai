#![forbid(unsafe_code)]
//! Sixty-four writers at once, through a real process, against a real database, behind a door that admits four.
//!
//! The store's own tests already show that concurrent writes do not lose each other. What they cannot show is the whole
//! path: the door counting requests, the pool queueing for two connections, the tenancy gate reading a membership, and the
//! engine answering every caller. So this spawns the binary a deployment runs, points it at Postgres with a pool of two and
//! a door of four, and has sixty-four callers write at the same moment - then reads the Book back and counts.
//!
//! It needs a Postgres, so it skips unless VIMBAI_TEST_POSTGRES names one.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PASSWORD: &str = "Password-1234";

fn binary() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("VIMBAI_API_BIN") {
        return std::path::PathBuf::from(path);
    }
    let here = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    here.join("../../target").join(profile).join("vimbai-api")
}

struct Engine {
    child: Child,
    port: u16,
}

impl Engine {
    fn start(dsn: &str) -> Option<Engine> {
        let binary = binary();
        if !binary.is_file() {
            eprintln!("skipped: {} is not built", binary.display());
            return None;
        }
        let mut child = Command::new(&binary)
            .env("VIMBAI_API_ADDR", "127.0.0.1:0")
            .env("VIMBAI_POSTGRES_URL", dsn)
            .env("VIMBAI_POSTGRES_POOL", "2")
            .env("VIMBAI_API_MAX_IN_FLIGHT", "4")
            .env("VIMBAI_API_TIMEOUT_MS", "20000")
            .stdout(Stdio::piped())
            .spawn()
            .expect("the engine starts");
        let port = announced_port(&mut child);
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    line.clear();
                }
            });
        }
        let engine = Engine { child, port };
        engine.wait_until_ready();
        Some(engine)
    }

    fn wait_until_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Some((200, _)) = self.try_request("GET", "/health", &[], None) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the engine did not answer /health within 30s");
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> (u16, String) {
        self.try_request(method, path, headers, body)
            .unwrap_or_else(|| panic!("{method} {path} answered nothing"))
    }

    fn try_request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> Option<(u16, String)> {
        call(self.port, method, path, headers, body)
    }
}

/// One request over a socket of its own, so a caller in another thread needs nothing but the port.
fn call(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Option<(u16, String)> {
    {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .ok()?;
        let body = body.unwrap_or("");
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {}\r\n",
            body.len()
        );
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        if body.starts_with('{') {
            request.push_str("Content-Type: application/json\r\n");
        }
        request.push_str("\r\n");
        request.push_str(body);
        stream.write_all(request.as_bytes()).ok()?;
        let mut raw = String::new();
        stream.read_to_string(&mut raw).ok()?;
        let status = raw
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())?;
        let answer = raw.split("\r\n\r\n").nth(1).unwrap_or_default().to_owned();
        Some((status, answer))
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn announced_port(child: &mut Child) -> u16 {
    let stdout = child.stdout.as_mut().expect("a piped standard out");
    let mut reader = BufReader::new(stdout);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            panic!("the engine stopped before saying which port it listens on");
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
    panic!("the engine did not say which port it listens on within 30s");
}

fn json_string(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    Some(rest[..rest.find('"')?].to_owned())
}

#[test]
fn sixty_four_writers_behind_a_door_of_four_and_a_pool_of_two_lose_nothing() {
    let Ok(dsn) = std::env::var("VIMBAI_TEST_POSTGRES") else {
        eprintln!(
            "skipped: VIMBAI_TEST_POSTGRES is not set, so there is no deployment store to write to"
        );
        return;
    };
    let Some(engine) = Engine::start(&dsn) else {
        return;
    };

    let username = format!("concurrent_{}", std::process::id());
    let register = format!(
        r#"{{"email":"{username}@example.com","username":"{username}","password":"{PASSWORD}","first_name":"Con","last_name":"Current"}}"#
    );
    let (status, body) = engine.request("POST", "/identity/users/register", &[], Some(&register));
    assert_eq!(status, 201, "registration answered {status}: {body}");
    let caller = json_string(&body, "id").expect("the account's id");
    let login = format!(r#"{{"username":"{username}","password":"{PASSWORD}"}}"#);
    let (status, body) = engine.request("POST", "/identity/users/login", &[], Some(&login));
    assert_eq!(status, 200, "login answered {status}: {body}");
    let token = format!(
        "Bearer {}",
        json_string(&body, "access_token").expect("a token")
    );
    let book = {
        let (status, body) = engine.request(
            "POST",
            "/books",
            &[
                ("Authorization", token.as_str()),
                ("X-User-Id", caller.as_str()),
            ],
            Some(r#"{"name":"Loaded Book","tier":"business"}"#),
        );
        assert_eq!(status, 200, "opening a Book answered {status}: {body}");
        json_string(&body, "id").expect("a Book id")
    };

    // sixty-four writers at once: sixteen times the door, thirty-two times the pool. Eight readers run throughout, because
    // a queue of writers must not cost a reader its answer.
    let writers = 64usize;
    let port = engine.port;
    let mut readers = Vec::new();
    for _ in 0..8 {
        let token = token.clone();
        let book = book.clone();
        let caller = caller.clone();
        readers.push(std::thread::spawn(move || {
            let mut answered = 0;
            for _ in 0..4 {
                let (status, body) = call(
                    port,
                    "GET",
                    "/rules?limit=10",
                    &[
                        ("Authorization", &token),
                        ("X-User-Id", &caller),
                        ("X-Book-ID", &book),
                    ],
                    None,
                )
                .expect("a reader is answered");
                assert_eq!(
                    status, 200,
                    "a reader was refused while writers queued: {body}"
                );
                answered += 1;
            }
            answered
        }));
    }
    let mut senders = Vec::new();
    for index in 0..writers {
        let token = token.clone();
        let book = book.clone();
        let caller = caller.clone();
        senders.push(std::thread::spawn(move || {
            let rule = format!(
                r#"{{"name":"Rule {index}","category":"fraud","severity":"critical","condition":{{"type":"threshold","field":"amount","operator":"gt","value":{index}}},"cooldown_seconds":3600}}"#
            );
            let (status, body) = call(
                port,
                "POST",
                "/rules",
                &[("Authorization", &token), ("X-User-Id", &caller), ("X-Book-ID", &book)],
                Some(&rule),
            )
            .expect("a writer is answered");
            (status, body)
        }));
    }
    let mut written = Vec::new();
    for sender in senders {
        let (status, body) = sender.join().expect("a writer thread finished");
        assert_eq!(status, 200, "a write was refused: {status} {body}");
        written.push(json_string(&body, "id").expect("an id per rule"));
    }
    for reader in readers {
        assert_eq!(reader.join().expect("a reader thread finished"), 4);
    }

    let unique: std::collections::HashSet<&String> = written.iter().collect();
    assert_eq!(unique.len(), writers, "every write got its own record");

    let (status, body) = engine.request(
        "GET",
        "/rules?limit=500",
        &[
            ("Authorization", token.as_str()),
            ("X-User-Id", caller.as_str()),
            ("X-Book-ID", book.as_str()),
        ],
        None,
    );
    assert_eq!(status, 200, "listing answered {status}: {body}");
    for id in &written {
        assert!(body.contains(id.as_str()), "the Book lost {id}");
    }
}
