use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use crate::error::DockerError;
use crate::transport::Transport;

const KILL_WAIT_ATTEMPTS: u32 = 10;
const KILL_WAIT_SLEEP: Duration = Duration::from_millis(20);
const TAIL_LINES: &str = "200";

/// Which pipe a log line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

/// Best-effort level parsed from the message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Fatal,
    Error,
    Warn,
    Info,
    Debug,
    None,
}

/// A container that should have `docker logs -f` attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogTarget {
    pub id: String,
    pub name: String,
}

/// One line from `docker logs --timestamps`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub container_id: String,
    pub container_name: String,
    pub timestamp: String,
    pub message: String,
    pub stream: LogStream,
    pub level: LogLevel,
    pub received_at: Instant,
}

impl LogLine {
    pub fn diagnostic(container_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            container_id: String::new(),
            container_name: container_name.into(),
            timestamp: String::new(),
            message: message.into(),
            stream: LogStream::Stderr,
            level: LogLevel::Error,
            received_at: Instant::now(),
        }
    }

    pub fn format_line(&self, show_timestamp: bool) -> String {
        let mut out = String::from("[");
        out.push_str(&self.container_name);
        out.push_str("] ");
        if show_timestamp && !self.timestamp.is_empty() {
            out.push_str(&self.timestamp);
            out.push(' ');
        }
        out.push_str(&self.message);
        out
    }

    /// Error / fatal / panic / oom / unhealthy lines for the Log Errors panel.
    pub fn is_error_line(&self) -> bool {
        if matches!(self.level, LogLevel::Fatal | LogLevel::Error) {
            return true;
        }
        let lower = self.message.to_ascii_lowercase();
        lower.contains("panic")
            || lower.contains("oom")
            || lower.contains("unhealthy")
            || lower.contains("fatal")
    }
}

/// Multiplexes `docker logs -f` for the current running containers into one channel.
pub struct LogsMux {
    transport: Transport,
    tx: Sender<LogLine>,
    streams: HashMap<String, LogFollow>,
}

impl LogsMux {
    pub fn spawn(transport: Transport) -> (Receiver<LogLine>, Self) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (
            rx,
            Self {
                transport,
                tx,
                streams: HashMap::new(),
            },
        )
    }

    /// Start or stop follow processes so they match `targets`.
    pub fn sync(&mut self, targets: &[LogTarget]) {
        let wanted: HashSet<&str> = targets.iter().map(|target| target.id.as_str()).collect();

        self.streams.retain(|id, follow| {
            let keep = wanted.contains(id.as_str()) && !follow.is_finished();
            if !keep {
                follow.shutdown();
            }
            keep
        });

        for target in targets {
            if self.streams.contains_key(&target.id) {
                continue;
            }
            match spawn_follow(&self.transport, target, self.tx.clone()) {
                Ok(follow) => {
                    self.streams.insert(target.id.clone(), follow);
                }
                Err(err) => {
                    let _ = self.tx.send(LogLine::diagnostic(
                        target.name.clone(),
                        format!("docker logs error: {}", err.user_message()),
                    ));
                }
            }
        }
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        for (_, mut follow) in self.streams.drain() {
            follow.shutdown();
        }
    }
}

impl Drop for LogsMux {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct LogFollow {
    child: Arc<Mutex<Child>>,
    join_handle: Option<JoinHandle<()>>,
}

impl LogFollow {
    fn is_finished(&self) -> bool {
        self.join_handle
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
    }

