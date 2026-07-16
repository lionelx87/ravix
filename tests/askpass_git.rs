use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;

use ravix::askpass::AskpassServer;

#[test]
fn git_pulls_the_password_from_the_rx_askpass_helper() {
    let server = AskpassServer::bind().unwrap();
    let socket = server.path().to_path_buf();
    let requests = server.into_requests();

    let responder = thread::spawn(move || {
        let request = requests.recv().unwrap();
        assert!(request.prompt().contains("Password"));
        request.answer("s3cr3t".to_string());
    });

    let mut child = Command::new("git")
        .args(["-c", "credential.helper=", "credential", "fill"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", env!("CARGO_BIN_EXE_rx"))
        .env("RAVIX_ASKPASS_SOCK", &socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"protocol=https\nhost=dev.azure.com\nusername=user\n\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    let filled = String::from_utf8_lossy(&output.stdout);

    responder.join().unwrap();
    assert!(
        filled.contains("password=s3cr3t"),
        "git should have received the password from our askpass helper, got: {filled}"
    );
}
