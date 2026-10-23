// EXPLORATORY SPIKE. Hardcoded sandbox paths; public fixtures only.
use outlook_pst::{
    ndb::{header::Header, root::Root},
    PstFile, UnicodePstFile,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = "/input/store";
    let output = std::env::args().nth(2).expect("output");
    assert!(!std::path::Path::new(&output).exists());
    std::fs::copy(input, &output)?;
    {
        let mut pst = UnicodePstFile::open(&output)?;
        println!("before_amap={:?}", pst.header().root().amap_is_valid());
        let mut transaction = pst.lock()?;
        transaction.flush()?;
    }
    let pst = UnicodePstFile::open(&output)?;
    println!("after_amap={:?}", pst.header().root().amap_is_valid());
    Ok(())
}
