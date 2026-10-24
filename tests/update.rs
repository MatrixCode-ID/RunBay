//! Update tests against a local HTTP server fixture.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use runbay::update::{self, UpdateState};

const EXE: &[u8] = b"pretend this is RunBay.exe 1.0.0-alpha.2";

struct Fixture {
    base: String,
    /// Paths of the requests, with "(etag)" when If-None-Match was sent.
    requests: Arc<Mutex<Vec<String>>>,
}

fn sha256(bytes: &[u8]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f");
    std::fs::write(&path, bytes).unwrap();
    update::sha256_file(&path).unwrap()
}

impl Fixture {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        let host = base.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let path = first.split_whitespace().nth(1).unwrap_or("").to_string();
                let mut etag = false;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    if line
                        .to_ascii_lowercase()
                        .starts_with("if-none-match: \"v1\"")
                    {
                        etag = true;
                    }
                }
                log.lock()
                    .unwrap()
                    .push(format!("{path}{}", if etag { " (etag)" } else { "" }));
                let releases = format!(
                    r#"[
                      {{"tag_name":"v1.0.0-alpha.2","draft":false,"html_url":"{host}/r2","assets":[
                        {{"name":"RunBay.exe","browser_download_url":"{host}/dl/RunBay.exe"}},
                        {{"name":"SHA256SUMS","browser_download_url":"{host}/dl/SHA256SUMS"}}]}},
                      {{"tag_name":"v1.0.0-beta.1","draft":true,"html_url":"{host}/rb","assets":[]}},
                      {{"tag_name":"v1.0.0-alpha.3","draft":false,"html_url":"{host}/r3","assets":[
                        {{"name":"RunBay.exe","browser_download_url":"{host}/dl/RunBay.exe"}},
                        {{"name":"SHA256SUMS","browser_download_url":"{host}/bad/SHA256SUMS"}}]}},
                      {{"tag_name":"v1.0.0-alpha.1","draft":false,"html_url":"{host}/r1","assets":[]}}
                    ]"#
                );
                let (status, headers, body): (&str, String, Vec<u8>) = if path.contains("/releases")
                {
                    if etag {
                        ("304 Not Modified", "ETag: \"v1\"\r\n".into(), Vec::new())
                    } else {
                        ("200 OK", "ETag: \"v1\"\r\n".into(), releases.into_bytes())
                    }
                } else if path == "/dl/RunBay.exe" {
                    ("200 OK", String::new(), EXE.to_vec())
                } else if path == "/dl/SHA256SUMS" {
                    (
                        "200 OK",
                        String::new(),
                        format!("{} *RunBay.exe", sha256(EXE)).into_bytes(),
                    )
                } else if path == "/bad/SHA256SUMS" {
                    (
                        "200 OK",
                        String::new(),
                        format!("{} *RunBay.exe", "0".repeat(64)).into_bytes(),
                    )
                } else {
                    ("404 Not Found", String::new(), Vec::new())
                };
                let mut out = stream;
                let head = format!(
                    "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = out.write_all(head.as_bytes());
                let _ = out.write_all(&body);
            }
        });
        Self { base, requests }
    }
}

#[test]
fn check_download_verify_and_etag() {
    let fixture = Fixture::start();
    let mut state = UpdateState::default();
    // The highest version (alpha.3) is offered first.
    let offered = update::check(&fixture.base, &mut state, "1.0.0-alpha.1")
        .unwrap()
        .unwrap();
    assert_eq!(offered.version, "1.0.0-alpha.3");
    assert_eq!(state.etag.as_deref(), Some("\"v1\""));

    // Its checksum does not match: the download is discarded.
    let dir = tempfile::tempdir().unwrap();
    let err = update::download(&offered, dir.path()).unwrap_err();
    assert!(format!("{err:#}").contains("checksum"), "{err:#}");
    assert!(!dir.path().join("RunBay-1.0.0-alpha.3.exe").exists());

    // Skipped: nothing is offered until a newer version than alpha.3 appears, and the
    // second check uses the ETag (304) with the cached list.
    state.skipped = Some("1.0.0-alpha.3".into());
    assert!(
        update::check(&fixture.base, &mut state, "1.0.0-alpha.1")
            .unwrap()
            .is_none()
    );
    let offered = state
        .releases
        .iter()
        .find(|r| r.version == "1.0.0-alpha.2")
        .unwrap()
        .clone();
    assert!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.contains("/releases") && r.ends_with("(etag)"))
    );
    let exe = update::download(&offered, dir.path()).unwrap();
    assert_eq!(std::fs::read(&exe).unwrap(), EXE);

    // Already on the newest version: nothing to offer.
    assert!(
        update::check(&fixture.base, &mut state, "1.0.0-alpha.3")
            .unwrap()
            .is_none()
    );

    // The state survives a save and load.
    state.save(dir.path()).unwrap();
    assert_eq!(UpdateState::load(dir.path()), state);
}

#[test]
fn unreachable_server_is_an_error() {
    let mut state = UpdateState::default();
    assert!(update::check("http://127.0.0.1:9", &mut state, "1.0.0-alpha.1").is_err());
}