    fn shutdown(&mut self) {
        kill_child(&self.child);
        let _ = self.join_handle.take();
    }
}

fn spawn_follow(
    transport: &Transport,
    target: &LogTarget,
    tx: Sender<LogLine>,
) -> Result<LogFollow, DockerError> {
    let child = spawn_logs_child(transport, &target.id)?;
    let child = Arc::new(Mutex::new(child));
    let reader_child = child.clone();
    let container_id = target.id.clone();
    let container_name = target.name.clone();

    let join_handle = thread::spawn(move || {
        stream_logs(reader_child.clone(), tx, container_id, container_name);
        kill_child(&reader_child);
    });

    Ok(LogFollow {
        child,
        join_handle: Some(join_handle),
    })
}

fn spawn_logs_child(transport: &Transport, container_id: &str) -> Result<Child, DockerError> {
    transport
        .command(&[
            "logs",
            "--tail",
            TAIL_LINES,
            "-f",
            "--timestamps",
            container_id,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                DockerError::NotFound
            } else {
                DockerError::Io(err)
            }
        })
}

fn stream_logs(
    child: Arc<Mutex<Child>>,
    tx: Sender<LogLine>,
    container_id: String,
    container_name: String,
) {
    let stderr = {
        let mut guard = match child.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        guard.stderr.take()
    };
    if let Some(stderr) = stderr {
        let stderr_tx = tx.clone();
        let stderr_id = container_id.clone();
        let stderr_name = container_name.clone();
        thread::spawn(move || {
            read_log_pipe(stderr, stderr_tx, stderr_id, stderr_name, LogStream::Stderr);
        });
    }

    let stdout = {
        let mut guard = match child.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        guard.stdout.take()
    };
    if let Some(stdout) = stdout {
        read_log_pipe(stdout, tx, container_id, container_name, LogStream::Stdout);
    }
}

fn read_log_pipe(
    pipe: impl std::io::Read,
    tx: Sender<LogLine>,
    container_id: String,
    container_name: String,
    stream: LogStream,
) {
    let mut reader = BufReader::new(pipe);
    let mut line_buffer = Vec::new();
    loop {
        let line = match read_line_lossy(&mut reader, &mut line_buffer) {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(err) => {
                let _ = tx.send(LogLine::diagnostic(
                    container_name.clone(),
                    format!("docker logs read error: {err}"),
                ));
                break;
            }
        };

        if line.is_empty() {
            continue;
        }

        if tx
            .send(parse_log_line(
                &container_id,
                &container_name,
                stream,
                &line,
            ))
            .is_err()
        {
            break;
        }
    }
}

fn kill_child(child: &Arc<Mutex<Child>>) {
    if let Ok(mut guard) = child.lock() {
        let _ = guard.kill();
        for _ in 0..KILL_WAIT_ATTEMPTS {
            if guard.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(KILL_WAIT_SLEEP);
        }
        let _ = guard.wait();
    }
}

/// Read one line as UTF-8 lossy text. Unlike [`BufRead::lines`], invalid bytes do not
/// terminate the stream.
fn read_line_lossy(
    reader: &mut impl BufRead,
    buffer: &mut Vec<u8>,
) -> std::io::Result<Option<String>> {
    buffer.clear();
    let bytes_read = reader.read_until(b'\n', buffer)?;
    if bytes_read == 0 {
        return Ok(None);
    }

    while matches!(buffer.last(), Some(b'\n') | Some(b'\r')) {
        buffer.pop();
    }

    Ok(Some(String::from_utf8_lossy(buffer).into_owned()))
}

pub(crate) fn parse_log_line(
    container_id: &str,
    container_name: &str,
    stream: LogStream,
    line: &str,
) -> LogLine {
    let (timestamp, message) = split_docker_timestamp(line);
    let level = detect_level(message);
    LogLine {
        container_id: container_id.to_string(),
        container_name: container_name.to_string(),
        timestamp: timestamp.to_string(),
        message: message.to_string(),
        stream,
        level,
        received_at: Instant::now(),
    }
}

fn split_docker_timestamp(line: &str) -> (&str, &str) {
    let Some((timestamp, rest)) = line.split_once(' ') else {
        return ("", line);
    };
    if is_rfc3339(timestamp) {
        (timestamp, rest)
    } else {
        ("", line)
    }
}

