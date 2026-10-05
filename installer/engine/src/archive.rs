//! Lossless, bounded-memory archives of eMMC user-area sectors. No erase,
//! hardware partition switch, RPMB or OTP command is available here.
use crate::{Result,fail,gpt,workflow::{UsbIo,Identity,Progress,read_exact},flash::Range};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use sha2::{Sha256,Digest};
use std::{fs::{self,File,OpenOptions},io::{Read,Write,Seek,SeekFrom,BufReader},path::{Path,PathBuf},os::windows::fs::OpenOptionsExt,time::Instant};
use xz2::read::XzDecoder;
use crate::pipeline::{self,HashWorker,Block};
const MAGIC_XZ:&[u8;8]=b"K11CBK03";
const MAGIC_ZSTD:&[u8;8]=b"K11CBK04";
const CHUNK:usize=1024*1024;
const FOOTER:u64=48;
const MAX_META:u64=256*1024;

fn magic(format:u32)->Result<&'static[u8;8]>{match format{3=>Ok(MAGIC_XZ),4=>Ok(MAGIC_ZSTD),_=>Err(err("Unsupported archive version"))}}
fn fast_encoder<W:Write>(writer:W,total:u64)->Result<zstd::stream::write::Encoder<'static,W>>{
    let mut encoder=zstd::stream::write::Encoder::new(writer,1).map_err(ioerr)?;
    // Zstd owns the bounded worker queue, ordering and worker lifetime. USB
    // remains single-owner; its next read overlaps the two compression workers.
    encoder.multithread(2).map_err(ioerr)?;
    encoder.window_log(23).map_err(ioerr)?;
    encoder.set_parameter(zstd::zstd_safe::CParameter::JobSize(2*CHUNK as u32)).map_err(ioerr)?;
    encoder.include_checksum(true).map_err(ioerr)?;
    encoder.set_pledged_src_size(Some(total)).map_err(ioerr)?;
    Ok(encoder)
}

