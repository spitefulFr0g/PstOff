// Bounded diagnostic: property IDs only; never print source timestamp/content values.
use pstfree::{ltp::{read_tc,read_node_pc},ndb::Pst};
use std::collections::BTreeMap;
fn main(){
 let mut p=Pst::open(&std::env::args().nth(1).unwrap()).unwrap();let ns=p.nodes();
 let mut counts=BTreeMap::new();let mut times=0;let mut rows=0;
 for t in ns.iter().filter(|n|matches!(n.nid_type(),14|15|16)) {
  let tc=read_tc(&mut p,t.bid_data,t.bid_sub).unwrap();
  for r in tc.rows {if let Some(n)=ns.iter().find(|n|n.nid==r.id && matches!(n.nid_type(),4|8)) {
   rows+=1;let pc=read_node_pc(&mut p,n).unwrap();
   for (id,v) in r.props {if id==0x67f2 || id==0x67f3 {continue;}
    if let Some(pv)=pc.props.get(&id) {if format!("{:?}",pv)!=format!("{:?}",v){*counts.entry(format!("different:{:04x}",id)).or_insert(0)+=1;} else if id==0x3008 {times+=1;}}
    else {*counts.entry(format!("absent_in_pc:{:04x}",id)).or_insert(0)+=1;}
   }
  }}
 }
 println!("message_rows={} exact_cached_modtime_matches={} other_cache_vs_pc_counts={:?}",rows,times,counts);
}
