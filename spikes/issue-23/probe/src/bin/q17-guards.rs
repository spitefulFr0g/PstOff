use pstfree::convert::q21::{derive, omit_cache, preserve_absence};
use std::collections::BTreeMap;
fn main() {
    let mut p = BTreeMap::new();
    let mut checks = 0;
    for (tag, reason) in [(0x65c60003,"ms_pst_get_zero"), (0x40760003,"ms_pst_get_zero"), (0x0e03001f,"ms_pst_display")] {
        let (v,r) = derive(tag,&p,4,2).unwrap();
        assert_eq!(r,reason);
        assert!(preserve_absence(false,r,&v));
        assert!(!preserve_absence(true,r,&v)); checks += 2;
    }
    assert!(preserve_absence(false,"ms_pst_hasattach",&[0]));
    assert!(!preserve_absence(false,"ms_pst_hasattach",&[1]));
    assert!(!preserve_absence(false,"exact_pc",&[0])); checks += 3;
    assert!(omit_cache(0x66280014,true,&p));
    assert!(!omit_cache(0x66280014,false,&p)); checks += 2;
    p.insert(0x66280014,vec![7;8]);
    assert!(!omit_cache(0x66280014,true,&p));
    assert_eq!(derive(0x66280014,&p,4,2).unwrap().0,vec![7;8]); checks += 2;
    for tag in [0x0e170003,0x66120003,0x0e380003,0x66280003,0x0e05001f] {
        assert!(!omit_cache(tag,true,&p));
        assert!(derive(tag,&p,4,2).is_none()); checks += 2;
    }
    assert!(!preserve_absence(false,"ms_pst_display",&[65,0])); checks += 1;
    assert!(!preserve_absence(false,"ms_pst_display",&[0,0])); checks += 1;
    println!("policy_guards_passed={checks}");
}
