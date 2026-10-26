use pstoff_engine::CancelToken;

#[cfg(windows)]
mod windows {
    use super::CancelToken;
    use std::sync::OnceLock;

    static CANCEL: OnceLock<CancelToken> = OnceLock::new();

    #[link(name = "Kernel32")]
    extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }

    // Windows invokes this on a dedicated thread. Ctrl+C and targeted
    // CTRL_BREAK_EVENT both use the same cooperative cancellation path.
    unsafe extern "system" fn handler(event: u32) -> i32 {
        if event <= 1 {
            if let Some(token) = CANCEL.get() {
                token.cancel();
            }
            return 1;
        }
        0
    }

    pub fn install(token: CancelToken) -> std::io::Result<()> {
        CANCEL.set(token).expect("one conversion per CLI process");
        // SAFETY: callback uses the Windows ABI and both callback and token
        // remain valid for the entire process lifetime.
        if unsafe { SetConsoleCtrlHandler(Some(handler), 1) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use windows::install;

#[cfg(not(windows))]
pub fn install(_token: CancelToken) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "PstOff CLI conversion requires Windows.",
    ))
}
