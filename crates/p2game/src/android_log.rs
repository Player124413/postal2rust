//! Android: the game's logs go to text files in `<data dir>/logs/`, so a crash can be sent back.
//!
//! - `game.txt`: everything the game prints to stdout and stderr (`println!`, `eprintln!`, the
//!   text of a panic). Android does not show these in logcat, so stdout and stderr are redirected
//!   into a pipe that is copied to this file.
//! - `logcat.txt`: the lines of this process from logcat (Bevy, wgpu, tracing, native crashes).
//!   It is written by a `logcat` process that the app starts and that keeps running after the app
//!   exits, so the native crash report is included too.
//! - `crash.txt`: panic messages with a backtrace.
//!
//! Each start moves the previous files to `*-previous.txt`, so the run that crashed is kept while
//! the next run starts. The launcher sends these files together (`GameLogs.kt`).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// `game.txt` stops growing here, so a long session cannot fill the storage.
const MAX_GAME_BYTES: u64 = 32 << 20;

unsafe extern "C" {
    fn pipe(fds: *mut i32) -> i32;
    fn dup2(old: i32, new: i32) -> i32;
    fn close(fd: i32) -> i32;
}

/// Starts the log files. Call once, first thing in `android_main`. Returns the logs folder.
pub fn start(data_dir: &Path) -> PathBuf {
    let dir = data_dir.join("logs");
    let _ = fs::create_dir_all(&dir);
    for name in ["game", "logcat", "crash"] {
        let current = dir.join(format!("{name}.txt"));
        if current.is_file() {
            let _ = fs::rename(&current, dir.join(format!("{name}-previous.txt")));
        }
    }
    let header = format!(
        "postal2rust {} (pid {}), started at unix time {}\n",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        unix_time()
    );
    install_panic_hook(dir.join("crash.txt"), header.clone());
    start_logcat(dir.join("logcat.txt"));
    capture_stdio(dir.join("game.txt"), header);
    dir
}

fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn install_panic_hook(path: PathBuf, header: String) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(header.as_bytes());
            let _ = writeln!(f, "PANIC: {info}\n{backtrace}\n");
        }
        previous(info);
    }));
}

fn start_logcat(path: PathBuf) {
    let pid = std::process::id().to_string();
    for program in ["/system/bin/logcat", "logcat"] {
        let started = std::process::Command::new(program)
            .args(["-v", "threadtime", "--pid", &pid, "-f"])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        // The child is not waited for: it ends by itself when the logcat service goes away.
        if started.is_ok() {
            return;
        }
    }
}

/// Points fd 1 (stdout) and fd 2 (stderr) at a pipe and copies the pipe to `path`.
fn capture_stdio(path: PathBuf, header: String) {
    let mut fds = [0i32; 2];
    // SAFETY: `fds` has room for the two descriptors `pipe` writes; dup2/close act on our own fds.
    unsafe {
        if pipe(fds.as_mut_ptr()) != 0 {
            return;
        }
        dup2(fds[1], 1);
        dup2(fds[1], 2);
        close(fds[1]);
    }
    // SAFETY: fds[0] is the read end of the pipe created above and is owned by nothing else.
    let mut reader = unsafe { File::from_raw_fd(fds[0]) };
    let spawned = std::thread::Builder::new().name("stdio-log".into()).spawn(move || {
        let mut out = OpenOptions::new().create(true).write(true).truncate(true).open(&path).ok();
        if let Some(f) = out.as_mut() {
            let _ = f.write_all(header.as_bytes());
        }
        let mut written: u64 = 0;
        let mut buf = [0u8; 8192];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if let Some(f) = out.as_mut() {
                if written < MAX_GAME_BYTES {
                    let take = n.min((MAX_GAME_BYTES - written) as usize);
                    let _ = f.write_all(&buf[..take]);
                    written += take as u64;
                    if written >= MAX_GAME_BYTES {
                        let _ = writeln!(f, "\n[log limit reached: the rest of the output is not written]");
                    }
                }
                // Flushed on every read, so the last lines before a crash are on disk.
                let _ = f.flush();
            }
        }
    });
    let _ = spawned;
}
