// EXPLORATORY SPIKE. Hardcoded sandbox paths; public fixtures only.
use outlook_pst::{PstFile, UnicodePstFile, ndb::{header::Header, root::Root}};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = "/input/baseline.pst";
    let output = "/output/hybrid-map-rebuild.pst";
    std::fs::copy(input, output)?;
    {
        let mut pst = UnicodePstFile::open(output)?;
        println!("before_amap={:?}", pst.header().root().amap_is_valid());
        let mut transaction = pst.lock()?;
        transaction.flush()?;
    }
    let pst = UnicodePstFile::open(output)?;
    println!("after_amap={:?}", pst.header().root().amap_is_valid());
    Ok(())
}