fn err(e:impl std::fmt::Display)->crate::Failure{fail("BACKUP_VERIFY",e)}
fn ioerr(e:impl std::fmt::Display)->crate::Failure{fail("BACKUP_IO",e)}
pub fn valid_id(id:&str)->bool{
    id.len()<120 && !id.contains("..") && id.bytes().all(|b|b.is_ascii_alphanumeric()||b"-._".contains(&b)) &&
    ((["FULL-","BOOT-","HAOS-"].iter().any(|p|id.starts_with(p))&&id.ends_with(".k11cbackup")) ||
    (id.starts_with("raw-")&&id.len()==68&&id[4..].bytes().all(|b|b.is_ascii_hexdigit())))
}
pub fn path(base:&Path,id:&str)->Result<PathBuf>{if !valid_id(id){return Err(fail("BACKUP_ID","Invalid backup identifier"));}Ok(base.join("backup").join(id))}
pub fn kind(s:&str)->Result<&str>{if ["FULL","BOOT","HAOS"].contains(&s){Ok(s)}else{Err(fail("BACKUP_KIND","Select FULL, BOOT or HAOS"))}}
#[derive(Clone,Serialize,Deserialize)]
pub struct Meta {
    pub format:u32,pub kind:String,pub identity:Identity,pub ranges:Vec<Range>,
    pub primary:Vec<u8>,pub tail:Vec<u8>,pub os:String,pub created:String,
    pub read_capability_hex:String,pub usb_reread_verified:bool,
}
pub fn classify(primary:&[u8],tail:&[u8],sectors:u32)->String{
    if gpt::require_k11c(primary,tail,sectors).is_ok(){"HAOS"}
    else if gpt::check(primary,tail,sectors)["healthy"]==true{"other"}else{"unknown"}.into()
}
pub fn ranges(kind_name:&str,primary:&[u8],tail:&[u8],sectors:u32)->Result<Vec<Range>>{
    kind(kind_name)?;
    if sectors<=34849{return Err(fail("INVALID_CAPACITY",sectors));}
    let blank=|label:&str,lba,bytes|Range{label:label.into(),lba,bytes,sha256:String::new()};
    if kind_name=="FULL" {return Ok(vec![blank("Full eMMC user area",0,sectors as u64*512)]);}
    gpt::require_k11c(primary,tail,sectors)?;
    if kind_name=="BOOT" {return Ok(vec![blank("Boot firmware and primary GPT",0,34816*512),blank("Backup GPT",sectors-33,33*512)]);}
    primary[1024..1024+8*128].chunks_exact(128).map(|e|{
        let start=gpt::u64at(e,32);let end=gpt::u64at(e,40);
        let label=String::from_utf16_lossy(&e[56..].chunks_exact(2).map(|b|u16::from_le_bytes([b[0],b[1]])).take_while(|c|*c!=0).collect::<Vec<_>>());
        Ok(blank(&label,start as u32,(end-start+1)*512))
    }).collect()
}
pub async fn headers(io:&mut impl UsbIo,identity:&Identity)->Result<(Vec<u8>,Vec<u8>)>{
    if identity.sectors<=34849{return Err(fail("INVALID_CAPACITY",identity.sectors));}
    let mut primary=vec![0;34*512];let mut tail=vec![0;33*512];
    read_exact(io,0,&mut primary).await?;read_exact(io,identity.sectors-33,&mut tail).await?;
    crate::workflow::require_gpt_bytes(&primary[512..])?;crate::workflow::require_gpt_bytes(&tail)?;
    Ok((primary,tail))
}
pub async fn inspect(io:&mut impl UsbIo)->Result<Value>{
    let identity=io.identity().await?;crate::usb::require_emmc(&identity.storage)?;crate::workflow::require_full_read(io).await?;
    let (a,b)=headers(io,&identity).await?;let os=classify(&a,&b,identity.sectors);
    Ok(json!({"identity":identity,"os":os,"gpt":gpt::check(&a,&b,identity.sectors),"kinds":if os=="HAOS"{vec!["FULL","BOOT","HAOS"]}else{vec!["FULL"]}}))
}
fn timestamp()->String{
    let mut t=unsafe{std::mem::zeroed::<windows_sys::Win32::Foundation::SYSTEMTIME>()};
    unsafe{windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t)};
    format!("{:02}{:02}{:02}-{:02}{:02}{:02}",t.wYear%100,t.wMonth,t.wDay,t.wHour,t.wMinute,t.wSecond)
}
fn reserve(base:&Path,kind:&str)->Result<(PathBuf,PathBuf,File)>{
    let dir=base.join("backup");fs::create_dir_all(&dir).map_err(ioerr)?;let stamp=timestamp();
    for n in 0..10000{
        let suffix=if n==0{String::new()}else{format!("-{n}")};
        let p=dir.join(format!("{kind}-{stamp}{suffix}.k11cbackup"));if p.exists(){continue;}
        let tmp=p.with_extension("k11cbackup.partial");
        match OpenOptions::new().write(true).read(true).create_new(true).open(&tmp){
            Ok(f)=>return Ok((p,tmp,f)),Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,Err(e)=>return Err(ioerr(e))
        }
    }Err(ioerr("Could not create unique backup name"))
}
fn append_footer(mut file:File,meta:&Meta)->Result<()> {
    let bytes=serde_json::to_vec(meta).map_err(err)?;
    if bytes.len() as u64>MAX_META{return Err(err("Metadata exceeds bound"));}
    file.write_all(&bytes).map_err(ioerr)?;file.write_all(&Sha256::digest(&bytes)).map_err(ioerr)?;
    file.write_all(&(bytes.len() as u64).to_le_bytes()).map_err(ioerr)?;file.write_all(magic(meta.format)?).map_err(ioerr)?;
    file.sync_all().map_err(ioerr)
}
pub async fn create(io:&mut impl UsbIo,base:&Path,kind_name:&str,notify:&mut impl FnMut(Progress))->Result<Value>{
    let identity=io.identity().await?;crate::usb::require_emmc(&identity.storage)?;
    let caps=crate::workflow::require_full_read(io).await?;let (primary,tail)=headers(io,&identity).await?;
    let mut selected=ranges(kind_name,&primary,&tail,identity.sectors)?;
    let (final_path,tmp,mut file)=reserve(base,kind_name)?;file.write_all(MAGIC_ZSTD).map_err(ioerr)?;
    let run:Result<Value>=async{
        let started=Instant::now();let total=selected.iter().map(|r|r.bytes).sum();
        let mut encoder=fast_encoder(file,total)?;let mut buf=vec![0;CHUNK];let mut done=0;
        for r in &mut selected{
            let mut offset=0;let mut hash=Sha256::new();
            while offset<r.bytes{let n=(r.bytes-offset).min(CHUNK as u64) as usize;
                read_exact(io,r.lba+(offset/512) as u32,&mut buf[..n]).await?;
                encoder.write_all(&buf[..n]).map_err(ioerr)?;hash.update(&buf[..n]);offset+=n as u64;done+=n as u64;
                notify(Progress::new("read-compress",done,total));
            }r.sha256=format!("{:x}",hash.finalize());
        }
        let file=encoder.finish().map_err(ioerr)?;
        let read_compress_s=started.elapsed().as_secs_f64();let checking=Instant::now();
        let meta=Meta{format:4,kind:kind_name.into(),identity:identity.clone(),ranges:selected,os:classify(&primary,&tail,identity.sectors),primary,tail,created:timestamp(),read_capability_hex:caps,usb_reread_verified:true};
        // USB consistency and persisted compressed bytes both count. Never
        // publish the final name until both complete successfully.
        compare_device(io,&meta,notify).await?;
        let device_verify_s=checking.elapsed().as_secs_f64();
        if io.identity().await?!=identity{return Err(fail("DEVICE_CHANGED","Identity changed during backup"));}
        crate::workflow::require_full_read(io).await?;
        append_footer(file,&meta)?;
        let checking=Instant::now();let mut archived=Archive::open(&tmp)?;archived.verify(notify)?;drop(archived);
        let file_verify_s=checking.elapsed().as_secs_f64();
        fs::rename(&tmp,&final_path).map_err(ioerr)?;
        let mut result=report(&final_path,&meta)?;
        result["timings"]=json!({"read_compress_s":read_compress_s,"device_verify_s":device_verify_s,"file_verify_s":file_verify_s,"total_s":started.elapsed().as_secs_f64()});
        Ok(result)
    }.await;
    run.map_err(|e|fail(&e.code,format!("{}; incomplete backup={}",e.detail,tmp.display())))
}
pub async fn compare_device(io:&mut impl UsbIo,meta:&Meta,notify:&mut impl FnMut(Progress))->Result<()> {
    let total=meta.ranges.iter().map(|r|r.bytes).sum();let mut done=0;
    for r in &meta.ranges {
        let actual=pipeline::read_digest(io,r.lba,r.bytes,&mut |offset|notify(Progress::new("verify",done+offset,total))).await?;
        if actual.hash!=r.sha256{return Err(fail("DEVICE_CHANGED","Device contents do not match the recovery backup"));}
        done+=r.bytes;
    }Ok(())
}
fn validate(meta:&Meta)->Result<()> {
    magic(meta.format)?;
    if meta.identity.storage!="emmc"||meta.primary.len()!=34*512||meta.tail.len()!=33*512{return Err(err("Invalid metadata"));}
    let expected=ranges(&meta.kind,&meta.primary,&meta.tail,meta.identity.sectors)?;
    if meta.os!=classify(&meta.primary,&meta.tail,meta.identity.sectors){return Err(err("OS classification differs from saved sectors"));}
    if expected.len()!=meta.ranges.len(){return Err(err("Incomplete range list"));}
    for (a,b) in expected.iter().zip(&meta.ranges){if a.lba!=b.lba||a.bytes!=b.bytes||b.sha256.len()!=64||!b.sha256.bytes().all(|b|b.is_ascii_hexdigit()){return Err(err("Invalid range/hash"));}}
    crate::workflow::require_gpt_bytes(&meta.primary[512..])?;crate::workflow::require_gpt_bytes(&meta.tail)?;
    Ok(())
}
fn report(p:&Path,meta:&Meta)->Result<Value>{
    Ok(json!({"id":p.file_name().unwrap().to_string_lossy(),"path":p,"kind":meta.kind,"os":meta.os,"identity":meta.identity,"bytes":meta.ranges.iter().map(|r|r.bytes).sum::<u64>(),"stored_bytes":fs::metadata(p).map_err(ioerr)?.len(),"gpt":gpt::check(&meta.primary,&meta.tail,meta.identity.sectors),"verified":true,"restorable":true,"scope":"emmc-user-area","usb_reread_verified":meta.usb_reread_verified}))
}
pub struct Archive {file:File,pub meta:Meta,payload:u64,raw:bool}
pub enum Reader{Raw(std::io::Take<File>),Xz(XzDecoder<std::io::Take<File>>),Zstd(zstd::stream::read::Decoder<'static,BufReader<std::io::Take<File>>>) }
impl Read for Reader{fn read(&mut self,b:&mut[u8])->std::io::Result<usize>{match self{Self::Raw(r)=>r.read(b),Self::Xz(r)=>r.read(b),Self::Zstd(r)=>r.read(b)}}}
impl Archive {
    pub fn open(path:&Path)->Result<Self>{
        let mut f=OpenOptions::new().read(true).share_mode(1).open(path).map_err(ioerr)?;
        let len=f.metadata().map_err(ioerr)?.len();if len<56{return Err(err("Truncated archive"));}
        let mut signature=[0;8];f.read_exact(&mut signature).map_err(err)?;
        let format=if &signature==MAGIC_XZ{3}else if &signature==MAGIC_ZSTD{4}else{return Err(err("Not a K11C backup"));};
        f.seek(SeekFrom::End(-(FOOTER as i64))).map_err(err)?;let mut footer=[0;48];f.read_exact(&mut footer).map_err(err)?;
        if footer[40..]!=signature{return Err(err("Incomplete archive footer"));}
        let n=u64::from_le_bytes(footer[32..40].try_into().unwrap());if n>MAX_META||n>len-56{return Err(err("Invalid metadata length"));}
        let payload=len-FOOTER-n-8;f.seek(SeekFrom::Start(8+payload)).map_err(err)?;let mut bytes=vec![0;n as usize];f.read_exact(&mut bytes).map_err(err)?;
        if Sha256::digest(&bytes)[..]!=footer[..32]{return Err(err("Metadata checksum mismatch"));}
        let meta:Meta=serde_json::from_slice(&bytes).map_err(err)?;validate(&meta)?;
        if meta.format!=format{return Err(err("Archive version does not match payload signature"));}
        Ok(Self{file:f,meta,payload,raw:false})
    }
    pub fn reader(&self)->Result<Reader>{
        let mut f=self.file.try_clone().map_err(err)?;f.seek(SeekFrom::Start(if self.raw{0}else{8})).map_err(err)?;
        if self.raw{Ok(Reader::Raw(f.take(self.payload)))}else if self.meta.format==3{
            let stream=xz2::stream::Stream::new_stream_decoder(128*1024*1024,0).map_err(err)?;
            Ok(Reader::Xz(XzDecoder::new_stream(f.take(self.payload),stream)))
        }else{
            let mut decoder=zstd::stream::read::Decoder::new(f.take(self.payload)).map_err(err)?.single_frame();
            decoder.window_log_max(27).map_err(err)?;
            Ok(Reader::Zstd(decoder))
        }
    }
    pub fn verify(&mut self,notify:&mut impl FnMut(Progress))->Result<()> {
        let mut stream=self.reader()?;let mut primary=vec![0;34*512];let mut tail=vec![0;33*512];
        let total=self.meta.ranges.iter().map(|r|r.bytes).sum();let mut done=0;
        for r in &self.meta.ranges{let mut worker=HashWorker::new(r.bytes)?;let mut offset=0;
            while offset<r.bytes{let mut buf=worker.buffer()?;let n=(r.bytes-offset).min(CHUNK as u64) as usize;stream.read_exact(&mut buf[..n]).map_err(err)?;
                overlay(&buf[..n],r.lba as u64*512+offset,&mut primary,0);
                overlay(&buf[..n],r.lba as u64*512+offset,&mut tail,(self.meta.identity.sectors as u64-33)*512);
                worker.submit(Block{offset,len:n,data:buf})?;
                offset+=n as u64;done+=n as u64;notify(Progress::new("verify-backup-file",done,total));
            }
            if worker.finish()?.hash!=r.sha256{return Err(err("Backup payload checksum mismatch"));}
        }
        if stream.read(&mut [0;1]).map_err(err)?!=0{return Err(err("Unexpected extra backup payload"));}
        match stream{
            Reader::Xz(r)=>if r.total_in()!=self.payload{return Err(err("Trailing compressed payload"));},
            Reader::Zstd(r)=>{let input=r.finish();if input.get_ref().limit()!=0||!input.buffer().is_empty(){return Err(err("Trailing compressed payload"));}},
            Reader::Raw(_)=>{},
        }
        if self.meta.kind!="HAOS"&&(primary!=self.meta.primary||tail!=self.meta.tail){return Err(err("GPT metadata does not match payload"));}
        Ok(())
    }
    pub fn boot_snapshot(&self)->Result<(Vec<u8>,Vec<u8>)>{
        if self.meta.kind=="HAOS"{return Err(err("HAOS archive has no boot firmware"));}
        let mut reader=self.reader()?;let mut head=vec![0;34816*512];reader.read_exact(&mut head).map_err(err)?;
        Ok((head,self.meta.tail.clone()))
    }
}
fn overlay(src:&[u8],offset:u64,dst:&mut[u8],target:u64){let start=offset.max(target);let end=(offset+src.len() as u64).min(target+dst.len() as u64);if start<end{dst[(start-target) as usize..(end-target) as usize].copy_from_slice(&src[(start-offset) as usize..(end-offset) as usize]);}}
pub fn open(base:&Path,id:&str,notify:&mut impl FnMut(Progress))->Result<Archive>{
    let mut a=open_metadata(base,id)?;a.verify(notify)?;Ok(a)
}
pub fn open_metadata(base:&Path,id:&str)->Result<Archive>{
    if !valid_id(id){return Err(fail("BACKUP_ID","Select a backup file"));}
    let a=if id.starts_with("raw-"){
        let path=raw_path(base,id)?;
        // Use the imported digest as the expectation, not a newly computed one.
        // One full pass below verifies payload, exact length and saved headers.
        raw_headers(&path,id[4..].into())?
    }else{Archive::open(&path(base,id)?)?};
    Ok(a)
}
fn raw_path(base:&Path,id:&str)->Result<PathBuf>{
    let p=base.join("data/backup-imports").join(format!("{id}.json"));
    let mut bytes=vec![];File::open(p).map_err(ioerr)?.take(MAX_META+1).read_to_end(&mut bytes).map_err(ioerr)?;
    if bytes.len()>MAX_META as usize{return Err(err("Import receipt too large"));}
    let receipt:Value=serde_json::from_slice(&bytes).map_err(err)?;
    Ok(PathBuf::from(receipt["path"].as_str().ok_or_else(||err("Missing raw path"))?))
}
// Bounded header reads for catalog previews; never certify the full payload here.
fn raw_headers(path:&Path,sha256:String)->Result<Archive>{
    let mut f=OpenOptions::new().read(true).share_mode(1).open(path).map_err(ioerr)?;let len=f.metadata().map_err(ioerr)?.len();
    if len%512!=0||len<=34849*512||len/512>u32::MAX as u64{return Err(fail("BACKUP_FORMAT","Expected full sector-aligned raw disk image"));}
    let mut a=vec![0;34*512];let mut b=vec![0;33*512];f.read_exact(&mut a).map_err(err)?;
    if &a[..4]==b"RKFW"||&a[..4]==b"RKAF"||a[..4]==[0x3a,0xff,0x26,0xed]{return Err(fail("BACKUP_FORMAT","Firmware package/sparse image is not a full raw disk dump"));}
    f.seek(SeekFrom::End(-(33*512))).map_err(err)?;f.read_exact(&mut b).map_err(err)?;
    crate::workflow::require_gpt_bytes(&a[512..])?;crate::workflow::require_gpt_bytes(&b)?;
    let sectors=(len/512) as u32;let meta=Meta{format:3,kind:"FULL".into(),identity:Identity{sectors,storage:"emmc".into(),chip_hex:String::new(),flash_id:String::new()},ranges:vec![Range{label:"Full eMMC user area".into(),lba:0,bytes:len,sha256}],os:classify(&a,&b,sectors),primary:a,tail:b,created:timestamp(),read_capability_hex:String::new(),usb_reread_verified:false};
    validate(&meta)?;Ok(Archive{file:f,meta,payload:len,raw:true})
}
fn raw(path:&Path,notify:&mut impl FnMut(Progress))->Result<Archive>{
    let mut a=raw_headers(path,"0".repeat(64))?;
    a.file.seek(SeekFrom::Start(0)).map_err(err)?;let mut worker=HashWorker::new(a.payload)?;let mut done=0;
    while done<a.payload{let mut data=worker.buffer()?;let n=(a.payload-done).min(CHUNK as u64) as usize;
        a.file.read_exact(&mut data[..n]).map_err(err)?;worker.submit(Block{offset:done,len:n,data})?;
        done+=n as u64;notify(Progress::new("verify-backup-file",done,a.payload));}
    a.meta.ranges[0].sha256=worker.finish()?.hash;Ok(a)
}
pub fn import(base:&Path,p:&Path,notify:&mut impl FnMut(Progress))->Result<Value>{
    if p.extension().and_then(|s|s.to_str()).is_some_and(|s|s.eq_ignore_ascii_case("img")){
        let a=raw(p,notify)?;let id=format!("raw-{}",a.meta.ranges[0].sha256);let dir=base.join("data/backup-imports");fs::create_dir_all(&dir).map_err(ioerr)?;
        fs::write(dir.join(format!("{id}.json")),serde_json::to_vec(&json!({"path":fs::canonicalize(p).map_err(ioerr)?})).map_err(err)?).map_err(ioerr)?;
        let mut v=report(p,&a.meta)?;v["id"]=json!(id);v["display_name"]=json!(p.file_name().unwrap().to_string_lossy());return Ok(v);
    }
    let mut a=Archive::open(p)?;
    let managed=base.join("backup");let selected=fs::canonicalize(p).map_err(ioerr)?;
    if managed.is_dir()&&selected.parent()==Some(fs::canonicalize(&managed).map_err(ioerr)?.as_path()){
        let name=selected.file_name().unwrap().to_string_lossy();
        if valid_id(&name){let mut v=report(&managed.join(name.as_ref()),&a.meta)?;v["verified"]=json!(false);return Ok(v);}
    }
    // Selected external archives become portable, without changing the source.
    let (final_path,tmp,mut f)=reserve(base,&a.meta.kind)?;
    a.file.seek(SeekFrom::Start(0)).map_err(err)?;std::io::copy(&mut a.file,&mut f).map_err(ioerr)?;f.sync_all().map_err(ioerr)?;drop(f);
    let mut copy=Archive::open(&tmp)?;copy.verify(notify)?;drop(copy);fs::rename(tmp,&final_path).map_err(ioerr)?;
    report(&final_path,&a.meta)
}
pub fn catalog(base:&Path)->Result<Vec<Value>>{
    let dir=base.join("backup");let mut rows=vec![];
    if dir.exists(){for entry in fs::read_dir(dir).map_err(ioerr)?{let entry=entry.map_err(ioerr)?;let id=entry.file_name().to_string_lossy().into_owned();if !valid_id(&id)||id.starts_with("raw-"){continue;}
        // Listing checks bounded metadata, not 32 GB per file. Restore always
        // verifies every payload byte before the first write, not at preview.
        match Archive::open(&entry.path()){Ok(a)=>{let mut v=report(&entry.path(),&a.meta)?;v["verified"]=json!(false);rows.push(v);},Err(e)=>rows.push(json!({"id":id,"restorable":false,"error":e}))}
    }}
    let imports=base.join("data/backup-imports");
    if imports.exists(){for entry in fs::read_dir(imports).map_err(ioerr)?{
        let entry=entry.map_err(ioerr)?;let name=entry.file_name().to_string_lossy().into_owned();
        let Some(id)=name.strip_suffix(".json").filter(|id|id.starts_with("raw-")&&valid_id(id)) else{continue;};
        let row=(||->Result<Value>{let p=raw_path(base,id)?;let a=raw_headers(&p,id[4..].into())?;
            let mut v=report(&p,&a.meta)?;v["id"]=json!(id);v["display_name"]=json!(p.file_name().unwrap().to_string_lossy());v["verified"]=json!(false);Ok(v)})();
        rows.push(row.unwrap_or_else(|e|json!({"id":id,"restorable":false,"verified":false,"error":e})));
    }}
    rows.sort_by(|a,b|b["id"].as_str().cmp(&a["id"].as_str()));Ok(rows)
}

#[cfg(test)] pub(crate) mod tests{
    use super::*;
    use xz2::write::XzEncoder;
    fn sample()->(Vec<u8>,Meta){
        let bytes=vec![0x15;35000*512];let primary=bytes[..34*512].to_vec();let tail=bytes[bytes.len()-33*512..].to_vec();
        let mut list=ranges("FULL",&primary,&tail,35000).unwrap();list[0].sha256=crate::hash_bytes(&bytes);
        (bytes,Meta{format:3,kind:"FULL".into(),identity:Identity{storage:"emmc".into(),sectors:35000,chip_hex:"3566".into(),flash_id:"EMMC".into()},ranges:list,primary,tail,os:"unknown".into(),created:"260930-120000".into(),read_capability_hex:"3f07000000000000".into(),usb_reread_verified:true})
    }
    pub(crate) fn encode(path:&Path,bytes:&[u8],meta:&Meta){
        let mut f=File::create(path).unwrap();f.write_all(magic(meta.format).unwrap()).unwrap();
        let f=if meta.format==3{let mut e=XzEncoder::new(f,1);e.write_all(bytes).unwrap();e.finish().unwrap()}
        else{let mut e=fast_encoder(f,bytes.len() as u64).unwrap();for b in bytes.chunks(CHUNK){e.write_all(b).unwrap();}e.finish().unwrap()};
        append_footer(f,meta).unwrap();
    }
    #[test] fn archive_metadata_and_payload_guards(){
        let dir=std::env::temp_dir().join(format!("k11c-archive-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir(&dir).unwrap();let file=dir.join("test.k11cbackup");
        let (bytes,mut meta)=sample();
        for format in [3,4]{
        meta.format=format;encode(&file,&bytes,&meta);let mut a=Archive::open(&file).unwrap();a.verify(&mut |_|{}).unwrap();drop(a);
        let valid=fs::read(&file).unwrap();
        for variant in 0..5{
            let mut broken=valid.clone();let n=broken.len();match variant{0=>broken[8]^=1,1=>broken[n-48]^=1,2=>{broken.truncate(n-1);},3=>{broken[0]^=1;},_=>{broken[n-16..n-8].copy_from_slice(&u64::MAX.to_le_bytes());}}
            fs::write(&file,&broken).unwrap();assert!(Archive::open(&file).and_then(|mut a|a.verify(&mut |_|{})).is_err(),"variant {variant}");
        }
        let mut bad=meta.clone();bad.ranges[0].bytes-=512;assert!(validate(&bad).is_err());
        bad=meta.clone();bad.ranges[0].lba=1;assert!(validate(&bad).is_err());
        bad=meta.clone();bad.os="HAOS".into();assert!(validate(&bad).is_err());
        bad=meta.clone();bad.primary[0]^=1;encode(&file,&bytes,&bad);assert!(Archive::open(&file).unwrap().verify(&mut |_|{}).is_err());
        bad=meta.clone();bad.ranges[0].sha256="0".repeat(64);encode(&file,&bytes,&bad);assert!(Archive::open(&file).unwrap().verify(&mut |_|{}).is_err());
        let mut more=bytes.clone();more.push(1);encode(&file,&more,&meta);assert!(Archive::open(&file).unwrap().verify(&mut |_|{}).is_err());
        }
        for id in ["../FULL-x.k11cbackup","FULL-x/../bad.k11cbackup","raw-xyz","BOOT-x.partial",""]{assert!(!valid_id(id));}
        fs::remove_file(file).unwrap();fs::remove_dir(dir).unwrap();
    }

    #[test] fn archive_zstd_framing_and_memory_limits(){
        let dir=std::env::temp_dir().join(format!("k11c-codec-{}",std::process::id()));fs::create_dir_all(&dir).unwrap();let file=dir.join("archive.k11cbackup");
        let (bytes,mut meta)=sample();meta.format=4;encode(&file,&bytes,&meta);
        let original=fs::read(&file).unwrap();let a=Archive::open(&file).unwrap();let end=8+a.payload as usize;drop(a);
        for suffix in [vec![0],zstd::stream::encode_all(&[][..],1).unwrap()]{
            let mut changed=original[..end].to_vec();changed.extend(suffix);changed.extend_from_slice(&original[end..]);fs::write(&file,changed).unwrap();
            assert!(Archive::open(&file).unwrap().verify(&mut |_|{}).is_err(),"Trailing bytes or a second empty frame must not be accepted");
        }
        let mut truncated=original.clone();truncated.remove(end-1);fs::write(&file,truncated).unwrap();assert!(Archive::open(&file).unwrap().verify(&mut |_|{}).is_err());
        // Metadata hash is valid, but its format cannot contradict the header.
        let mut f=File::create(&file).unwrap();f.write_all(&original[..end]).unwrap();let mut wrong=meta.clone();wrong.format=3;append_footer(f,&wrong).unwrap();
        let mut changed=fs::read(&file).unwrap();let n=changed.len();changed[n-8..].copy_from_slice(MAGIC_ZSTD);fs::write(&file,changed).unwrap();assert!(Archive::open(&file).is_err());
        // A valid stream requiring a 256-MiB window exceeds the 128-MiB read cap.
        let mut f=File::create(&file).unwrap();f.write_all(MAGIC_ZSTD).unwrap();let mut encoder=zstd::stream::write::Encoder::new(f,1).unwrap();encoder.window_log(28).unwrap();
        for b in bytes.chunks(CHUNK){encoder.write_all(b).unwrap();}append_footer(encoder.finish().unwrap(),&meta).unwrap();
        assert!(Archive::open(&file).unwrap().verify(&mut |_|{}).is_err(),"Oversized decoder window must be rejected");
        // The actual writer propagates finalization errors, including short input.
        let mut encoder=fast_encoder(Vec::new(),2).unwrap();encoder.write_all(&[1]).unwrap();assert!(encoder.finish().is_err());
        fs::remove_file(file).unwrap();fs::remove_dir(dir).unwrap();
    }
}
