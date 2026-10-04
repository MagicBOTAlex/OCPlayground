//! End-to-end tests: run the compiled `ocplay` binary against temporary
//! computers and assert on its output.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "ocplay-test-{}-{}-{}-{}",
        tag,
        std::process::id(),
        nanos,
        COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A minimal config that boots the built-in OpenOS and never waits on exit.
fn base_config(extra_mount: Option<&Path>) -> String {
    let mut config = String::from(
        "name: test\n\
         memory: 512\n\
         cpu:\n  tier: 2\n\
         gpu:\n  tier: 3\n  screen:\n    width: 80\n    height: 25\n    maxDepth: 8\n\
         filesystems:\n\
         \x20 - label: openos\n    path: builtin\n    mount: /\n    readonly: false\n",
    );
    if let Some(path) = extra_mount {
        config.push_str(&format!(
            "  - label: out\n    path: {}\n    mount: /out\n    readonly: false\n",
            path.display()
        ));
    }
    config.push_str(
        "components:\n  keyboard: true\n  mouse: false\n\
         internet:\n  enabled: true\n  timeout: 30\n\
         run:\n  timeout: 0\n  interactive: false\n  terminateDelay: 0\n",
    );
    config
}

fn run_ocplay(config: &Path, script: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ocplay"));
    command.arg(config);
    if let Some(script) = script {
        command.arg(script);
    }
    command.output().expect("failed to run ocplay")
}

#[test]
fn boots_openos_and_runs_a_script() {
    let dir = temp_dir("run");
    let config = dir.join("computer.yaml");
    fs::write(&config, base_config(None)).unwrap();
    let script = dir.join("script.lua");
    fs::write(
        &script,
        r#"require("term").clear()
print("OCPLAY_MARKER_OK")
require("computer").shutdown()"#,
    )
    .unwrap();

    let output = run_ocplay(&config, Some(&script));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(stdout.contains("OCPLAY_MARKER_OK"), "stdout: {stdout}");
}

#[test]
fn internet_get_returns_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0u8; 1024];
        let _ = socket.read(&mut request);
        let body = "hello-from-test";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).unwrap();
    });

    let dir = temp_dir("internet");
    let config = dir.join("computer.yaml");
    fs::write(&config, base_config(None)).unwrap();
    let script = dir.join("script.lua");
    fs::write(
        &script,
        format!(
            r#"local internet = require("internet")
local chunks = {{}}
for chunk in internet.request("http://{addr}/") do
  chunks[#chunks + 1] = chunk
end
print("BODY:" .. table.concat(chunks))
require("computer").shutdown()"#
        ),
    )
    .unwrap();

    let output = run_ocplay(&config, Some(&script));
    server.join().unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("BODY:hello-from-test"), "stdout: {stdout}");
}

#[test]
fn missing_root_filesystem_fails() {
    let dir = temp_dir("noroot");
    let config = dir.join("computer.yaml");
    fs::write(
        &config,
        "filesystems:\n  - label: home\n    path: builtin\n    mount: /home\n    readonly: false\n\
         run:\n  terminateDelay: 0\n",
    )
    .unwrap();

    let output = run_ocplay(&config, None);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("no root filesystem"), "stderr: {stderr}");
}
