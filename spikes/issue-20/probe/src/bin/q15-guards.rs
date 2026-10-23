use pstfree::convert::q15::{select, valid_pair, Pair};
fn main() {
    let key = vec![1u8; 17];
    let mut pcl = vec![17];
    pcl.extend(&key);
    let pair: Pair = [key, pcl];
    let none = [None, None];
    let good = vec![Ok(pair.clone())];
    assert!(valid_pair(&pair));
    assert_eq!(select(&none, &good, true).unwrap().len(), 2);
    assert!(select(&none, &good, false).is_err());
    assert!(select(&none, &[], true).is_err());
    assert!(select(&none, &[Err("missing".into())], true).is_err());
    let mut other = pair.clone();
    other[0][0] = 2;
    other[1][1] = 2;
    assert!(select(&none, &[Ok(pair.clone()), Ok(other)], true).is_err());
    for bad in [
        [vec![1; 16], pair[1].clone()],
        [pair[0].clone(), vec![24, 1]],
        [pair[0].clone(), vec![]],
        [pair[0].clone(), vec![0]],
        [vec![2; 17], pair[1].clone()],
    ] {
        assert!(!valid_pair(&bad));
        assert!(select(&none, &[Ok(bad)], true).is_err());
    }
    let mut higher = pair[0].clone();
    higher[0] = 2;
    let mut ordered = pair[1].clone();
    ordered.push(17);
    ordered.extend(&higher);
    assert!(valid_pair(&[pair[0].clone(), ordered.clone()]));
    let mut descending = vec![17];
    descending.extend(&higher);
    descending.extend(&pair[1]);
    assert!(!valid_pair(&[pair[0].clone(), descending]));
    let mut duplicate = pair[1].clone();
    duplicate.extend(&pair[1]);
    assert!(!valid_pair(&[pair[0].clone(), duplicate]));
    let mut existing = [Some((0x102, pair[0].clone())), None];
    let selected = select(&existing, &good, true).unwrap();
    assert_eq!(selected.len(), 1);
    assert!(selected.contains_key(&0x65e3));
    existing[0].as_mut().unwrap().1[0] = 2;
    assert!(select(&existing, &good, true).is_err());
    existing[0] = Some((3, vec![]));
    assert!(select(&existing, &good, true).is_err());
    let both = [
        Some((0x102, pair[0].clone())),
        Some((0x102, pair[1].clone())),
    ];
    assert_eq!(
        select(&both, &good, true).unwrap_err(),
        "preserved_existing"
    );
    println!("Q15 selection guards PASS: table integrity, missing cells, conflicts, XID/PCL shape, pair consistency, existing type/value, single missing member, preserve existing");
}
