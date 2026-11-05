//! Starting and supervising the engine processes (adb, scrcpy, UxPlay).

use async_trait::async_trait;
use phonemirror_core::adb::{CmdOutput, CommandRunner, RunError};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;

/// A command that never flashes a console window on Windows.
pub fn command(program: &Path) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.stdin(Stdio::null()).kill_on_drop(true);
    c
}

/// Runs the bundled adb.
pub struct AdbRunner {
    pub adb: PathBuf,
}

#[async_trait]
impl CommandRunner for AdbRunner {
    async fn run(&self, args: &[String], timeout: Duration) -> Result<CmdOutput, RunError> {
        if self.adb.is_absolute() && !self.adb.exists() {
            return Err(RunError::NotFound(self.adb.display().to_string()));
        }
        let mut cmd = command(&self.adb);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => RunError::NotFound(self.adb.display().to_string()),
            _ => RunError::Other(e.to_string()),
        })?;
        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => Ok(CmdOutput {
                code: out.status.code(),
                stdout: out.stdout,
                stderr: out.stderr,
            }),
            Ok(Err(e)) => Err(RunError::Other(e.to_string())),
            Err(_) => Err(RunError::TimedOut),
        }
    }
}

/// The last lines an engine printed, for "Copy diagnostics".
#[derive(Default)]
pub struct LogTail {
    lines: Mutex<VecDeque<String>>,
}

impl LogTail {
    const MAX: usize = 300;
    pub fn push(&self, line: &str) {
        let mut l = self.lines.lock().unwrap();
        if l.len() == Self::MAX {
            l.pop_front();
        }
        l.push_back(line.to_string());
    }
    pub fn dump(&self) -> String {
        self.lines
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// How to ask a process to quit before killing it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StopStyle {
    /// Close its window (scrcpy finishes the MP4 file on close).
    CloseWindow,
    /// Just end it.
    Kill,
}

/// A supervised child. Dropping it stops the process.
pub struct Supervised {
    pub pid: u32,
    stop_tx: Option<oneshot::Sender<()>>,
}

impl Supervised {
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for Supervised {
    fn drop(&mut self) {
        self.stop();
    }
}

pub type LineFn = Arc<dyn Fn(String) + Send + Sync>;
/// Called once with the exit code and whether we asked it to stop.
pub type ExitFn = Box<dyn FnOnce(Option<i32>, bool) + Send>;

fn read_lines<R: AsyncRead + Unpin + Send + 'static>(r: R, on_line: LineFn) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(r);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf);
                    // UxPlay draws progress with '\r'; keep the last segment.
                    let line = line
                        .trim_end_matches(['\r', '\n'])
                        .rsplit('\r')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if !line.trim().is_empty() {
                        on_line(line);
                    }
                }
            }
        }
    });
}

pub fn spawn_supervised(
    mut cmd: Command,
    style: StopStyle,
    on_line: LineFn,
    on_exit: ExitFn,
) -> std::io::Result<Supervised> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let pid = child.id().unwrap_or(0);
    if let Some(out) = child.stdout.take() {
        read_lines(out, on_line.clone());
    }
    if let Some(err) = child.stderr.take() {
        read_lines(err, on_line);
    }
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let mut stopped = false;
        let status = tokio::select! {
            s = child.wait() => s.ok(),
            _ = stop_rx => {
                stopped = true;
                if style == StopStyle::CloseWindow {
                    crate::win::request_close(pid);
                    match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                        Ok(s) => s.ok(),
                        Err(_) => { let _ = child.kill().await; None }
                    }
                } else {
                    let _ = child.kill().await;
                    None
                }
            }
        };
        on_exit(status.and_then(|s| s.code()), stopped);
    });
    Ok(Supervised {
        pid,
        stop_tx: Some(stop_tx),
    })
}
