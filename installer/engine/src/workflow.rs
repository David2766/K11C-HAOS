use crate::{Result, fail};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{path::{Path,PathBuf}, fs::{self,OpenOptions},io::{Read,Write},time::{SystemTime,UNIX_EPOCH}};

pub const FIRST_PARTITION:u32=34816;
pub const CHUNK:usize=128*512;
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct Identity {pub sectors:u32,pub storage:String,pub chip_hex:String,pub flash_id:String}
#[derive(Debug,Clone,Serialize)]
pub struct Progress {pub phase:String,pub completed:u64,pub total:u64}
impl Progress {pub fn new(phase:&str,completed:u64,total:u64)->Self{Self{phase:phase.into(),completed,total}}}
// Read/upload operations cannot write eMMC. Confirmed transactions additionally
// require the separate flash::FlashIo boundary.
#[allow(async_fn_in_trait)]
pub trait UsbIo {
    async fn identity(&mut self)->Result<Identity>;
    async fn capability(&mut self)->Result<[u8;8]>;
    async fn read(&mut self,lba:u32,bytes:&mut [u8])->Result<u32>;
    async fn upload(&mut self,area:u16,bytes:&[u8])->Result<()>;
}
pub async fn require_full_read(io:&mut impl UsbIo)->Result<String> {
    let caps=io.capability().await?;
    if caps[0]&0x08==0 {return Err(fail("LOADER_READ_RESTRICTED","Read LBA On is disabled. Power-cycle into MASKROM and prepare with this Installer; no storage operation was started"));}
    Ok(caps.iter().map(|b|format!("{b:02x}")).collect())
}
// A complete 0xCC GPT response matches the vendor read-denial sentinel. Do not
// classify it as damaged GPT or authorize a repair, even if repeated reads agree.
// Other damaged/blank GPT bytes remain eligible for a raw recovery backup.
pub fn require_gpt_bytes(bytes:&[u8])->Result<()> {
    if !bytes.is_empty() && bytes.iter().all(|b|*b==0xcc) {
        return Err(fail("USB_READ_UNTRUSTED","GPT read contains only the vendor 0xCC sentinel; actual disk contents are not established"));
    }
    Ok(())
}
pub async fn read_exact(io:&mut impl UsbIo,lba:u32,buf:&mut [u8])->Result<()> {
    let n=io.read(lba,buf).await?;
    if n as usize != buf.len() {return Err(fail("SHORT_READ",format!("LBA {lba}: {n}/{} bytes",buf.len())));}
    Ok(())
}
#[derive(Debug,Serialize,Deserialize)]
pub struct Part {pub file:String,pub lba:u32,pub sectors:u32,pub bytes:u64,pub sha256:String}
fn plan(sectors:u32)->Result<Vec<Part>> {
    if sectors<=FIRST_PARTITION+33 {return Err(fail("INVALID_CAPACITY",sectors));}
    Ok([("gpt-primary.bin",0,34),("reserved-gap.bin",34,30),("uboot-reserved.bin",64,FIRST_PARTITION-64),("gpt-backup.bin",sectors-33,33)]
        .map(|(f,l,n)|Part{file:f.into(),lba:l,sectors:n,bytes:n as u64*512,sha256:String::new()}).into())
}
fn new_directory(base:&Path)->Result<PathBuf> {
    fs::create_dir_all(base).map_err(|e|fail("BACKUP_IO",e))?;
    let ns=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    for suffix in 0..100 {
        let p=base.join(format!("k11c-{ns}-{}-{suffix}.partial",std::process::id()));
        match fs::create_dir(&p) {Ok(())=>return Ok(p),Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,Err(e)=>return Err(fail("BACKUP_IO",e))}
    }
    Err(fail("BACKUP_IO","Could not reserve a new backup directory"))
}
fn save_new(path:&Path,bytes:&[u8])->Result<()> {
    let mut f=OpenOptions::new().write(true).create_new(true).open(path).map_err(|e|fail("BACKUP_IO",e))?;
    f.write_all(bytes).and_then(|_|f.sync_all()).map_err(|e|fail("BACKUP_IO",e))
}
pub fn verify_saved(dir:&Path,parts:&[Part])->Result<()> {
    for part in parts {
        let mut f=fs::File::open(dir.join(&part.file)).map_err(|e|fail("BACKUP_VERIFY",e))?;
        let len=f.metadata().map_err(|e|fail("BACKUP_VERIFY",e))?.len();
        let mut h=Sha256::new();let mut b=vec![0u8;CHUNK];
        loop {let n=f.read(&mut b).map_err(|e|fail("BACKUP_VERIFY",e))?;if n==0{break;}h.update(&b[..n]);}
        let hash=format!("{:x}",h.finalize());
        if len != part.bytes || hash != part.sha256 {return Err(fail("BACKUP_VERIFY",&part.file));}
    }
    Ok(())
}
pub async fn backup(io:&mut impl UsbIo,base:&Path,device:&crate::usb::Device,progress:&mut impl FnMut(Progress))->Result<Value> {
    crate::usb::guard(device)?;
    let identity=io.identity().await?;crate::usb::require_emmc(&identity.storage)?;
    let capabilities=require_full_read(io).await?;
    let mut parts=plan(identity.sectors)?;
    let dir=new_directory(base)?;
    save_new(&dir.join("incomplete.json"),&serde_json::to_vec_pretty(&json!({"complete":false,"device":device,"identity":identity})).unwrap())?;
    let run:Result<Value>=async {
        let total:u64=parts.iter().map(|p|p.bytes).sum();let mut done=0;
        let mut buf=vec![0u8;CHUNK];
        for part in &mut parts {
            let mut f=OpenOptions::new().write(true).create_new(true).open(dir.join(&part.file)).map_err(|e|fail("BACKUP_IO",e))?;
            let mut hash=Sha256::new();let mut offset=0;
            while offset<part.bytes {
                let n=((part.bytes-offset) as usize).min(CHUNK);
                read_exact(io,part.lba+(offset/512) as u32,&mut buf[..n]).await?;
                f.write_all(&buf[..n]).map_err(|e|fail("BACKUP_IO",e))?;hash.update(&buf[..n]);
                offset+=n as u64;done+=n as u64;progress(Progress::new("read",done,total));
            }
            f.sync_all().map_err(|e|fail("BACKUP_IO",e))?;
            part.sha256=format!("{:x}",hash.finalize());
        }
        done=0;
        for part in &parts {
            let mut hash=Sha256::new();let mut offset=0;
            while offset<part.bytes {
                let n=((part.bytes-offset) as usize).min(CHUNK);
                read_exact(io,part.lba+(offset/512) as u32,&mut buf[..n]).await?;
                hash.update(&buf[..n]);offset+=n as u64;done+=n as u64;progress(Progress::new("verify",done,total));
            }
            if format!("{:x}",hash.finalize()) != part.sha256 {return Err(fail("DEVICE_CHANGED","Repeated USB reads differ"));}
        }
        verify_saved(&dir,&parts)?;
        let after=io.identity().await?;
        if after != identity {return Err(fail("DEVICE_CHANGED","Device identity/storage changed during backup"));}
        let primary=fs::read(dir.join("gpt-primary.bin")).map_err(|e|fail("BACKUP_IO",e))?;
        let tail=fs::read(dir.join("gpt-backup.bin")).map_err(|e|fail("BACKUP_IO",e))?;
        require_gpt_bytes(&primary[512..])?;require_gpt_bytes(&tail)?;
        require_full_read(io).await?;
        let gpt=crate::gpt::check(&primary,&tail,identity.sectors);
        let manifest=json!({"format":2,"complete":true,"kind":"k11c-boot-and-gpt","identity":identity,"device":device,"read_capability_hex":capabilities,
            "files":parts,"gpt":gpt,"usb_reread_verified":true,"user_data_included":false});
        save_new(&dir.join("manifest.json"),&serde_json::to_vec_pretty(&manifest).unwrap())?;
        let hashes=parts.iter().map(|p|format!("{}  {}\n",p.sha256,p.file)).collect::<String>();
        save_new(&dir.join("SHA256SUMS"),hashes.as_bytes())?;
        fs::remove_file(dir.join("incomplete.json")).map_err(|e|fail("BACKUP_IO",e))?;
        let finished=dir.with_extension("");
        fs::rename(&dir,&finished).map_err(|e|fail("BACKUP_IO",e))?;
        progress(Progress::new("complete",total,total));
        Ok(json!({"path":finished,"bytes":total,"gpt":gpt,"verified":true,"files":parts}))
    }.await;
    run.map_err(|e|fail(&e.code,format!("{}; incomplete backup: {}",e.detail,dir.display())))
}

