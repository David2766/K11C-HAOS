use serde_json::{Value,json};
pub(crate) fn u32at(b:&[u8],o:usize)->u32{u32::from_le_bytes(b[o..o+4].try_into().unwrap())}
pub(crate) fn u64at(b:&[u8],o:usize)->u64{u64::from_le_bytes(b[o..o+8].try_into().unwrap())}
pub(crate) fn crc(b:&[u8])->u32{let mut c=!0u32;for x in b{c^=*x as u32;for _ in 0..8{c=(c>>1)^if c&1!=0{0xedb88320}else{0};}}!c}
fn header(h:&[u8],current:u64,other:u64,entries:u64)->bool{
    if h.len()!=512||&h[..8]!=b"EFI PART"{return false;}
    let size=u32at(h,12) as usize;if !(92..=512).contains(&size){return false;}
    let mut bytes=h[..size].to_vec();bytes[16..20].fill(0);
    crc(&bytes)==u32at(h,16)&&u32at(h,8)==0x10000&&u32at(h,20)==0&&u64at(h,24)==current&&u64at(h,32)==other&&u64at(h,72)==entries&&u32at(h,80)==128&&u32at(h,84)==128
}
pub(crate) fn put64(b:&mut[u8],o:usize,n:u64){b[o..o+8].copy_from_slice(&n.to_le_bytes());}
pub(crate) fn put32(b:&mut[u8],o:usize,n:u32){b[o..o+4].copy_from_slice(&n.to_le_bytes());}
pub(crate) fn seal(h:&mut[u8]){put32(h,16,0);let size=u32at(h,12) as usize;let c=crc(&h[..size]);put32(h,16,c);}

pub fn require_k11c(primary:&[u8],tail:&[u8],sectors:u32)->crate::Result<()> {
    if check(primary,tail,sectors)["healthy"]!=true {return Err(crate::fail("GPT_INVALID","A healthy GPT pair is required"));}
    let labels=["hassos-boot","hassos-kernel0","hassos-system0","hassos-kernel1","hassos-system1","hassos-bootstate","hassos-overlay","hassos-data"];
    for (i,e) in primary[1024..].chunks_exact(128).enumerate(){
        if i>=8 {if e.iter().any(|b|*b!=0){return Err(crate::fail("GPT_LAYOUT","Unexpected additional partition"));}continue;}
        let chars:Vec<u16>=e[56..128].chunks_exact(2).map(|b|u16::from_le_bytes([b[0],b[1]])).take_while(|n|*n!=0).collect();
        if String::from_utf16(&chars).ok().as_deref()!=Some(labels[i]) || u64at(e,32)<34816 {return Err(crate::fail("GPT_LAYOUT","K11C reserved boot region or HAOS partitions missing"));}
    }
    if u64at(primary,1024+32)!=34816{return Err(crate::fail("GPT_LAYOUT","Expected K11C first partition at LBA 34816"));}Ok(())
}

/// Preserve partition identities and every payload byte. Only physical LBAs and
/// the disk-sized GPT/PMBR metadata change; HAOS itself is not patched.
pub fn relocated(primary:&[u8],tail:&[u8],source_sectors:u32,target_sectors:u32)->crate::Result<(Vec<u8>,Vec<u8>)>{
    if check(primary,tail,source_sectors)["healthy"]!=true {return Err(crate::fail("IMAGE_GPT","Invalid source GPT"));}
    if u64at(primary,1024+32)!=2048 {return Err(crate::fail("IMAGE_LAYOUT","Use an unmodified official image (first partition LBA 2048)"));}
    if u32at(primary,454)!=1 || u32at(primary,458)!=source_sectors-1 || primary[462..510].iter().any(|b|*b!=0){return Err(crate::fail("IMAGE_GPT","Unsupported protective MBR"));}
    if target_sectors < source_sectors.saturating_add(32768) {return Err(crate::fail("CAPACITY","Image does not fit the selected eMMC"));}
    let mut a=primary.to_vec();
    for e in a[1024..].chunks_exact_mut(128) {if e[..16].iter().any(|b|*b!=0){for o in [32,40]{put64(e,o,u64at(e,o)+32768);}}}
    put32(&mut a,458,target_sectors-1);
    let ec=crc(&a[1024..]);let h=&mut a[512..1024];
    put64(h,32,target_sectors as u64-1);put64(h,48,target_sectors as u64-34);put32(h,88,ec);seal(h);
    let mut b=vec![0;33*512];b[..16384].copy_from_slice(&a[1024..]);b[16384..].copy_from_slice(&a[512..1024]);
    let h=&mut b[16384..];put64(h,24,target_sectors as u64-1);put64(h,32,1);put64(h,72,target_sectors as u64-33);seal(h);
    require_k11c(&a,&b,target_sectors)?;Ok((a,b))
}

