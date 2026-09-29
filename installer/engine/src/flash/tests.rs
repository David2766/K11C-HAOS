use super::*;
use std::time::{SystemTime,UNIX_EPOCH};
struct Sandbox(PathBuf);
impl Sandbox {
    fn new()->Self{
        let p=std::env::temp_dir().join(format!("k11c-flash-test-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(p.join("resources/firmware")).unwrap();
        fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/firmware").join(FIRMWARE),p.join("resources/firmware").join(FIRMWARE)).unwrap();Self(p)
    }
    fn image(&self)->String{let b=crate::images::tests::fixture();let id=hash_bytes(b);let dir=self.0.join("data/images/prepared");fs::create_dir_all(&dir).unwrap();fs::write(dir.join(format!("{id}.img")),b).unwrap();id}
}
impl Drop for Sandbox{fn drop(&mut self){fs::remove_dir_all(&self.0).unwrap();}}
struct Fake{disk:Vec<u8>,writes:Vec<(u32,usize)>,short:bool,corrupt:bool,fail_at:Option<usize>,change_identity:bool,sd:bool}
impl Fake {
    fn new()->Self{Self{disk:vec![0xa5;73728*512],writes:vec![],short:false,corrupt:false,fail_at:None,change_identity:false,sd:false}}
    fn healthy()->Self{
        let mut io=Self::new();let b=crate::images::tests::fixture();
        let (a,t)=gpt::relocated(&b[..34*512],&b[b.len()-33*512..],(b.len()/512) as u32,73728).unwrap();
        io.disk[..a.len()].copy_from_slice(&a);let n=io.disk.len();io.disk[n-t.len()..].copy_from_slice(&t);io
    }
}
impl UsbIo for Fake{
    async fn identity(&mut self)->Result<Identity>{Ok(Identity{sectors:(self.disk.len()/512) as u32,storage:if self.sd{"sd0"}else{"emmc"}.into(),chip_hex:if self.change_identity&&!self.writes.is_empty(){"changed"}else{"3566"}.into(),flash_id:"EMMC".into()})}
    async fn read(&mut self,lba:u32,b:&mut[u8])->Result<u32>{let off=lba as usize*512;if off+b.len()>self.disk.len(){return Err(fail("USB_READ","bounds"));}b.copy_from_slice(&self.disk[off..off+b.len()]);Ok(b.len() as u32)}
    async fn upload(&mut self,_:u16,_:&[u8])->Result<()>{panic!("Flash operations must not upload/reset the device")}
}
impl FlashIo for Fake{
    async fn write(&mut self,lba:u32,b:&[u8])->Result<u32>{
        if self.fail_at==Some(self.writes.len()){return Err(fail("USB_WRITE","disconnect"));}
        self.writes.push((lba,b.len()));let start=lba as usize*512;self.disk[start..start+b.len()].copy_from_slice(b);
        if self.corrupt{self.disk[start]^=1;}Ok((b.len()-if self.short{512}else{0}) as u32)
    }
}
fn device()->Device{Device{instance_id:"K11C-TEST".into(),vid:0x2207,pid:0x350a,mode:"Loader".into(),binding:true,location:"PORT1".into(),interface_path:None}}
async fn make(io:&mut Fake,s:&Sandbox,op:&str,source:&str)->String{plan(io,&s.0,&device(),op,source,&mut |_|{}).await.unwrap()["plan_id"].as_str().unwrap().into()}
async fn run(io:&mut Fake,s:&Sandbox,id:&str)->Result<Value>{execute(io,&s.0,&device(),id,true,&mut |_|{}).await}
#[tokio::test] async fn install_preserves_every_official_payload_and_identity(){
    let s=Sandbox::new();let image=s.image();let mut io=Fake::new();let id=make(&mut io,&s,"install",&image).await;assert!(io.writes.is_empty());
    let out=run(&mut io,&s,&id).await.unwrap();assert_eq!(out["verified"],true);assert_eq!(out["gpt"]["healthy"],true);
    let source=crate::images::tests::fixture();let a=&io.disk[..34*512];
    for i in 0..8{
        let e=&source[1024+i*128..1024+(i+1)*128];let target=&a[1024+i*128..1024+(i+1)*128];
        assert_eq!(&e[..32],&target[..32]);assert_eq!(&e[48..],&target[48..]);
        let st=gpt::u64at(e,32) as usize;let en=gpt::u64at(e,40) as usize+1;
        assert_eq!(gpt::u64at(target,32),st as u64+32768);assert_eq!(gpt::u64at(target,40),en as u64-1+32768);
        assert_eq!(&source[st*512..en*512],&io.disk[(st+32768)*512..(en+32768)*512]);
    }
    assert_eq!(&io.disk[64*512..64*512+9687040],&firmware(&s.0).unwrap());
    let old=(source.len()/512+32768-33)*512;assert!(io.disk[old..old+33*512].iter().all(|b|*b==0));
    assert_eq!(gpt::u32at(a,458),73727);
    let count=io.writes.len();assert!(run(&mut io,&s,&id).await.is_err());assert_eq!(io.writes.len(),count,"one-use plan");
}
#[tokio::test] async fn uboot_update_and_restore_are_range_exact(){
    let s=Sandbox::new();let mut io=Fake::healthy();let original=io.disk.clone();let id=make(&mut io,&s,"uboot","").await;
    let p=load_plan(&s.0,&id).unwrap();run(&mut io,&s,&id).await.unwrap();
    assert_eq!(&io.disk[..64*512],&original[..64*512]);assert_eq!(&io.disk[64*512+9687040..],&original[64*512+9687040..]);
    let restore=make(&mut io,&s,"restore",&p.backup).await;run(&mut io,&s,&restore).await.unwrap();assert_eq!(io.disk,original);
}
#[tokio::test] async fn gpt_repair_only_changes_broken_copy(){
    let s=Sandbox::new();let mut io=Fake::healthy();let end=io.disk.len();io.disk[end-512+56]^=1;gpt::seal(&mut io.disk[end-512..]);
    let before=io.disk.clone();let id=make(&mut io,&s,"gpt-repair","").await;run(&mut io,&s,&id).await.unwrap();
    assert_eq!(&io.disk[..end-33*512],&before[..end-33*512]);assert!(io.writes.iter().all(|(l,_)|*l>=73728-33));
    assert_eq!(plan(&mut io,&s.0,&device(),"gpt-repair","",&mut |_|{}).await.unwrap()["no_changes"],true);
    io.disk[512+16]^=1;let id=make(&mut io,&s,"gpt-repair","").await;run(&mut io,&s,&id).await.unwrap();
    assert_eq!(check(&mut io).await.unwrap()["gpt"]["healthy"],true);
}
#[tokio::test] async fn missing_confirmation_changed_device_and_snapshot_block_all_writes(){
    let s=Sandbox::new();let mut io=Fake::healthy();let id=make(&mut io,&s,"uboot","").await;
    assert_eq!(execute(&mut io,&s.0,&device(),&id,false,&mut |_|{}).await.unwrap_err().code,"CONFIRM_WRITE");
    let mut d=device();d.location="PORT2".into();assert_eq!(execute(&mut io,&s.0,&d,&id,true,&mut |_|{}).await.unwrap_err().code,"DEVICE_CHANGED");
    io.sd=true;assert!(run(&mut io,&s,&id).await.is_err());io.sd=false;
    io.disk[0]^=1;assert_eq!(run(&mut io,&s,&id).await.unwrap_err().code,"DEVICE_CHANGED");assert!(io.writes.is_empty());
}
#[tokio::test] async fn source_plan_backup_and_firmware_tampering_block_all_writes(){
    for kind in 0..4{
        let s=Sandbox::new();let image=s.image();let mut io=Fake::healthy();let id=make(&mut io,&s,"install",&image).await;
        match kind{
            0=>{let p=s.0.join("data/images/prepared").join(format!("{image}.img"));let mut b=fs::read(&p).unwrap();b[100000]^=1;fs::write(p,b).unwrap();},
            1=>{let p=s.0.join("data/plans").join(format!("{id}.json"));let mut b=fs::read(&p).unwrap();b.push(b' ');fs::write(p,b).unwrap();},
            2=>{let p=load_plan(&s.0,&id).unwrap();fs::write(s.0.join("data/backups").join(p.backup).join("gpt-primary.bin"),b"bad").unwrap();},
            _=>{fs::write(s.0.join("resources/firmware").join(FIRMWARE),b"bad").unwrap();}
        }
        assert!(run(&mut io,&s,&id).await.is_err(),"kind={kind}");assert!(io.writes.is_empty());
    }
}
#[tokio::test] async fn short_corrupt_disconnected_and_changed_identity_never_succeed(){
    for kind in 0..4{
        let s=Sandbox::new();let mut io=Fake::healthy();let id=make(&mut io,&s,"uboot","").await;
        io.short=kind==0;io.corrupt=kind==1;io.fail_at=if kind==2{Some(1)}else{None};io.change_identity=kind==3;
        assert!(run(&mut io,&s,&id).await.is_err());
        let log=fs::read_to_string(s.0.join("data/plans").join(format!("{id}.journal.jsonl"))).unwrap();
        assert!(log.contains("\"failed\""));assert!(!log.contains("\"complete\""));
        assert!(s.0.join("data/plans").join(format!("{id}.used.json")).exists());
    }
}
#[tokio::test] async fn modified_partition_layout_and_ambiguous_gpt_are_rejected(){
    let s=Sandbox::new();let mut io=Fake::healthy();let id=make(&mut io,&s,"uboot","").await;let backup=load_plan(&s.0,&id).unwrap().backup;
    let end=io.disk.len();io.disk[1024+16]^=1;let ec=gpt::crc(&io.disk[1024..34*512]);gpt::put32(&mut io.disk,512+88,ec);gpt::seal(&mut io.disk[512..1024]);
    assert_eq!(plan(&mut io,&s.0,&device(),"gpt-repair","",&mut |_|{}).await.unwrap_err().code,"GPT_AMBIGUOUS");
    let entries=io.disk[1024..34*512].to_vec();io.disk[end-33*512..end-512].copy_from_slice(&entries);gpt::put32(&mut io.disk,end-512+88,ec);gpt::seal(&mut io.disk[end-512..]);
    assert_eq!(plan(&mut io,&s.0,&device(),"restore",&backup,&mut |_|{}).await.unwrap_err().code,"RESTORE_LAYOUT");assert!(io.writes.is_empty());
}
#[test] fn bounds_ids_and_locked_source(){
    for bad in ["../x","","UPPERCASE","a/b"]{assert!(!valid_id(bad));assert!(!backup_id(bad));}
    let r=Range{label:"test".into(),lba:50,bytes:512,sha256:String::new()};
    assert!(validate_ranges(&[r.clone()],50).is_err());assert!(validate_ranges(&[r.clone(),r.clone()],100).is_err());
    let mut bad=r;bad.bytes=513;assert!(validate_ranges(&[bad],100).is_err());
    let s=Sandbox::new();let id=s.image();let path=s.0.join("data/images/prepared").join(format!("{id}.img"));let _held=locked(&path).unwrap();
    assert!(OpenOptions::new().write(true).open(&path).is_err());assert!(fs::remove_file(&path).is_err());
}
#[tokio::test] async fn invalid_firmware_capacity_and_backup_schema_fail_before_writes(){
    let s=Sandbox::new();let source=s.image();let mut io=Fake::new();io.disk.truncate(50000*512);
    assert_eq!(plan(&mut io,&s.0,&device(),"install",&source,&mut |_|{}).await.unwrap_err().code,"CAPACITY");assert!(io.writes.is_empty());
    let mut io=Fake::healthy();let id=make(&mut io,&s,"uboot","").await;
    let p=load_plan(&s.0,&id).unwrap();let manifest=s.0.join("data/backups").join(&p.backup).join("manifest.json");
    let mut m:Value=serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();m["files"][0]["lba"]=1.into();fs::write(&manifest,serde_json::to_vec(&m).unwrap()).unwrap();
    assert_eq!(run(&mut io,&s,&id).await.unwrap_err().code,"BACKUP_VERIFY");assert!(io.writes.is_empty());
    fs::write(s.0.join("resources/firmware").join(FIRMWARE),vec![0;512]).unwrap();
    assert_eq!(plan(&mut io,&s.0,&device(),"uboot","",&mut |_|{}).await.unwrap_err().code,"FIRMWARE_HASH");assert!(io.writes.is_empty());
}

