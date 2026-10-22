// Exercise real PUBLIC PC/table allocations through the writer's selection/emission seams.
use pstfree::{convert::q12, ltp::Heap, ndb::Pst};
fn u16b(b:&[u8],i:usize)->usize {u16::from_le_bytes(b[i..i+2].try_into().unwrap()) as usize}
fn u32b(b:&[u8],i:usize)->u32 {u32::from_le_bytes(b[i..i+4].try_into().unwrap())}
fn bth_records(h:&Heap)->Result<(usize,Vec<Vec<u8>>),String>{
 let b=h.item(h.user_root)?; assert_eq!(&b[..4],&[0xb5,2,6,0]);
 Ok((2,h.item(u32b(b,4))?.chunks_exact(8).map(|r|r.to_vec()).collect()))
}
fn main(){
 let mut p=Pst::open(&std::env::args().nth(1).unwrap()).unwrap();
 let ns=p.nodes(); let plan=q12::plan(&mut p,&ns).unwrap(); assert_eq!(plan.len(),17);
 let mut checked=0;
 for n in ns.iter().filter(|n|matches!(n.nid_type(),4|8)) {
  let blocks=p.node_blocks(n.bid_data).unwrap(); let h=Heap::new(blocks.clone()).unwrap();
  let val=plan[&n.nid]; let (_,v)=q12::eligible(false,&[Ok(val),Ok(val)],true); assert_eq!(v,Some(val));
  let promoted=q12::promote(blocks.clone(),v.unwrap()).unwrap(); let ph=Heap::new(promoted.clone()).unwrap();
  let (key,before)=bth_records(&h).unwrap(); let (pk,after)=bth_records(&ph).unwrap(); assert_eq!(key,pk);
  assert_eq!(after.len(),before.len()+1);
  for r in &before {assert!(after.contains(r));}
  let r=after.iter().find(|r|u16b(r,0)==0x3008).unwrap(); assert_eq!(u16b(r,2),0x40); assert_eq!(ph.item(u32b(r,4)).unwrap(),val);
  let root=u32b(h.item(h.user_root).unwrap(),4);
  // Every old allocation except the leaf records is byte-exact, with the same HID.
  for (bi,b) in blocks.iter().enumerate(){let map=u16b(b,0); for ai in 1..=u16b(b,map){let id=((bi as u32)<<16)|((ai as u32)<<5); if id!=root {assert_eq!(h.item(id).unwrap(),ph.item(id).unwrap());}}}
  let different=(u64::from_le_bytes(val)+1).to_le_bytes();
  // Existing item wins even when the cache conflicts/is invalid or absent.
  assert_eq!(q12::promote(promoted.clone(),different).unwrap(),promoted);
  for obs in [vec![],vec![Err("missing".into())],vec![Ok([0;8])],vec![Ok([255;8])],vec![Ok(val),Ok(different)],vec![Ok(val),Err("invalid occurrence".into())]] {
   assert!(q12::eligible(false,&obs,true).1.is_none());
   assert_eq!(q12::eligible(true,&obs,false),("preserved_existing",None));
   let emitted=match q12::eligible(false,&obs,true).1 {Some(v)=>q12::promote(blocks.clone(),v).unwrap(),None=>blocks.clone()};
   assert_eq!(emitted,blocks,"rejected cache changed emitted stream");
  }
  assert!(q12::eligible(false,&[Ok(val)],false).1.is_none());
  assert!(q12::promote(blocks.clone(),[0;8]).is_err());
  checked+=1;
 }
 // Mutate actual source TC bytes: wrong type/length, missing presence, invalid value,
 // duplicate descriptor and row identity must never produce an eligible timestamp.
 let n=ns.iter().find(|n|n.nid==0x218e).unwrap(); let original=p.node_blocks(n.bid_data).unwrap();
 let h=Heap::new(original.clone()).unwrap(); let info=h.item(h.user_root).unwrap();
 let map=u16b(&original[0],0); let alloc=|hid:u32| {assert_eq!(hid>>16,0); u16b(&original[0],map+4+(((hid>>5) as usize)-1)*2)};
 let ip=alloc(h.user_root); let rm=alloc(u32b(info,14)); let ceb=u16b(info,6);
 let ci=info[22..].chunks_exact(8).position(|c|u32b(c,0)==0x30080040).unwrap(); let cp=ip+22+ci*8;
 let off=u16b(&original[0],cp+4); let bit=original[0][cp+7] as usize;
 assert!(q12::observations(original.clone(),None).unwrap().iter().all(|(_,v)|v.is_ok()));
 for mode in 0..7 {
  let mut bs=original.clone(); let b=&mut bs[0];
  match mode {0=>b[cp]=3,1=>b[cp+6]=4,2=>b[rm+ceb+bit/8]&=!(0x80>>(bit%8)),3=>b[rm+off..rm+off+8].fill(0),4=>b[rm]^=32,5=>{b[cp+2]=0x37; b[cp+3]=0;},6=>{let other=if ci==0 {ip+30} else {ip+22}; b[other+2]=8;b[other+3]=0x30;},_=>unreachable!()}
  let rejected=match q12::observations(bs,None) {Err(_)=>true,Ok(rows)=>rows.iter().any(|(_,v)|v.is_err())}; assert!(rejected,"raw mutation {mode} accepted");
 }
 println!("Q12_GUARDS PASS actual_source_messages={} raw_table_rejection_cases=7 existing_preserved=true exact_emission=true missing_invalid_conflicting_rejected=true",checked);
}
