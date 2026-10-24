use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Fixture {
    pub root: PathBuf,
    pub source: PathBuf,
}
impl Fixture {
    pub fn invalid() -> Self {
        let root = std::env::temp_dir().join(format!(
            "pstoff-generated-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let source = root.join("synthetic.ost");
        std::fs::write(&source, [0; 512]).unwrap();
        Self { root, source }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