struct DiskFile {file:File,sectors:u32,writes:Vec<(u32,usize)>}
impl UsbIo for DiskFile {
    async fn identity(&mut self)->Result<Identity>{Ok(Identity{sectors:self.sectors,storage:"emmc".into(),chip_hex:"3566".into(),flash_id:"FILE-TEST".into()})}
    async fn read(&mut self,lba:u32,b:&mut[u8])->Result<u32>{self.file.seek(SeekFrom::Start(lba as u64*512)).map_err(disk_io)?;self.file.read_exact(b).map_err(disk_io)?;Ok(b.len() as u32)}
    async fn upload(&mut self,_:u16,_:&[u8])->Result<()>{panic!("No upload in a disk-file transaction")}
}
impl FlashIo for DiskFile {
    async fn write(&mut self,lba:u32,b:&[u8])->Result<u32>{self.writes.push((lba,b.len()));self.file.seek(SeekFrom::Start(lba as u64*512)).map_err(disk_io)?;self.file.write_all(b).and_then(|_|self.file.sync_data()).map_err(disk_io)?;Ok(b.len() as u32)}
}
/// Opt-in integration test: use the real downloaded official HAOS image and the
/// real firmware, replacing only USB transport with an ordinary temporary file.
#[tokio::test] #[ignore="Requires K11C_TEST_OFFICIAL_IMAGE; writes only temporary PC files"]
async fn actual_official_image_full_transaction(){
    let path=PathBuf::from(std::env::var("K11C_TEST_OFFICIAL_IMAGE").expect("set official image path"));
    let s=Sandbox::new();let mut input=locked(&path).unwrap();let size=input.metadata().unwrap().len();let id=hash_range(&mut input,0,size).unwrap();
    let dir=s.0.join("data/images/prepared");fs::create_dir_all(&dir).unwrap();fs::copy(&path,dir.join(format!("{id}.img"))).unwrap();
    let disk=OpenOptions::new().create_new(true).read(true).write(true).open(s.0.join("fake-emmc.img")).unwrap();
    let sectors=(size/512) as u32+32768+4096;disk.set_len(sectors as u64*512).unwrap();
    let mut io=DiskFile{file:disk,sectors,writes:vec![]};
    let p=plan(&mut io,&s.0,&device(),"install",&id,&mut |_|{}).await.unwrap();assert!(io.writes.is_empty());
    let result=execute(&mut io,&s.0,&device(),p["plan_id"].as_str().unwrap(),true,&mut |_|{}).await.unwrap();assert_eq!(result["verified"],true);
    let mut source_header=vec![0;34*512];input.seek(SeekFrom::Start(0)).unwrap();input.read_exact(&mut source_header).unwrap();
    let mut target_header=vec![0;34*512];io.read(0,&mut target_header).await.unwrap();
    for i in 0..8{
        let e=&source_header[1024+i*128..1024+(i+1)*128];let t=&target_header[1024+i*128..1024+(i+1)*128];
        assert_eq!(&e[..32],&t[..32]);assert_eq!(&e[48..],&t[48..]);
        let source_start=gpt::u64at(e,32);let len=(gpt::u64at(e,40)+1-source_start)*512;
        assert_eq!(gpt::u64at(t,32),source_start+32768);
        let source_hash=hash_range(&mut input,source_start*512,len).unwrap();let target_hash=hash_range(&mut io.file,gpt::u64at(t,32)*512,len).unwrap();assert_eq!(source_hash,target_hash,"partition {}",i+1);
        println!("OFFICIAL_PARTITION_{}_IDENTICAL sha256={source_hash}",i+1);
    }
    assert_eq!(check(&mut io).await.unwrap()["gpt"]["healthy"],true);
    println!("OFFICIAL_INSTALL_PASS image_sha256={id} firmware={FIRMWARE_SHA256} writes={} target_sectors={sectors}",io.writes.len());
    drop(io);
}