fn is_rfc3339(timestamp: &str) -> bool {
    let bytes = timestamp.as_bytes();
    if bytes.len() < 20 || bytes.get(4) != Some(&b'-') || bytes.get(10) != Some(&b'T') {
        return false;
    }
    timestamp.ends_with('Z') || timestamp.contains('+') || timestamp[11..].contains('-')
}

fn detect_level(message: &str) -> LogLevel {
    let trimmed = message.trim();
    let token = trimmed.split([' ', ':', '\t']).next().unwrap_or("");

    match token.to_ascii_uppercase().as_str() {
        "FATAL" => return LogLevel::Fatal,
        "ERROR" | "ERR" => return LogLevel::Error,
        "WARN" | "WARNING" => return LogLevel::Warn,
        "INFO" => return LogLevel::Info,
        "DEBUG" => return LogLevel::Debug,
        _ => {}
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("fatal") || lower.contains("panic") {
        LogLevel::Fatal
    } else if lower.contains("unhealthy") || lower.contains("oom") || lower.contains("error") {
        LogLevel::Error
    } else if lower.contains("warn") {
        LogLevel::Warn
    } else {
        LogLevel::None
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_level, parse_log_line, LogLevel, LogLine, LogStream};

    #[test]
    fn splits_docker_timestamp_prefix() {
        let line = parse_log_line(
            "abc123",
            "api",
            LogStream::Stdout,
            "2026-09-08T07:10:15.123456789Z INFO api handled request id=1",
        );
        assert_eq!(line.timestamp, "2026-09-08T07:10:15.123456789Z");
        assert_eq!(line.message, "INFO api handled request id=1");
        assert_eq!(line.level, LogLevel::Info);
        assert_eq!(line.container_name, "api");
    }

    #[test]
    fn keeps_unprefixed_line_as_message() {
        let line = parse_log_line("id", "db", LogStream::Stdout, "not a timestamped line");
        assert!(line.timestamp.is_empty());
        assert_eq!(line.message, "not a timestamped line");
    }

    #[test]
    fn colors_error_warn_info_from_leading_token() {
        assert_eq!(
            detect_level("ERROR api failed to reach db"),
            LogLevel::Error
        );
        assert_eq!(detect_level("WARN api slow response"), LogLevel::Warn);
        assert_eq!(detect_level("INFO api listening"), LogLevel::Info);
        assert_eq!(detect_level("FATAL worker exiting"), LogLevel::Fatal);
    }

    #[test]
    fn error_panel_keeps_crash_and_unhealthy_lines() {
        let crash = parse_log_line(
            "w",
            "worker",
            LogStream::Stdout,
            "2026-09-08T07:10:15Z ERROR worker panic: connection refused to db",
        );
        let fatal = parse_log_line("w", "worker", LogStream::Stdout, "FATAL worker exiting");
        let unhealthy = parse_log_line(
            "u",
            "unhealthy",
            LogStream::Stdout,
            "ERROR unhealthy healthcheck failing",
        );
        let info = parse_log_line("a", "api", LogStream::Stdout, "INFO api handled request");
        let warn = parse_log_line("a", "api", LogStream::Stdout, "WARN api slow response");

        assert!(crash.is_error_line());
        assert!(fatal.is_error_line());
        assert!(unhealthy.is_error_line());
        assert!(!info.is_error_line());
        assert!(!warn.is_error_line());
    }

    #[test]
    fn format_line_omits_timestamp_when_hidden() {
        let line = LogLine {
            container_id: "id".into(),
            container_name: "api".into(),
            timestamp: "2026-09-08T07:10:15Z".into(),
            message: "INFO hello".into(),
            stream: LogStream::Stdout,
            level: LogLevel::Info,
            received_at: std::time::Instant::now(),
        };
        assert_eq!(
            line.format_line(true),
            "[api] 2026-09-08T07:10:15Z INFO hello"
        );
        assert_eq!(line.format_line(false), "[api] INFO hello");
    }
}
