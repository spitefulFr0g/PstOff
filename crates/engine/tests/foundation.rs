mod support;
use pstoff_engine::{
    convert, inspect, CancelToken, Conversion, ConvertOptions, ErrorKind, Split, Support,
    UnsupportedReason, DEFAULT_SPLIT_BYTES,
};
#[test]
fn rejected_generated_input_is_unchanged_and_publishes_nothing() {
    let fixture = support::Fixture::invalid();
    let before = std::fs::read(&fixture.source).unwrap();
    assert!(matches!(
        inspect(&fixture.source).unwrap().support,
        Support::Unsupported(UnsupportedReason::InvalidHeader)
    ));
    let options = ConvertOptions::new(fixture.root.clone(), "synthetic".into());
    assert!(matches!(
        convert(
            &fixture.source,
            &options,
            &mut |_| {},
            &CancelToken::default()
        )
        .unwrap_err()
        .kind,
        ErrorKind::Unsupported(UnsupportedReason::InvalidHeader)
    ));
    assert_eq!(std::fs::read(&fixture.source).unwrap(), before);
    assert_eq!(std::fs::read_dir(&fixture.root).unwrap().count(), 1);
}
#[test]
fn independent_reader_rejects_generated_invalid_file() {
    let fixture = support::Fixture::invalid();
    assert!(outlook_pst::open_store(&fixture.source).is_err());
}

#[test]
fn pre_cancelled_conversion_has_no_outputs() {
    let fixture = support::Fixture::invalid();
    let token = CancelToken::default();
    token.clone().cancel();
    let options = ConvertOptions::new(fixture.root.clone(), "synthetic".into());
    assert!(
        matches!(convert(&fixture.source, &options, &mut |_| panic!("no work should start"), &token).unwrap(), Conversion::Cancelled { completed_files } if completed_files.is_empty())
    );
    assert_eq!(std::fs::read_dir(&fixture.root).unwrap().count(), 1);
}
#[test]
fn split_defaults_to_twenty_gib() {
    let options = ConvertOptions::new("unused".into(), "synthetic".into());
    assert_eq!(options.split, Split::Bytes(DEFAULT_SPLIT_BYTES));
}