#[cfg(test)] pub(crate) mod tests {
    use super::*;
    pub struct Fake {pub uploads:Vec<(u16,usize)>,pub upload_hashes:Vec<String>,pub fail_upload:bool,reads:usize,short:bool,disconnect:bool,changed:bool,identity_change:bool,queries:u32,restricted:bool,cc_tail:bool,lose_capability:bool,cap_queries:u32}
    impl Fake {pub fn new()->Self{Self{uploads:vec![],upload_hashes:vec![],fail_upload:false,reads:0,short:false,disconnect:false,changed:false,identity_change:false,queries:0,restricted:false,cc_tail:false,lose_capability:false,cap_queries:0}}}
    impl UsbIo for Fake {
        async fn identity(&mut self)->Result<Identity>{self.queries+=1;Ok(Identity{sectors:65536+if self.identity_change&&self.queries>1{1}else{0},storage:"emmc".into(),chip_hex:"rk3566".into(),flash_id:"EMMC ".into()})}
        async fn capability(&mut self)->Result<[u8;8]>{self.cap_queries+=1;Ok([if self.restricted||(self.lose_capability&&self.cap_queries>1){0x37}else{0x3f},7,0,0,0,0,0,0])}
        async fn read(&mut self,lba:u32,buf:&mut[u8])->Result<u32>{
            if self.disconnect&&self.reads>3{return Err(fail("USB_READ","disconnected"));}
            for (i,b) in buf.iter_mut().enumerate(){*b=((lba as usize+i/512+i)%251) as u8;}
            if self.cc_tail&&lba==65536-33{buf.fill(0xcc);}
            // Initial pass has 275 reads: 1+1+272+1.
            if self.changed&&self.reads>=275{buf[0]^=1;}
            self.reads+=1;Ok((buf.len()-if self.short{512}else{0}) as u32)
        }
        async fn upload(&mut self,area:u16,b:&[u8])->Result<()>{self.uploads.push((area,b.len()));self.upload_hashes.push(crate::hash_bytes(b));if self.fail_upload{Err(fail("USB_UPLOAD","failure"))}else{Ok(())}}
    }
    fn device()->crate::usb::Device{crate::usb::Device{instance_id:"test".into(),vid:0x2207,pid:0x350a,mode:"Loader".into(),binding:true,location:"port1".into(),interface_path:None}}
    fn temp()->PathBuf{let p=std::env::temp_dir().join(format!("k11c-backup-test-{}",SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir(&p).unwrap();p}
    #[tokio::test] async fn backup_roundtrip_and_tamper(){
        let root=temp();let mut io=Fake::new();let out=backup(&mut io,&root,&device(),&mut |_|{}).await.unwrap();
        let dir=PathBuf::from(out["path"].as_str().unwrap());assert!(!dir.to_string_lossy().ends_with(".partial"));
        let manifest:Value=serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["complete"],true);assert_eq!(manifest["user_data_included"],false);assert_eq!(io.queries,2);assert!(io.uploads.is_empty());
        let parts:Vec<Part>=serde_json::from_value(manifest["files"].clone()).unwrap();
        verify_saved(&dir,&parts).unwrap();
        let target=dir.join("gpt-primary.bin");let orig=fs::read(&target).unwrap();let mut corrupt=orig.clone();corrupt[1]^=1;fs::write(&target,corrupt).unwrap();assert!(verify_saved(&dir,&parts).is_err());
        fs::write(target,&orig[..512]).unwrap();assert!(verify_saved(&dir,&parts).is_err());
        let again=backup(&mut Fake::new(),&root,&device(),&mut |_|{}).await.unwrap();assert_ne!(out["path"],again["path"]);
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test] async fn failed_backups_never_publish(){
        for kind in 0..4 {
            let root=temp();let mut io=Fake::new();io.short=kind==0;io.disconnect=kind==1;io.changed=kind==2;io.identity_change=kind==3;
            let result=backup(&mut io,&root,&device(),&mut |_|{}).await;assert!(result.is_err(),"kind={kind}");
            for entry in fs::read_dir(&root).unwrap(){let p=entry.unwrap().path();assert_eq!(p.extension().unwrap(),"partial");assert!(!p.join("manifest.json").exists());}
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test] async fn restricted_or_identical_cc_reads_never_publish_a_backup(){
        for restricted in [true,false] {
            let root=temp();let mut io=Fake::new();io.restricted=restricted;io.cc_tail=true;
            let mut completed=false;
            let err=backup(&mut io,&root,&device(),&mut |p|{completed|=p.phase=="complete";}).await.unwrap_err();
            assert_eq!(err.code,if restricted{"LOADER_READ_RESTRICTED"}else{"USB_READ_UNTRUSTED"});
            assert!(!completed);assert!(io.uploads.is_empty());
            if restricted{assert_eq!(io.reads,0);}
            for entry in fs::read_dir(&root).unwrap(){let p=entry.unwrap().path();assert_eq!(p.extension().unwrap(),"partial");assert!(!p.join("manifest.json").exists());}
            fs::remove_dir_all(root).unwrap();
        }
        require_gpt_bytes(&vec![0;33*512]).unwrap();require_gpt_bytes(&vec![0xff;33*512]).unwrap();
    }
    #[tokio::test] async fn capability_loss_during_backup_never_publishes(){
        let root=temp();let mut io=Fake::new();io.lose_capability=true;
        let result=backup(&mut io,&root,&device(),&mut |_|{}).await;
        assert_eq!(result.unwrap_err().code,"LOADER_READ_RESTRICTED");
        for entry in fs::read_dir(&root).unwrap(){let p=entry.unwrap().path();assert_eq!(p.extension().unwrap(),"partial");assert!(!p.join("manifest.json").exists());}
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test] async fn wrong_mode_and_unwritable_destination(){
        let root=temp();let mut d=device();d.mode="Unknown".into();let mut io=Fake::new();
        assert!(backup(&mut io,&root,&d,&mut |_|{}).await.is_err());assert_eq!(io.queries,0);
        let p=root.join("not-a-directory");fs::write(&p,b"existing").unwrap();assert!(backup(&mut io,&p,&device(),&mut |_|{}).await.is_err());assert_eq!(fs::read(p).unwrap(),b"existing");fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test] async fn saved_file_tampering_is_rejected_by_the_transaction(){
        let root=temp();let mut changed=false;
        let out=backup(&mut Fake::new(),&root,&device(),&mut |p|{
            if p.phase=="verify" && p.completed==p.total && !changed {
                let dir=fs::read_dir(&root).unwrap().next().unwrap().unwrap().path();
                let file=dir.join("gpt-primary.bin");let mut bytes=fs::read(&file).unwrap();bytes[0]^=1;fs::write(file,bytes).unwrap();changed=true;
            }
        }).await;
        assert!(changed);assert_eq!(out.unwrap_err().code,"BACKUP_VERIFY");
        let dir=fs::read_dir(&root).unwrap().next().unwrap().unwrap().path();assert_eq!(dir.extension().unwrap(),"partial");assert!(!dir.join("manifest.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn ranges_cover_only_boot_area_and_end(){
        let p=plan(61071360).unwrap();assert_eq!(p.iter().map(|p|(p.lba,p.sectors)).collect::<Vec<_>>(),vec![(0,34),(34,30),(64,34752),(61071327,33)]);
        assert!(plan(34849).is_err());
    }
}
