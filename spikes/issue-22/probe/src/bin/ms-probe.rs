use outlook_pst::{
    ndb::{header::Header, root::Root},
    PstFile, UnicodePstFile,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("input required")?;
    let pst = UnicodePstFile::open(&path)?;
    println!(
        "version={:?} amap_valid={:?}",
        pst.header().version(),
        pst.header().root().amap_is_valid()
    );
    let store = outlook_pst::open_store(&path)?;
    println!("store_property_count={}", store.properties().iter().count());
    let root = store.root_hierarchy_table()?;
    let mut rows = 0;
    let mut values = 0;
    for row in root.rows_matrix() {
        rows += 1;
        for (column, value) in root
            .context()
            .columns()
            .iter()
            .zip(row.columns(root.context())?)
        {
            if let Some(value) = value {
                root.read_column(&value, column.prop_type())?;
                values += 1;
            }
        }
    }
    println!("root_rows={rows} decoded_root_values={values}");
    let entry = store.properties().ipm_sub_tree_entry_id()?;
    println!(
        "ipm_key_matches={}",
        store.properties().matches_record_key(&entry)?
    );
    let folder = store.open_folder(&entry)?;
    println!(
        "ipm_opened=true hierarchy_present={}",
        folder.hierarchy_table().is_some()
    );
    Ok(())
}
