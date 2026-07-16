use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

pub const SOCK_ENV: &str = "RAVIX_ASKPASS_SOCK";

#[derive(Debug, Clone)]
pub struct AskpassConfig {
    pub helper: PathBuf,
    pub socket: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptKind {
    Username,
    Password,
}

#[derive(Default)]
pub struct CredentialCache {
    entries: std::collections::HashMap<(String, PromptKind), String>,
}

impl CredentialCache {
    pub fn get(&self, host: &str, kind: PromptKind) -> Option<&str> {
        self.entries
            .get(&(host.to_string(), kind))
            .map(String::as_str)
    }

    pub fn store(&mut self, host: &str, kind: PromptKind, value: String) {
        self.entries.insert((host.to_string(), kind), value);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskpassPrompt {
    pub kind: PromptKind,
    pub host: String,
}

pub fn parse_askpass_prompt(prompt: &str) -> AskpassPrompt {
    let kind = if prompt.contains("Username") {
        PromptKind::Username
    } else {
        PromptKind::Password
    };
    AskpassPrompt {
        kind,
        host: quoted_host(prompt).unwrap_or_default(),
    }
}

fn quoted_host(text: &str) -> Option<String> {
    let start = text.find('\'')?;
    let rest = &text[start + 1..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

pub fn request_credential(path: &Path, prompt: &str) -> io::Result<String> {
    let stream = UnixStream::connect(path)?;
    let mut writer = &stream;
    writer.write_all(prompt.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;

    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(strip_newline(&line).to_string())
}

pub fn handle_connection(
    stream: UnixStream,
    resolver: impl FnOnce(&str) -> String,
) -> io::Result<()> {
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let answer = resolver(strip_newline(&line));

    let mut writer = &stream;
    writer.write_all(answer.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn strip_newline(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

pub fn helper_prompt() -> Option<String> {
    std::env::var_os(SOCK_ENV)?;
    let mut args = std::env::args();
    let _binary = args.next();
    let prompt = args.next()?;
    if args.next().is_some() {
        return None;
    }
    Some(prompt)
}

pub fn run_helper(prompt: &str) -> io::Result<()> {
    let sock =
        std::env::var_os(SOCK_ENV).ok_or_else(|| io::Error::other("askpass socket not set"))?;
    let answer = request_credential(Path::new(&sock), prompt)?;
    println!("{answer}");
    Ok(())
}

pub struct AskpassRequest {
    prompt: String,
    reply: mpsc::Sender<String>,
}

impl AskpassRequest {
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn answer(self, value: String) {
        let _ = self.reply.send(value);
    }
}

pub struct AskpassServer {
    path: PathBuf,
    requests: mpsc::Receiver<AskpassRequest>,
}

impl AskpassServer {
    pub fn bind() -> io::Result<Self> {
        let path = socket_path();
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;

        let (requests_tx, requests) = mpsc::channel();
        let thread_path = path.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else {
                    break;
                };
                let requests_tx = requests_tx.clone();
                let _ = handle_connection(stream, move |prompt| {
                    let (reply_tx, reply_rx) = mpsc::channel();
                    let request = AskpassRequest {
                        prompt: prompt.to_string(),
                        reply: reply_tx,
                    };
                    if requests_tx.send(request).is_err() {
                        return String::new();
                    }
                    reply_rx.recv().unwrap_or_default()
                });
            }
            let _ = std::fs::remove_file(&thread_path);
        });

        Ok(Self { path, requests })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn into_requests(self) -> mpsc::Receiver<AskpassRequest> {
        self.requests
    }
}

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join(format!("ravix-askpass-{}.sock", std::process::id()))
}
