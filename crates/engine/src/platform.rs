//! Platform-neutral boundary for host I/O adapters. These helpers do not open files.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoFailure {
    InUse,
    InsufficientSpace,
    Other(u32),
}

/// Classify a Win32 last-error value, never a Linux errno or an HRESULT.
pub fn classify_windows_io(code: u32) -> IoFailure {
    match code {
        32 | 33 => IoFailure::InUse,
        39 | 112 => IoFailure::InsufficientSpace,
        code => IoFailure::Other(code),
    }
}

/// Host adapters return bytes available to this caller (including quota limits).
/// A preflight is advisory: writes still need to handle disk-full errors.
pub trait AvailableSpace {
    fn available_bytes(&self, directory: &std::path::Path) -> std::io::Result<u64>;
}

/// Include the writer's estimated overhead without overflowing the size budget.
pub fn require_space(available: u64, payload: u64, overhead: u64) -> Result<(), IoFailure> {
    match payload.checked_add(overhead) {
        Some(required) if required <= available => Ok(()),
        _ => Err(IoFailure::InsufficientSpace),
    }
}
