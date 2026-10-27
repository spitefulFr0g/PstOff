use super::*;
use std::io::Read;
#[path = "../../tests/support/mod.rs"]
mod support;
#[allow(dead_code)]
#[path = "../../tests/support/synthetic.rs"]
mod synthetic;

#[test]
fn source_payload_relocation_is_selective_ordered_and_cancellable() {
    let fixture = support::Fixture::invalid();
    let payload: Vec<_> = (0..180_013)
        .map(|i| ((i * 37 + i / 251) % 251) as u8)
        .collect();
    let mut item = synthetic::Item::new("IPM.Note", 1);
    item.properties
        .push(synthetic::Property::binary(0x6801, payload.clone()));
    let bytes = synthetic::Store {
        root: synthetic::Folder {
            name: "Generated".into(),
            items: vec![item],
            ..Default::default()
        },
    }
    .build();
    std::fs::write(&fixture.source, &bytes).unwrap();
    let mut source = Pst::from_file(File::open(&fixture.source).unwrap()).unwrap();
    let node = source
        .nodes()
        .into_iter()
        .find(|n| n.nid_type() == 4)
        .unwrap();
    let cancel = crate::CancelToken::default();
    let cells =
        context::relocation_properties(&mut source, node.bid_data, node.bid_sub, &cancel).unwrap();
    assert!(cells[&0x6801].bytes.is_empty());
    let local = cells[&0x6801].subnode.unwrap();
    let selected = context::selected_properties(
        &mut source,
        node.bid_data,
        node.bid_sub,
        &[0x68010102].into_iter().collect(),
        &cancel,
    )
    .unwrap();
    assert_eq!(selected[&0x6801].bytes, payload);
    assert_eq!(selected[&0x6801].subnode, Some(local));
    let bid = source.subnodes(node.bid_sub).unwrap()[&local].data;
    let mut output = Output {
        cancel: cancel.clone(),
        file: File::create(fixture.root.join("blocks.bin")).unwrap(),
        pos: AMAP + 2048,
        next: 1,
        blocks: vec![],
        nodes: vec![],
        differences: vec![],
        highwater: 1024,
        planned: Default::default(),
        omitted: Default::default(),
        created: Default::default(),
    };
    let root = output.source_stream(&mut source, bid, false).unwrap();
    let mut readback = Vec::new();
    let mut file = File::open(fixture.root.join("blocks.bin")).unwrap();
    for (id, entry) in &output.blocks {
        let at = u64::from_le_bytes(entry[8..16].try_into().unwrap());
        let len = u16::from_le_bytes(entry[16..18].try_into().unwrap()) as usize;
        file.seek(SeekFrom::Start(at)).unwrap();
        let mut bytes = vec![0; len];
        file.read_exact(&mut bytes).unwrap();
        if id & 2 == 0 {
            pstoff_format::crypt::permute_decode(&mut bytes);
            readback.extend_from_slice(&bytes);
        } else if *id == root {
            assert_eq!(bytes[1], 1);
            assert_eq!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
                payload.len()
            );
        }
    }
    assert_eq!(readback, payload);
    let count = output.blocks.len();
    let len = output.file.metadata().unwrap().len();
    cancel.cancel();
    assert_eq!(output.source_stream(&mut source, bid, false).unwrap(), 0);
    assert_eq!(output.blocks.len(), count);
    assert_eq!(output.file.metadata().unwrap().len(), len);
    assert_eq!(std::fs::read(&fixture.source).unwrap(), bytes);
}
