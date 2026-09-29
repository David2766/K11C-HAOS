use crate::{Result, fail, hash_bytes, workflow::{UsbIo, Progress}};
use std::{path::Path, time::Duration};

pub const NAME: &str = "k11c-usb-loader-v1.23.114.bin";
pub const SHA256: &str = "b0228bbe9d3be4ea13f399df58289a82b5dfe62d1f67d75512a9ddd2d9b25454";
#[derive(Debug)]
pub struct Entry { pub area:u16, pub offset:usize, pub size:usize, pub delay_ms:u32 }
pub struct Loader { bytes:Vec<u8>, entries:Vec<Entry> }
fn word(b:&[u8],at:usize)->u32 {u32::from_le_bytes(b[at..at+4].try_into().unwrap())}

// Layout follows pinned rockfile::boot / rockusb examples/common.rs. All bounds
// are checked before the first transport call. Only the audited binary is used.
fn parse(bytes:Vec<u8>)->Result<Loader> {
    if bytes.len()<102 || (&bytes[..4]!=b"BOOT" && &bytes[..4]!=b"LDR ") {return Err(fail("LOADER_FORMAT","Invalid boot header"));}
    let mut entries=Vec::new();
    for (header,area) in [(25,0x471),(31,0x472)] {
        let count=bytes[header] as usize;let offset=word(&bytes,header+1) as usize;
        let stride=bytes[header+5] as usize;
        if count==0 || count>8 || stride!=57 || offset<102 || offset+count*stride>bytes.len() {return Err(fail("LOADER_FORMAT","Invalid entry table"));}
        for i in 0..count {
            let e=&bytes[offset+i*stride..offset+(i+1)*stride];
            let start=word(e,45) as usize;let size=word(e,49) as usize;let delay_ms=word(e,53);
            if e[0]!=57 || size==0 || start<102 || start.checked_add(size).is_none_or(|end|end>bytes.len()) || delay_ms>5000 {
                return Err(fail("LOADER_FORMAT","Invalid payload bounds/delay"));
            }
            entries.push(Entry{area,offset:start,size,delay_ms});
        }
    }
    Ok(Loader{bytes,entries})
}
pub fn load(base:&Path)->Result<Loader> {
    let bytes=std::fs::read(base.join("resources/loader").join(NAME)).map_err(|e|fail("LOADER_MISSING",e))?;
    checked(bytes)
}
fn checked(bytes:Vec<u8>)->Result<Loader> {
    if hash_bytes(&bytes) != SHA256 {return Err(fail("LOADER_HASH","Bundled RAM loader checksum does not match"));}
    parse(bytes)
}
pub async fn upload(io:&mut impl UsbIo, loader:&Loader, progress:&mut impl FnMut(Progress))->Result<()> {
    let total=loader.entries.iter().map(|e|e.size as u64).sum();let mut completed=0;
    for e in &loader.entries {
        progress(Progress::new("upload",completed,total));
        io.upload(e.area,&loader.bytes[e.offset..e.offset+e.size]).await?;
        if e.delay_ms>0 {tokio::time::sleep(Duration::from_millis(e.delay_ms as u64)).await;}
        completed+=e.size as u64;
    }
    progress(Progress::new("upload",completed,total));Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    fn fixture()->Vec<u8>{std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/loader").join(NAME)).unwrap()}
    #[test] fn audited_loader_and_tamper_check(){
        let b=fixture();let p=checked(b.clone()).unwrap();
        assert_eq!(p.entries.iter().map(|e|(e.area,e.size)).collect::<Vec<_>>(),vec![(0x471,2048),(0x471,59392),(0x472,100352)]);
        let mut corrupt=b;corrupt[1000]^=1;assert!(checked(corrupt).is_err());
        assert!(checked(vec![]).is_err());
    }
    #[test] fn entry_bounds_are_checked_before_transport(){
        let mut b=fixture();b[25]=255;assert!(parse(b).is_err());
        let mut b=fixture();b[102+49..102+53].copy_from_slice(&u32::MAX.to_le_bytes());assert!(parse(b).is_err());
        let mut b=fixture();b[102+53..102+57].copy_from_slice(&5001u32.to_le_bytes());assert!(parse(b).is_err());
    }
    #[tokio::test] async fn uploads_only_ram_in_order_and_stops_on_error(){
        let l=checked(fixture()).unwrap();let mut io=crate::workflow::tests::Fake::new();
        upload(&mut io,&l,&mut |_|{}).await.unwrap();
        assert_eq!(io.uploads,vec![(0x471,2048),(0x471,59392),(0x472,100352)]);
        let mut io=crate::workflow::tests::Fake::new();io.fail_upload=true;
        assert!(upload(&mut io,&l,&mut |_|{}).await.is_err());assert_eq!(io.uploads.len(),1);
    }
}
