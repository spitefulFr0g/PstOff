use pstoff_engine::platform::{classify_windows_io, require_space, IoFailure};

#[test]
fn windows_failures_preserve_distinct_outcomes() {
    assert_eq!(classify_windows_io(32), IoFailure::InUse);
    assert_eq!(classify_windows_io(33), IoFailure::InUse);
    assert_eq!(classify_windows_io(112), IoFailure::InsufficientSpace);
    assert_eq!(classify_windows_io(39), IoFailure::InsufficientSpace);
    for code in [0, 2, 5, 87] {
        assert_eq!(classify_windows_io(code), IoFailure::Other(code));
    }
}

#[test]
fn space_preflight_uses_caller_available_bytes_and_checks_overflow() {
    assert!(require_space(100, 90, 10).is_ok());
    assert!(require_space(99, 90, 10).is_err());
    assert!(require_space(u64::MAX, u64::MAX, 1).is_err());
    assert!(require_space(0, 0, 0).is_ok());
}
