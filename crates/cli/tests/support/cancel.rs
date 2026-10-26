//! The driver owns a separate console so a real Ctrl+C cannot reach Cargo,
//! the user's console, or concurrently running tests. No private inputs.
use super::{status, Fixture, Item};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[link(name = "Kernel32")]
extern "system" {
    fn FreeConsole() -> i32;
    fn AllocConsole() -> i32;
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
    fn GenerateConsoleCtrlEvent(event: u32, group: u32) -> i32;
}

unsafe extern "system" fn keep_driver_alive(_event: u32) -> i32 {
    1
}

fn wait_bounded(child: &mut std::process::Child) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            return;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("console cancellation smoke timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn ctrl_c_cleans_staging() {
    let mut driver = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "cancel::console_driver", "--nocapture"])
        .env("PSTOFF_CLI_CANCEL_DRIVER", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_bounded(&mut driver);
    status(&driver.wait_with_output().unwrap(), 0);
}

#[test]
fn console_driver() {
    if std::env::var_os("PSTOFF_CLI_CANCEL_DRIVER").is_none() {
        return;
    }
    // SAFETY: only this isolated helper process detaches from its inherited
    // console. All subsequent control events target its new console alone.
    unsafe {
        FreeConsole();
        assert_ne!(AllocConsole(), 0, "{}", std::io::Error::last_os_error());
        assert_ne!(SetConsoleCtrlHandler(Some(keep_driver_alive), 1), 0);
    }
    let f = Fixture::new((0..100).map(|_| Item::new("IPM.Note", 42)).collect());
    let mut child = f
        .command()
        .args(["--split-limit", "off"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let staging = f.out.join(".synthetic.pstoff-partial");
    let reader = std::thread::spawn(move || {
        let mut interrupted = false;
        let mut log = String::new();
        for line in BufReader::new(stderr).lines() {
            let line = line.unwrap();
            if !interrupted && line.starts_with("Progress:") {
                assert!(staging.is_dir(), "cancel must exercise active staging");
                // SAFETY: event 0 is CTRL_C_EVENT, group 0 broadcasts only to
                // processes on the driver's isolated console (driver + CLI).
                assert_ne!(unsafe { GenerateConsoleCtrlEvent(0, 0) }, 0);
                interrupted = true;
            }
            log.push_str(&line);
            log.push('\n');
        }
        assert!(interrupted, "conversion never reported progress: {log}");
        log
    });
    wait_bounded(&mut child);
    assert_eq!(child.wait().unwrap().code(), Some(130));
    let log = reader.join().unwrap();
    assert!(
        log.contains("Conversion cancelled; unfinished output removed."),
        "{log}"
    );
    assert_eq!(std::fs::read_dir(&f.out).unwrap().count(), 0);
}