/// Repair only an unambiguous physical-end GPT. Never infer partition geometry.
pub fn repaired(primary:&[u8],tail:&[u8],sectors:u32)->crate::Result<(Vec<u8>,Vec<u8>)>{
    use crate::{fail,Result};
    if primary.len()!=34*512||tail.len()!=33*512||sectors<=34849{return Err(fail("GPT_INVALID","Invalid GPT capture"));}
    let end=sectors as u64-1;let a=&primary[512..1024];let b=&tail[16384..];
    let valid=|h:&[u8],entries:&[u8],current,other,at|->bool{
        if !header(h,current,other,at)||crc(entries)!=u32at(h,88){return false;}
        let first=u64at(h,40);let last=u64at(h,48);if first<34||last>end-33||first>last{return false;}
        let mut ranges=vec![];
        for e in entries.chunks_exact(128){if e[..16].iter().all(|b|*b==0){continue;}let s=u64at(e,32);let t=u64at(e,40);if s<first||t>last||s>t{return false;}ranges.push((s,t));}
        ranges.sort_unstable();!ranges.windows(2).any(|r|r[0].1>=r[1].0)
    };
    let av=valid(a,&primary[1024..],1,end,2);let bv=valid(b,&tail[..16384],end,1,end-32);
    if !av&&!bv{return Err(fail("GPT_NO_VALID_COPY","Neither GPT copy is valid"));}
    if av&&bv&&(primary[1024..]!=tail[..16384]||a[40..56]!=b[40..56]){return Err(fail("GPT_AMBIGUOUS","Valid copies disagree on partition layout"));}
    let mut p=primary.to_vec();let mut t=tail.to_vec();
    if av {t[..16384].copy_from_slice(&p[1024..]);t[16384..].copy_from_slice(&p[512..1024]);let h=&mut t[16384..];put64(h,24,end);put64(h,32,1);put64(h,72,end-32);seal(h);}
    else {p[1024..].copy_from_slice(&t[..16384]);p[512..1024].copy_from_slice(&t[16384..]);let h=&mut p[512..1024];put64(h,24,1);put64(h,32,end);put64(h,72,2);seal(h);}
    let verify:Result<()>=require_k11c(&p,&t,sectors);verify?;Ok((p,t))
}
// Fixed capture layout, not a repair routine. An unfamiliar GPT is kept as raw
// bytes and explicitly not certified. Both CRCs and all partition bounds count.
pub fn check(primary:&[u8],tail:&[u8],sectors:u32)->Value {
    if primary.len()!=34*512||tail.len()!=33*512||sectors<=34849{return json!({"healthy":false,"issues":["capture_size"]});}
    let a=&primary[512..1024];let b=&tail[32*512..];let x=&primary[1024..];let y=&tail[..32*512];let end=sectors as u64-1;
    let mut issues=vec![];
    if !header(a,1,end,2){issues.push("primary_header");}
    if !header(b,end,1,end-32){issues.push("backup_header");}
    if crc(x)!=u32at(a,88)||crc(y)!=u32at(b,88){issues.push("entries_crc");}
    if x!=y||a[56..72]!=b[56..72]||a[40..56]!=b[40..56]{issues.push("copies_differ");}
    if primary[510..512]!=[0x55,0xaa]||primary[450]!=0xee{issues.push("protective_mbr");}
    let first=u64at(a,40);let last=u64at(a,48);
    if first<34||last>=end-32||first>last{issues.push("usable_bounds");}
    let mut ranges=vec![];
    for e in x.chunks_exact(128){
        if e[..16].iter().all(|b|*b==0){continue;}
        let start=u64at(e,32);let finish=u64at(e,40);
        if start<first||finish>last||start>finish{issues.push("partition_bounds");}
        ranges.push((start,finish));
    }
    ranges.sort_unstable();if ranges.windows(2).any(|r|r[0].1>=r[1].0){issues.push("partition_overlap");}
    json!({"healthy":issues.is_empty(),"issues":issues,"partitions":ranges.len(),"layout":"128x128"})
}
#[cfg(test)] mod tests{
    use super::*;
    fn put64(b:&mut[u8],o:usize,n:u64){b[o..o+8].copy_from_slice(&n.to_le_bytes());}
    fn put32(b:&mut[u8],o:usize,n:u32){b[o..o+4].copy_from_slice(&n.to_le_bytes());}
    fn seal(h:&mut[u8]){put32(h,16,0);let c=crc(&h[..92]);put32(h,16,c);}
    #[test] fn healthy_and_damaged_copies(){
        let sectors=65536;let mut a=vec![0u8;34*512];let mut b=vec![0u8;33*512];
        a[510..512].copy_from_slice(&[0x55,0xaa]);a[450]=0xee;
        a[1024]=1;put64(&mut a,1024+32,34816);put64(&mut a,1024+40,60000);
        b[..16384].copy_from_slice(&a[1024..]);let ec=crc(&a[1024..]);
        let h=&mut a[512..1024];h[..8].copy_from_slice(b"EFI PART");put32(h,8,0x10000);put32(h,12,92);
        put64(h,24,1);put64(h,32,65535);put64(h,40,34);put64(h,48,65502);h[56]=42;put64(h,72,2);put32(h,80,128);put32(h,84,128);put32(h,88,ec);seal(h);
        b[16384..].copy_from_slice(h);let h=&mut b[16384..];put64(h,24,65535);put64(h,32,1);put64(h,72,65503);seal(h);
        assert_eq!(check(&a,&b,sectors)["healthy"],true);
        let mut bad=b.clone();bad[16384+56]^=1;seal(&mut bad[16384..]);assert_eq!(check(&a,&bad,sectors)["healthy"],false);
        let mut bad=a.clone();bad[16+512]^=1;assert_eq!(check(&bad,&b,sectors)["healthy"],false);
        let mut bad=a.clone();bad[1100]^=1;assert_eq!(check(&bad,&b,sectors)["healthy"],false);
        assert_eq!(check(&a,&b,65537)["healthy"],false);
    }
}
