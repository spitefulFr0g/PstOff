//! Thread-local allocation high-water mark for generated table regressions.
//! This is neither a reservation counter nor a Windows private-commit measure.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {
    static LIVE: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}
struct Tracked;
fn change(bytes: isize) {
    let _ = LIVE.try_with(|live| {
        let value = live.get() + bytes;
        live.set(value);
        let _ = PEAK.try_with(|peak| peak.set(peak.get().max(value)));
    });
}
// SAFETY: every operation delegates unchanged to System. The counters retain
// no pointers, allocate nothing and tolerate TLS teardown and other test threads.
unsafe impl GlobalAlloc for Tracked {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            change(layout.size() as isize);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        change(-(layout.size() as isize));
        unsafe { System.dealloc(ptr, layout) };
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(ptr, layout, size) };
        if !next.is_null() {
            change(size as isize - layout.size() as isize);
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: Tracked = Tracked;
pub(in crate::writer) fn measure<T>(run: impl FnOnce() -> T) -> (T, usize) {
    let baseline = LIVE.with(Cell::get);
    PEAK.with(|peak| peak.set(baseline));
    let value = run();
    let peak = PEAK.with(Cell::get);
    (value, peak.saturating_sub(baseline) as usize)
}
