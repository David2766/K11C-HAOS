use super::*;
use crate::archive;
use std::time::{SystemTime,UNIX_EPOCH};
#[tokio::test] async fn factory_raw_uses_real_transaction_and_no_backup(){
    let s=Sandbox::new();let path=s.0.join("manufacturer.img");let source=crate::factory::tests::disk(36864);fs::write(&path,&source).unwrap();
    let imported=crate::factory::import(&s.0,&path,&mut |_|{}).unwrap();let source_id=imported["id"].as_str().unwrap();let mut io=Fake::healthy();
    let p=direct(&mut io,&s,"factory-raw",source_id).await.unwrap();assert_eq!(p["backup_path"],Value::Null);assert_eq!(p["erases_user_data"],true);assert!(io.writes.is_empty());assert!(!s.0.join("backup").exists());
    let id=p["plan_id"].as_str().unwrap();assert_eq!(execute(&mut io,&s.0,&device(),id,false,&mut |_|{}).await.unwrap_err().code,"CONFIRM_WRITE");assert!(io.writes.is_empty());
    execute(&mut io,&s.0,&device(),id,true,&mut |_|{}).await.unwrap();assert_eq!(&io.disk[34*512..(36864-33)*512],&source[34*512..source.len()-33*512]);
    let n=io.disk.len();assert_eq!(gpt::check(&io.disk[..34*512],&io.disk[n-33*512..],73728)["healthy"],true);assert!(execute(&mut io,&s.0,&device(),id,true,&mut |_|{}).await.is_err());
    let mut fresh=Fake::healthy();let next=direct(&mut fresh,&s,"factory-raw",source_id).await.unwrap();assert_ne!(next["plan_id"],id);assert!(fresh.writes.is_empty());
}
#[tokio::test] async fn factory_mbr_transaction_preserves_full_payload_and_clears_old_gpt(){
    for sectors in [36864,73720,73728]{
        let s=Sandbox::new();let path=s.0.join("linux-mbr.img");let mut source=vec![0x73;sectors as usize*512];source[446..510].fill(0);source[450]=0x83;source[510..512].copy_from_slice(&[0x55,0xaa]);gpt::put32(&mut source,454,2048);gpt::put32(&mut source,458,sectors-2048);fs::write(&path,&source).unwrap();
        let imported=crate::factory::import(&s.0,&path,&mut |_|{}).unwrap();let mut io=Fake::healthy();let p=direct(&mut io,&s,"factory-raw",imported["id"].as_str().unwrap()).await.unwrap();assert!(!s.0.join("backup").exists());
        let id=p["plan_id"].as_str().unwrap();io.disk[64*512]^=1;assert_eq!(execute(&mut io,&s.0,&device(),id,true,&mut |_|{}).await.unwrap_err().code,"DEVICE_CHANGED");assert!(io.writes.is_empty());io.disk[64*512]^=1;
        execute(&mut io,&s.0,&device(),id,true,&mut |_|{}).await.unwrap();assert_eq!(&io.disk[..source.len()],source.as_slice());if sectors<73728{assert!(io.disk[(sectors.max(73728-33)) as usize*512..].iter().all(|b|*b==0));}
    }
}
#[tokio::test] async fn factory_raw_corrupt_readback_keeps_failed_journal(){
    let s=Sandbox::new();let path=s.0.join("linux.img");fs::write(&path,crate::factory::tests::disk(36864)).unwrap();let imported=crate::factory::import(&s.0,&path,&mut |_|{}).unwrap();let mut io=Fake::healthy();
    let p=direct(&mut io,&s,"factory-raw",imported["id"].as_str().unwrap()).await.unwrap();io.corrupt_at=Some(35000*512+511);let id=p["plan_id"].as_str().unwrap();assert_eq!(execute(&mut io,&s.0,&device(),id,true,&mut |_|{}).await.unwrap_err().code,"WRITE_VERIFY");
    assert!(s.0.join("data/plans").join(format!("{id}.used.json")).exists());let log=fs::read_to_string(s.0.join("data/plans").join(format!("{id}.journal.jsonl"))).unwrap();assert!(log.contains("failed"));assert!(!log.contains("\"state\":\"complete\""));
}
struct Sandbox(PathBuf);
impl Sandbox {
    fn new()->Self{
        let p=std::env::temp_dir().join(format!("k11c-flash-test-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(p.join("resources/firmware")).unwrap();
        fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/firmware").join(FIRMWARE),p.join("resources/firmware").join(FIRMWARE)).unwrap();Self(p)
    }
    fn image(&self)->String{let b=crate::images::tests::fixture();let id=hash_bytes(b);let dir=self.0.join("data/images/prepared");fs::create_dir_all(&dir).unwrap();fs::write(dir.join(format!("{id}.img")),b).unwrap();crate::components::tests::register_fixture(&self.0,&id);id}
}
impl Drop for Sandbox{fn drop(&mut self){fs::remove_dir_all(&self.0).unwrap();}}
struct Fake{disk:Vec<u8>,writes:Vec<(u32,usize)>,short:bool,corrupt:bool,corrupt_at:Option<usize>,fail_at:Option<usize>,change_identity:bool,sd:bool,restricted:bool,cc_reads:bool,cap_remaining:Option<u32>}
impl Fake {
    fn new()->Self{Self{disk:vec![0xa5;73728*512],writes:vec![],short:false,corrupt:false,corrupt_at:None,fail_at:None,change_identity:false,sd:false,restricted:false,cc_reads:false,cap_remaining:None}}
    fn healthy()->Self{
        let mut io=Self::new();let b=crate::images::tests::fixture();
        let (a,t)=gpt::relocated(&b[..34*512],&b[b.len()-33*512..],(b.len()/512) as u32,73728).unwrap();
        io.disk[..a.len()].copy_from_slice(&a);let n=io.disk.len();io.disk[n-t.len()..].copy_from_slice(&t);io
    }
}
impl UsbIo for Fake{
    async fn identity(&mut self)->Result<Identity>{Ok(Identity{sectors:(self.disk.len()/512) as u32,storage:if self.sd{"sd0"}else{"emmc"}.into(),chip_hex:if self.change_identity&&!self.writes.is_empty(){"changed"}else{"3566"}.into(),flash_id:"EMMC".into()})}
    async fn capability(&mut self)->Result<[u8;8]>{
        let blocked=self.restricted||self.cap_remaining==Some(0);
        if let Some(n)=&mut self.cap_remaining{*n=n.saturating_sub(1);}
        Ok([if blocked{0x37}else{0x3f},7,0,0,0,0,0,0])
    }
    async fn read(&mut self,lba:u32,b:&mut[u8])->Result<u32>{let off=lba as usize*512;if off+b.len()>self.disk.len(){return Err(fail("USB_READ","bounds"));}b.copy_from_slice(&self.disk[off..off+b.len()]);if self.cc_reads&&lba>=65536{b.fill(0xcc);}Ok(b.len() as u32)}
    async fn upload(&mut self,_:u16,_:&[u8])->Result<()>{panic!("Flash operations must not upload/reset the device")}
}
impl FlashIo for Fake{
    async fn write(&mut self,lba:u32,b:&[u8])->Result<u32>{
        if self.fail_at==Some(self.writes.len()){return Err(fail("USB_WRITE","disconnect"));}
        self.writes.push((lba,b.len()));let start=lba as usize*512;self.disk[start..start+b.len()].copy_from_slice(b);
        if self.corrupt{self.disk[start]^=1;}
        if let Some(at)=self.corrupt_at{if (start..start+b.len()).contains(&at){self.disk[at]^=1;}}
        Ok((b.len()-if self.short{512}else{0}) as u32)
    }
}
fn device()->Device{Device{instance_id:"K11C-TEST".into(),vid:0x2207,pid:0x350a,mode:"Loader".into(),binding:true,location:"PORT1".into(),interface_path:None}}
async fn archived(io:&mut Fake,s:&Sandbox,kind:&str)->String{archive::create(io,&s.0,kind,&mut |_|{}).await.unwrap()["id"].as_str().unwrap().into()}
async fn backed(io:&mut Fake,s:&Sandbox,op:&str,source:&str,recovery:&str)->Result<Value>{plan_backed(io,&s.0,&device(),op,source,recovery,&mut |_|{}).await}
async fn direct(io:&mut Fake,s:&Sandbox,op:&str,source:&str)->Result<Value>{plan_restore_direct(io,&s.0,&device(),op,source,&mut |_|{}).await}
struct Metered {
    inner: Fake, events: Vec<(char,u32,usize)>, owner: std::thread::ThreadId,
    read_fault: u8, late_damage: bool, delay_ms: u64,
}
impl Metered {
    fn new(inner:Fake)->Self{Self{inner,events:vec![],owner:std::thread::current().id(),read_fault:0,late_damage:false,delay_ms:0}}
    fn owner(&self){assert_eq!(self.owner,std::thread::current().id(),"Only the caller may use USB");}
}
impl UsbIo for Metered {
    async fn identity(&mut self)->Result<Identity>{self.owner();self.inner.identity().await}
    async fn capability(&mut self)->Result<[u8;8]>{self.owner();self.inner.capability().await}
    async fn upload(&mut self,_:u16,_:&[u8])->Result<()>{panic!("No upload during restore")}
    async fn read(&mut self,lba:u32,b:&mut[u8])->Result<u32>{
        self.owner();self.events.push(('r',lba,b.len()));
        if self.delay_ms>0{std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));}
        if !self.inner.writes.is_empty()&&self.read_fault==1{return Err(fail("USB_READ","disconnected during readback"));}
        let n=self.inner.read(lba,b).await?;
        Ok(if !self.inner.writes.is_empty()&&self.read_fault==2{n-512}else{n})
    }
}
impl FlashIo for Metered {
    async fn write(&mut self,lba:u32,b:&[u8])->Result<u32>{
        self.owner();self.events.push(('w',lba,b.len()));
        if self.delay_ms>0{std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));}
        let n=self.inner.write(lba,b).await?;
        if self.late_damage&&(lba as usize*512+b.len()==self.inner.disk.len()){
            self.inner.disk[CHUNK+517]^=1; // Earlier chunk damaged by the final write.
        }
        Ok(n)
    }
}
#[tokio::test] async fn pipeline_production_readback_faults_and_late_damage_never_complete(){
    for fault in 0..3{
        let s=Sandbox::new();let mut inner=Fake::new();let source=archived(&mut inner,&s,"FULL").await;
        let mut io=Metered::new(inner);let p=plan_restore_direct(&mut io,&s.0,&device(),"restore-archive",&source,&mut |_|{}).await.unwrap();
        io.late_damage=fault==0;io.read_fault=fault;io.events.clear();let mut phases=vec![];let id=p["plan_id"].as_str().unwrap();
        let error=execute(&mut io,&s.0,&device(),id,true,&mut |p|phases.push(p.phase)).await.unwrap_err();
        assert_eq!(error.code,["WRITE_VERIFY","USB_READ","SHORT_READ"][fault as usize]);
        assert!(!phases.contains(&"write-complete".to_string()));
        let log=fs::read_to_string(s.0.join("data/plans").join(format!("{id}.journal.jsonl"))).unwrap();
        assert!(!log.contains("\"state\":\"complete\""));assert!(log.contains("\"state\":\"failed\""));
        assert!(execute(&mut io,&s.0,&device(),id,true,&mut |_|{}).await.is_err());
        // Every write completed before readback starts, including fault cases.
        let first=io.events.iter().position(|e|e.0=='w').unwrap();
        assert!(io.events[first..first+36].iter().all(|e|e.0=='w'));
        assert!(io.events[first+36..].iter().all(|e|e.0=='r'));
    }
}
#[tokio::test] async fn pipeline_production_raw_order_lock_and_single_preflight_pass(){
    let s=Sandbox::new();let inner=Fake::new();let mut original=inner.disk.clone();
    for (i,b) in original.iter_mut().enumerate(){*b=((i/512+i/CHUNK*3)%251) as u8;}
    let path=s.0.join("full.img");fs::write(&path,&original).unwrap();
    let row=archive::import(&s.0,&path,&mut |_|{}).unwrap();let source=row["id"].as_str().unwrap();
    let mut bytes=0;let mut previous=0;
    let a=archive::open(&s.0,source,&mut |p|{
        assert_eq!(p.phase,"verify-backup-file");assert!(p.completed>previous);bytes+=p.completed-previous;previous=p.completed;
        assert!(OpenOptions::new().write(true).open(&path).is_err());
    }).unwrap();assert_eq!(bytes,original.len() as u64);drop(a);
    let mut io=Metered::new(inner);let p=plan_restore_direct(&mut io,&s.0,&device(),"restore-archive",source,&mut |_|{}).await.unwrap();
    io.events.clear();let mut phases=vec![];
    let out=execute(&mut io,&s.0,&device(),p["plan_id"].as_str().unwrap(),true,&mut |p|{
        if p.phase=="write"||p.phase=="verify-write"{assert!(OpenOptions::new().write(true).open(&path).is_err());}
        phases.push(p.phase);
    }).await.unwrap();
    assert_eq!(io.inner.disk,original);assert_eq!(phases.last().unwrap(),"write-complete");
    let first=io.events.iter().position(|e|e.0=='w').unwrap();
    for i in 0..36{assert_eq!(io.events[first+i],('w',(i*CHUNK/512) as u32,CHUNK));assert_eq!(io.events[first+36+i],('r',(i*CHUNK/512) as u32,CHUNK));}
    assert_eq!(out["timings"]["buffers"],3);assert_eq!(out["timings"]["chunk_bytes"],CHUNK);
    for name in ["write_s","readback_s","source_work_s","usb_write_s","readback_hash_s"]{assert!(out["timings"][name].as_f64().unwrap()>0.0);}
    assert!(out["timings"]["total_s"].as_f64().unwrap()>=out["timings"]["write_s"].as_f64().unwrap()+out["timings"]["readback_s"].as_f64().unwrap());
    assert!(OpenOptions::new().write(true).open(&path).is_ok(),"No worker retains a source lock after completion");
}
#[tokio::test]
#[ignore = "PC-only simulated transport comparison, not physical USB/eMMC speed"]
async fn pipeline_speed_report(){
    let mut reports=vec![];
    for format in [0,3,4]{
        let s=Sandbox::new();let mut inner=Fake::new();
        for (i,b) in inner.disk.iter_mut().enumerate(){*b=((i*31+i/997)%251) as u8;}
        let original=inner.disk.clone();let source=if format==0{
            let path=s.0.join("full.img");fs::write(&path,&original).unwrap();archive::import(&s.0,&path,&mut |_|{}).unwrap()["id"].as_str().unwrap().to_string()
        }else{
            let id=archived(&mut inner,&s,"FULL").await;
            if format==3{let path=archive::path(&s.0,&id).unwrap();let a=archive::Archive::open(&path).unwrap();let mut meta=a.meta.clone();meta.format=3;drop(a);archive::tests::encode(&path,&original,&meta);}id
        };
        // Same real source/codec and fake transport delays; only overlap differs.
        // This reference reproduces the old serial transfer/hash loop.
        let a=archive::open(&s.0,&source,&mut |_|{}).unwrap();let mut input=a.reader().unwrap();let mut io=Metered::new(inner);io.delay_ms=10;
        let started=Instant::now();let mut buf=vec![0;CHUNK];let mut offset=0;let mut hash=Sha256::new();
        while offset<original.len(){let n=(original.len()-offset).min(CHUNK);input.read_exact(&mut buf[..n]).unwrap();hash.update(&buf[..n]);assert_eq!(io.write((offset/512) as u32,&buf[..n]).await.unwrap() as usize,n);offset+=n;}
        let serial_write_s=started.elapsed().as_secs_f64();assert_eq!(format!("{:x}",hash.finalize()),hash_bytes(&original));
        let started=Instant::now();let mut hash=Sha256::new();let mut offset=0;
        while offset<original.len(){let n=(original.len()-offset).min(CHUNK);read_exact(&mut io,(offset/512) as u32,&mut buf[..n]).await.unwrap();hash.update(&buf[..n]);offset+=n;}
        let serial_readback_s=started.elapsed().as_secs_f64();assert_eq!(format!("{:x}",hash.finalize()),hash_bytes(&original));drop(input);drop(a);
        let p=plan_restore_direct(&mut io,&s.0,&device(),"restore-archive",&source,&mut |_|{}).await.unwrap();io.inner.disk.fill(0);io.inner.disk[..RESERVE].copy_from_slice(&original[..RESERVE]);let n=original.len();io.inner.disk[n-33*512..].copy_from_slice(&original[n-33*512..]);
        let out=execute(&mut io,&s.0,&device(),p["plan_id"].as_str().unwrap(),true,&mut |_|{}).await.unwrap();assert_eq!(io.inner.disk,original);
        reports.push(json!({"source":match format{0=>"RAW",3=>"XZ",_=>"Zstd"},"bytes":n,"simulated_delay_ms":10,"serial_write_s":serial_write_s,"serial_readback_s":serial_readback_s,"pipeline":out["timings"]}));
    }
    let report=json!({"scope":"PC-only fake USB with 10 ms/block latency; production execute, real files/codecs; not a board throughput claim","results":reports});
    let dir=Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-results");fs::create_dir_all(&dir).unwrap();fs::write(dir.join("pipeline-speed.json"),serde_json::to_vec_pretty(&report).unwrap()).unwrap();println!("{report}");
}
#[tokio::test] async fn direct_restore_raw_without_any_recovery_files(){
    let s=Sandbox::new();let mut io=Fake::new();let original=io.disk.clone();
    let path=s.0.join("android.img");fs::write(&path,&original).unwrap();
    let row=archive::import(&s.0,&path,&mut |_|{}).unwrap();let source=row["id"].as_str().unwrap();
    io.disk.fill(0x17);let mut phases=vec![];
    let p=plan_restore_direct(&mut io,&s.0,&device(),"restore-archive",source,&mut |p|phases.push(p.phase)).await.unwrap();
    assert!(p["backup_path"].is_null());assert_eq!(phases,["preflight-write"]);assert!(io.writes.is_empty());
    assert!(!s.0.join("backup").exists());assert!(!s.0.join("data/backups").exists());
    let pid=p["plan_id"].as_str().unwrap();let out=run(&mut io,&s,pid).await.unwrap();
    assert_eq!(io.disk,original);assert_eq!(out["verified"],true);assert!(out["backup_path"].is_null());
    assert!(!s.0.join("backup").exists());assert!(!s.0.join("data/backups").exists());
    assert!(run(&mut io,&s,pid).await.is_err());
}
#[tokio::test] async fn repeated_restore_renews_consumed_plan_and_reuses_pending_preview(){
    for kind in ["raw","FULL","legacy"]{
        let s=Sandbox::new();let mut io=Fake::healthy();let original=io.disk.clone();
        let (op,source)=if kind=="raw"{
            let path=s.0.join("android.img");fs::write(&path,&original).unwrap();
            ("restore-archive",archive::import(&s.0,&path,&mut |_|{}).unwrap()["id"].as_str().unwrap().to_owned())
        }else if kind=="legacy"{
            let backup=workflow::backup(&mut io,&s.0.join("data/backups"),&device(),&mut |_|{}).await.unwrap();
            ("restore",Path::new(backup["path"].as_str().unwrap()).file_name().unwrap().to_string_lossy().into_owned())
        }else{("restore-archive",archived(&mut io,&s,kind).await)};
        io.disk[64*512]^=1;io.disk[35000*512]^=2;let target=io.disk.clone();
        let folders=["backup","data/backups"];let counts=folders.map(|folder|fs::read_dir(s.0.join(folder)).map(|d|d.count()).unwrap_or(0));
        let dir=s.0.join("data/plans");let mut records:Vec<(PathBuf,Vec<u8>)>=vec![];let mut ids=vec![];
        for attempt in 0..3{
            io.disk.clone_from(&target);let writes=io.writes.len();
            let p=direct(&mut io,&s,op,&source).await.unwrap();let id=p["plan_id"].as_str().unwrap().to_owned();
            assert!(p["backup_path"].is_null());assert!(!ids.contains(&id));
            let pending=dir.join(format!("{id}.json"));let bytes=fs::read(&pending).unwrap();
            assert_eq!(hash_bytes(&bytes),id);let saved:Value=serde_json::from_slice(&bytes).unwrap();
            if attempt==0{assert!(saved.get("attempt").is_none(),"Existing format-1 IDs stay compatible");}
            else{assert_eq!(saved["attempt"],attempt);}
            assert_eq!(direct(&mut io,&s,op,&source).await.unwrap()["plan_id"],id,"Unconsumed preview is reused");
            assert_eq!(io.writes.len(),writes,"Planning never writes eMMC");
            fs::write(&pending,b"changed").unwrap();
            assert_eq!(direct(&mut io,&s,op,&source).await.unwrap_err().code,"PLAN_CHANGED");
            fs::write(&pending,&bytes).unwrap();
            let out=run(&mut io,&s,&id).await.unwrap();assert_eq!(out["verified"],true);assert!(out["backup_path"].is_null());
            let mut expected=target.clone();for r in p["ranges"].as_array().unwrap(){let start=r["lba"].as_u64().unwrap() as usize*512;let end=start+r["bytes"].as_u64().unwrap() as usize;expected[start..end].copy_from_slice(&original[start..end]);}
            assert!(io.disk==expected,"Every selected and unselected byte");
            let used=dir.join(format!("{id}.used.json"));assert_eq!(fs::read(&used).unwrap(),bytes);assert!(!pending.exists());
            let writes=io.writes.len();assert!(run(&mut io,&s,&id).await.is_err());assert_eq!(io.writes.len(),writes,"Consumed plan cannot execute again");
            for path in [used,dir.join(format!("{id}.journal.jsonl"))]{records.push((path.clone(),fs::read(path).unwrap()));}
            for (path,before) in &records{assert_eq!(fs::read(path).unwrap(),*before,"Prior attempts remain untouched");}
            assert_eq!(folders.map(|folder|fs::read_dir(s.0.join(folder)).map(|d|d.count()).unwrap_or(0)),counts,"No new recovery backup");
            ids.push(id);
        }
    }
}
#[tokio::test] async fn repeated_restore_after_failed_write_requires_a_new_confirmed_plan(){
    let s=Sandbox::new();let mut io=Fake::healthy();let source=archived(&mut io,&s,"FULL").await;
    let target=io.disk.clone();let mut prior_ids=vec![];
    for attempt in 0..3{
        let p=direct(&mut io,&s,"restore-archive",&source).await.unwrap();let id=p["plan_id"].as_str().unwrap().to_owned();
        assert!(!prior_ids.contains(&id));let writes=io.writes.len();
        assert_eq!(execute(&mut io,&s.0,&device(),&id,false,&mut |_|{}).await.unwrap_err().code,"CONFIRM_WRITE");
        assert_eq!(io.writes.len(),writes);
        io.fail_at=Some(io.writes.len());assert_eq!(run(&mut io,&s,&id).await.unwrap_err().code,"USB_WRITE");io.fail_at=None;
        assert!(io.disk==target,"Failure before the first successful write keeps the same target fingerprint");
        let dir=s.0.join("data/plans");assert!(dir.join(format!("{id}.used.json")).exists());
        let journal=fs::read(dir.join(format!("{id}.journal.jsonl"))).unwrap();let log=String::from_utf8(journal.clone()).unwrap();
        assert!(log.contains("\"failed\""));assert!(!log.contains("\"complete\""));
        let writes=io.writes.len();assert!(run(&mut io,&s,&id).await.is_err());assert_eq!(io.writes.len(),writes);
        assert_eq!(fs::read(dir.join(format!("{id}.journal.jsonl"))).unwrap(),journal);
        assert_eq!(load_used_attempt(&dir,&id),if attempt==0{None}else{Some(attempt)});
        prior_ids.push(id);
    }
    let p=direct(&mut io,&s,"restore-archive",&source).await.unwrap();let id=p["plan_id"].as_str().unwrap();
    assert!(!prior_ids.iter().any(|old|old==id));assert_eq!(run(&mut io,&s,id).await.unwrap()["verified"],true);
    assert_eq!(fs::read_dir(s.0.join("backup")).unwrap().count(),1);
}
fn load_used_attempt(dir:&Path,id:&str)->Option<u64>{serde_json::from_slice::<Value>(&fs::read(dir.join(format!("{id}.used.json"))).unwrap()).unwrap()["attempt"].as_u64()}
#[tokio::test] async fn repeated_restore_with_existing_wizard_recovery_does_not_duplicate_backups(){
    let s=Sandbox::new();let mut io=Fake::healthy();let original=io.disk.clone();let source=archived(&mut io,&s,"FULL").await;
    io.disk[64*512]^=1;let target=io.disk.clone();let recovery=archived(&mut io,&s,"FULL").await;
    let mut ids=vec![];
    for _ in 0..3{
        io.disk.clone_from(&target);let writes=io.writes.len();
        let p=backed(&mut io,&s,"restore-archive",&source,&recovery).await.unwrap();let id=p["plan_id"].as_str().unwrap().to_owned();
        assert!(!ids.contains(&id));assert!(!p["backup_path"].is_null());
        assert_eq!(backed(&mut io,&s,"restore-archive",&source,&recovery).await.unwrap()["plan_id"],id);
        assert_eq!(io.writes.len(),writes);assert_eq!(fs::read_dir(s.0.join("backup")).unwrap().count(),2);
        assert_eq!(run(&mut io,&s,&id).await.unwrap()["verified"],true);assert_eq!(io.disk,original);
        let writes=io.writes.len();assert!(run(&mut io,&s,&id).await.is_err());assert_eq!(io.writes.len(),writes);ids.push(id);
    }
}
#[tokio::test] async fn repeated_restore_renewed_plan_retains_all_write_guards(){
    for fault in 0..8{
        let s=Sandbox::new();let mut io=Fake::healthy();let source=archived(&mut io,&s,"FULL").await;
        io.disk[64*512]^=1;let target=io.disk.clone();
        let p=direct(&mut io,&s,"restore-archive",&source).await.unwrap();let old=p["plan_id"].as_str().unwrap();
        assert_eq!(run(&mut io,&s,old).await.unwrap()["verified"],true);io.disk.clone_from(&target);
        let p=direct(&mut io,&s,"restore-archive",&source).await.unwrap();let id=p["plan_id"].as_str().unwrap();assert_ne!(old,id);
        let mut selected=device();let writes=io.writes.len();
        match fault{
            0=>{},
            1=>selected.location="PORT2".into(),
            2=>io.disk[64*512]^=1,
            3=>io.disk.resize(io.disk.len()+512,0),
            4=>io.restricted=true,
            5=>{let path=archive::path(&s.0,&source).unwrap();let mut data=fs::read(&path).unwrap();data[8]^=1;fs::write(path,data).unwrap();},
            6=>{let path=s.0.join("data/plans").join(format!("{id}.json"));let mut p:Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();p["attempt"]=json!(9);fs::write(path,serde_json::to_vec(&p).unwrap()).unwrap();},
            _=>io.corrupt_at=Some(35000*512),
        }
        let result=execute(&mut io,&s.0,&selected,id,fault!=0,&mut |_|{}).await;
        assert!(result.is_err(),"fault {fault} must not report success");
        if fault<7{assert_eq!(io.writes.len(),writes,"fault {fault} must be rejected before any write");}
        else{
            assert_eq!(result.unwrap_err().code,"WRITE_VERIFY");
            let log=fs::read_to_string(s.0.join("data/plans").join(format!("{id}.journal.jsonl"))).unwrap();
            assert!(log.contains("\"failed\""));assert!(!log.contains("\"complete\""));
        }
        assert_eq!(fs::read_dir(s.0.join("backup")).unwrap().count(),1);
    }
}
#[test] #[ignore="Requires K11C_TEST_USED_PLAN_DIR; reads historical metadata and writes temporary PC files only"]
fn captured_consumed_restore_plan_can_be_renewed_without_changing_history(){
    let dir=PathBuf::from(std::env::var("K11C_TEST_USED_PLAN_DIR").expect("set historical plan directory"));
    let s=Sandbox::new();let target=s.0.join("data/plans");fs::create_dir_all(&target).unwrap();let mut reports=vec![];
    for entry in fs::read_dir(&dir).unwrap(){
        let path=entry.unwrap().path();let name=path.file_name().unwrap().to_string_lossy();
        let Some(id)=name.strip_suffix(".used.json") else{continue;};
        let before=read_bounded(&path,128*1024).unwrap();let mut p:Plan=serde_json::from_slice(&before).unwrap();
        if p.operation!="restore-archive"{continue;}
        assert_eq!(hash_bytes(&before),id);assert_eq!(serde_json::to_vec(&p).unwrap(),before,"Legacy serialization remains byte-identical");
        fs::write(target.join(name.as_ref()),&before).unwrap();let journal_path=dir.join(format!("{id}.journal.jsonl"));let journal=fs::read(&journal_path).unwrap();
        fs::write(target.join(format!("{id}.journal.jsonl")),&journal).unwrap();
        let renewed=save_plan(&s.0,&mut p).unwrap();assert_ne!(renewed,id);assert_eq!(p.attempt,Some(1));
        assert_eq!(save_plan(&s.0,&mut p).unwrap(),renewed);let loaded=load_plan(&s.0,&renewed).unwrap();assert_eq!(loaded.attempt,Some(1));
        assert_eq!(fs::read(&path).unwrap(),before);assert_eq!(fs::read(&journal_path).unwrap(),journal);
        assert_eq!(fs::read(target.join(name.as_ref())).unwrap(),before);assert_eq!(fs::read(target.join(format!("{id}.journal.jsonl"))).unwrap(),journal);
        reports.push(json!({"old_id":id,"new_id":renewed,"source":p.source,"attempt":p.attempt,"history_unchanged":true,"pending_reused":true}));
    }
    assert!(reports.iter().any(|r|r["source"]=="FULL-261001-173727.k11cbackup"),"The actual failing HAOS restore must be covered");
    let out=Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-results");fs::create_dir_all(&out).unwrap();
    fs::write(out.join("restore-plan-renewal.json"),serde_json::to_vec_pretty(&json!({"passed":true,"physical_usb_writes":0,"results":reports})).unwrap()).unwrap();
}
#[tokio::test] async fn direct_restore_archive_and_legacy_preserve_scope_without_new_backup(){
    for kind in ["FULL","BOOT","HAOS","legacy"]{
        let s=Sandbox::new();let mut io=Fake::healthy();let original=io.disk.clone();
        let (op,source)=if kind=="legacy"{
            let p=workflow::backup(&mut io,&s.0.join("data/backups"),&device(),&mut |_|{}).await.unwrap();
            ("restore",Path::new(p["path"].as_str().unwrap()).file_name().unwrap().to_string_lossy().into_owned())
        }else{("restore-archive",archived(&mut io,&s,kind).await)};
        io.disk[64*512]^=1;io.disk[35000*512]^=2;let before=io.disk.clone();
        let p=direct(&mut io,&s,op,&source).await.unwrap();assert!(p["backup_path"].is_null());
        run(&mut io,&s,p["plan_id"].as_str().unwrap()).await.unwrap();
        let mut expected=before;
        for r in p["ranges"].as_array().unwrap(){let start=r["lba"].as_u64().unwrap() as usize*512;let end=start+r["bytes"].as_u64().unwrap() as usize;expected[start..end].copy_from_slice(&original[start..end]);}
        assert!(io.disk==expected,"{kind}: every selected and unselected byte");
        for folder in ["backup","data/backups"]{let path=s.0.join(folder);if path.exists(){assert_eq!(fs::read_dir(path).unwrap().count(),1);}}
    }
}
#[tokio::test] async fn direct_restore_guards_remain_before_first_write(){
    let s=Sandbox::new();let mut io=Fake::healthy();let source=archived(&mut io,&s,"FULL").await;
    for op in ["install","uboot","gpt-repair"]{assert_eq!(direct(&mut io,&s,op,&source).await.unwrap_err().code,"COMMAND_NOT_ALLOWED");}
    io.disk.resize(io.disk.len()+512,0);assert_eq!(direct(&mut io,&s,"restore-archive",&source).await.unwrap_err().code,"RESTORE_CAPACITY");io.disk.truncate(73728*512);
    let p=direct(&mut io,&s,"restore-archive",&source).await.unwrap();let pid=p["plan_id"].as_str().unwrap();
    assert_eq!(execute(&mut io,&s.0,&device(),pid,false,&mut |_|{}).await.unwrap_err().code,"CONFIRM_WRITE");
    let mut wrong=device();wrong.location="PORT2".into();assert_eq!(execute(&mut io,&s.0,&wrong,pid,true,&mut |_|{}).await.unwrap_err().code,"DEVICE_CHANGED");
    io.disk[64*512]^=1;assert_eq!(run(&mut io,&s,pid).await.unwrap_err().code,"DEVICE_CHANGED");io.disk[64*512]^=1;
    io.restricted=true;assert!(run(&mut io,&s,pid).await.is_err());io.restricted=false;
    let path=archive::path(&s.0,&source).unwrap();let mut bytes=fs::read(&path).unwrap();bytes[8]^=1;fs::write(path,bytes).unwrap();
    assert!(run(&mut io,&s,pid).await.is_err());assert!(io.writes.is_empty());
    assert_eq!(fs::read_dir(s.0.join("backup")).unwrap().count(),1);
}
#[tokio::test] async fn direct_restore_write_failure_never_reports_success(){
    for fault in 0..3{
        let s=Sandbox::new();let mut io=Fake::new();let source=archived(&mut io,&s,"FULL").await;
        let p=direct(&mut io,&s,"restore-archive",&source).await.unwrap();let pid=p["plan_id"].as_str().unwrap();
        io.short=fault==0;
        // Damage only user data, leaving both GPT copies intact: the full
        // readback hash itself must catch this, not the later header checks.
        if fault==1{io.corrupt_at=Some(35000*512);}if fault==2{io.fail_at=Some(1);}
        assert_eq!(run(&mut io,&s,pid).await.unwrap_err().code,["SHORT_WRITE","WRITE_VERIFY","USB_WRITE"][fault]);
        assert!(s.0.join("data/plans").join(format!("{pid}.used.json")).exists());assert!(run(&mut io,&s,pid).await.is_err());
    }
}
#[tokio::test] async fn raw_catalog_persists_without_full_scan_and_rechecks_changed_source(){
    let s=Sandbox::new();let io=Fake::new();let path=s.0.join("android.img");fs::write(&path,&io.disk).unwrap();
    let row=archive::import(&s.0,&path,&mut |_|{}).unwrap();let id=row["id"].as_str().unwrap();
    assert!(!s.0.join("backup").exists());
    // Existing path-only receipts from the previous release remain valid.
    let rows=archive::catalog(&s.0).unwrap();assert_eq!(rows.len(),1);assert_eq!(rows[0]["id"],id);assert_eq!(rows[0]["kind"],"FULL");assert_eq!(rows[0]["verified"],false);assert_eq!(rows[0]["display_name"],"android.img");
    let mut changed=io.disk.clone();changed[35000*512]^=1;fs::write(&path,changed).unwrap();
    assert_eq!(archive::catalog(&s.0).unwrap()[0]["restorable"],true,"Listing is bounded metadata, not a full hash pass");
    assert!(archive::open(&s.0,id,&mut |_|{}).is_err(),"Restore must rehash RAW imports");
    fs::remove_file(&path).unwrap();let missing=archive::catalog(&s.0).unwrap();assert_eq!(missing.len(),1);assert_eq!(missing[0]["restorable"],false);
}
#[tokio::test] async fn archive_full_unknown_os_roundtrip_is_sector_exact(){
    let s=Sandbox::new();let mut io=Fake::new();
    for i in (0..io.disk.len()).step_by(997){io.disk[i]=(i%251) as u8;}
    let original=io.disk.clone();let id=archived(&mut io,&s,"FULL").await;
    assert!(io.writes.is_empty());assert!(s.0.join("backup").join(&id).is_file());
    assert!(!s.0.join("data/backups").exists());
    let count=fs::read_dir(s.0.join("backup")).unwrap().count();
    let selected=archive::import(&s.0,&archive::path(&s.0,&id).unwrap(),&mut |_|{}).unwrap();
    assert_eq!(selected["id"],id);assert_eq!(fs::read_dir(s.0.join("backup")).unwrap().count(),count);
    let a=archive::open(&s.0,&id,&mut |_|{}).unwrap();assert_eq!(a.meta.os,"unknown");drop(a);
    io.disk.fill(0x17);let current=io.disk.clone();
    let p=backed(&mut io,&s,"restore-archive",&id,"").await.unwrap();
    assert!(io.writes.is_empty());assert_eq!(p["erases_user_data"],true);
    let recovery=p["backup_path"].as_str().unwrap();let mut recovery=archive::Archive::open(Path::new(recovery)).unwrap();recovery.verify(&mut |_|{}).unwrap();
    assert_eq!(recovery.meta.ranges[0].sha256,hash_bytes(&current));drop(recovery);
    let pid=p["plan_id"].as_str().unwrap();let result=run(&mut io,&s,pid).await.unwrap();
    assert_eq!(result["verified"],true);assert_eq!(io.disk,original);assert_eq!(result["gpt"]["healthy"],false);
    assert!(run(&mut io,&s,pid).await.is_err());
}
#[tokio::test] async fn archive_partial_restores_preserve_every_unselected_byte(){
    for kind in ["BOOT","HAOS"]{
        let s=Sandbox::new();let mut io=Fake::healthy();let original=io.disk.clone();let id=archived(&mut io,&s,kind).await;
        let a=archive::open(&s.0,&id,&mut |_|{}).unwrap();let ranges=a.meta.ranges.clone();drop(a);
        for r in &ranges{let start=r.lba as usize*512;for b in &mut io.disk[start..start+r.bytes as usize]{*b^=0x22;}}
        // Partial restore requires a recognizable matching current partition map.
        io.disk[..34*512].copy_from_slice(&original[..34*512]);let n=io.disk.len();io.disk[n-33*512..].copy_from_slice(&original[n-33*512..]);
        let before=io.disk.clone();let p=backed(&mut io,&s,"restore-archive",&id,"").await.unwrap();
        assert_eq!(p["erases_user_data"],false);run(&mut io,&s,p["plan_id"].as_str().unwrap()).await.unwrap();
        for i in 0..io.disk.len(){let selected=ranges.iter().any(|r|i>=r.lba as usize*512&&i<(r.lba as u64*512+r.bytes) as usize);assert_eq!(io.disk[i],if selected{original[i]}else{before[i]},"{kind} byte {i}");}
    }
}
#[tokio::test] async fn archive_legacy_xz_restores_full_and_partial(){
    for kind in ["FULL","BOOT","HAOS"]{
        let s=Sandbox::new();let mut io=Fake::healthy();let original=io.disk.clone();
        let new_id=archived(&mut io,&s,kind).await;
        let a=archive::open(&s.0,&new_id,&mut |_|{}).unwrap();assert_eq!(a.meta.format,4);
        let mut meta=a.meta.clone();drop(a);meta.format=3;
        let bytes=meta.ranges.iter().flat_map(|r|original[r.lba as usize*512..r.lba as usize*512+r.bytes as usize].iter().copied()).collect::<Vec<_>>();
        let external=s.0.join("legacy.k11cbackup");archive::tests::encode(&external,&bytes,&meta);
        let imported=archive::import(&s.0,&external,&mut |_|{}).unwrap();let id=imported["id"].as_str().unwrap();
        io.disk[64*512]^=0x12;io.disk[35000*512]^=0x34;let before=io.disk.clone();
        let p=backed(&mut io,&s,"restore-archive",id,"").await.unwrap();
        assert!(io.writes.is_empty());run(&mut io,&s,p["plan_id"].as_str().unwrap()).await.unwrap();
        for r in &meta.ranges{let start=r.lba as usize*512;assert_eq!(&io.disk[start..start+r.bytes as usize],&original[start..start+r.bytes as usize]);}
        for i in [64*512,35000*512]{let selected=meta.ranges.iter().any(|r|i>=r.lba as usize*512&&i<(r.lba as u64*512+r.bytes) as usize);assert_eq!(io.disk[i],if selected{original[i]}else{before[i]});}
    }
}
#[tokio::test] async fn archive_async_read_failure_never_publishes(){
    struct Fault {inner:Fake,reads:usize,short:bool}
    impl UsbIo for Fault{
        async fn identity(&mut self)->Result<Identity>{self.inner.identity().await}
        async fn capability(&mut self)->Result<[u8;8]>{self.inner.capability().await}
        async fn read(&mut self,lba:u32,b:&mut[u8])->Result<u32>{
            self.reads+=1;if self.reads==7{return if self.short{Ok(0)}else{Err(fail("USB_READ","Disconnected after worker jobs queued"))};}self.inner.read(lba,b).await
        }
        async fn upload(&mut self,_:u16,_:&[u8])->Result<()>{panic!("backup must not upload")}
    }
    for short in [false,true]{
        let s=Sandbox::new();let mut io=Fault{inner:Fake::healthy(),reads:0,short};let mut phases=vec![];
        let error=archive::create(&mut io,&s.0,"FULL",&mut |p|phases.push(p.phase)).await.unwrap_err();
        assert_eq!(error.code,if short{"SHORT_READ"}else{"USB_READ"});assert_eq!(io.reads,7);
        assert!(phases.iter().all(|p|p=="read-compress"));assert!(!phases.is_empty());assert!(io.inner.writes.is_empty());
        let files=fs::read_dir(s.0.join("backup")).unwrap().map(|f|f.unwrap().path()).collect::<Vec<_>>();
        assert_eq!(files.len(),1);assert_eq!(files[0].extension().unwrap(),"partial");assert!(archive::catalog(&s.0).unwrap().is_empty());
        assert!(archive::Archive::open(&files[0]).is_err());
    }
}
#[tokio::test] #[ignore="PC-only performance report, not an eMMC throughput benchmark"]
async fn archive_speed_report(){
    use std::io::Write;
    let s=Sandbox::new();let mut io=Fake::new();io.disk.resize(64*1024*1024,0);let mut rng=0x12345678u32;
    // Alternating zero, structured and incompressible 1-MiB regions; no user data.
    for (i,region) in io.disk.chunks_mut(1024*1024).enumerate(){for (j,b) in region.iter_mut().enumerate(){rng^=rng<<13;rng^=rng>>17;rng^=rng<<5;*b=match i%4{0=>0,1=>(j%251) as u8,2=>(rng&15) as u8,_=>rng as u8};}}
    let expected=hash_bytes(&io.disk);let mut reports=vec![];
    for trial in 0..3{
        let started=std::time::Instant::now();let old=s.0.join("old.xz");
        let mut encoder=xz2::write::XzEncoder::new(fs::File::create(&old).unwrap(),1);let mut hash=Sha256::new();
        for b in io.disk.chunks(1024*1024){encoder.write_all(b).unwrap();hash.update(b);}encoder.finish().unwrap();
        let old_s=started.elapsed().as_secs_f64();assert_eq!(format!("{:x}",hash.finalize()),expected);
        let old_bytes=fs::metadata(old).unwrap().len();let mut phases=vec![];
        let new=archive::create(&mut io,&s.0,"FULL",&mut |p|{if phases.last()!=Some(&p.phase){phases.push(p.phase);}}).await.unwrap();
        assert_eq!(phases,["read-compress","verify","verify-backup-file"]);assert!(io.writes.is_empty());
        let a=archive::open(&s.0,new["id"].as_str().unwrap(),&mut |_|{}).unwrap();assert_eq!(a.meta.ranges[0].sha256,expected);drop(a);
        reports.push(json!({"trial":trial+1,"input_mib":64,"old_xz_read_hash_compress_s":old_s,"old_xz_bytes":old_bytes,"new_stored_bytes":new["stored_bytes"],"new_timings":new["timings"]}));
    }
    let report=json!({"scope":"Synthetic PC benchmark; production create with virtual USB, real files, device reread and file verify; not real USB throughput","trials":reports});
    eprintln!("ARCHIVE_SPEED_REPORT={report}");
    let out=Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-results");fs::create_dir_all(&out).unwrap();fs::write(out.join("archive-speed.json"),serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
#[tokio::test] async fn archive_wizard_reuses_full_recovery_and_checks_data_not_only_boot(){
    let s=Sandbox::new();let mut io=Fake::healthy();let image=s.image();let recovery=archived(&mut io,&s,"FULL").await;
    let count=fs::read_dir(s.0.join("backup")).unwrap().count();
    let p=backed(&mut io,&s,"install",&image,&recovery).await.unwrap();assert_eq!(fs::read_dir(s.0.join("backup")).unwrap().count(),count);
    // Closing the preview and opening it again reuses the pending plan and FULL
    // recovery, without a new backup or any write to the device.
    let again=backed(&mut io,&s,"install",&image,&recovery).await.unwrap();assert_eq!(p["plan_id"],again["plan_id"]);
    assert_eq!(fs::read_dir(s.0.join("backup")).unwrap().count(),count);assert!(io.writes.is_empty());
    let pending=s.0.join("data/plans").join(format!("{}.json",p["plan_id"].as_str().unwrap()));
    let original=fs::read(&pending).unwrap();fs::write(&pending,b"changed").unwrap();
    assert_eq!(backed(&mut io,&s,"install",&image,&recovery).await.unwrap_err().code,"PLAN_CHANGED");fs::write(&pending,original).unwrap();
    io.disk[36000*512]^=1;
    assert_eq!(run(&mut io,&s,p["plan_id"].as_str().unwrap()).await.unwrap_err().code,"DEVICE_CHANGED");assert!(io.writes.is_empty());
    assert!(backed(&mut io,&s,"install",&image,&recovery).await.is_err());
}
#[tokio::test] async fn archive_restore_rejects_corruption_capacity_layout_and_unconfirmed_write(){
    let s=Sandbox::new();let mut io=Fake::healthy();let id=archived(&mut io,&s,"BOOT").await;
    io.disk[1024+16]^=1;let entries=io.disk[1024..34*512].to_vec();let n=io.disk.len();io.disk[n-33*512..n-512].copy_from_slice(&entries);let crc=gpt::crc(&entries);gpt::put32(&mut io.disk,512+88,crc);gpt::put32(&mut io.disk,n-512+88,crc);gpt::seal(&mut io.disk[512..1024]);gpt::seal(&mut io.disk[n-512..]);
    assert!(backed(&mut io,&s,"restore-archive",&id,"").await.is_err());assert!(io.writes.is_empty());
    let mut io=Fake::healthy();let id=archived(&mut io,&s,"FULL").await;
    io.disk.resize(io.disk.len()+512,0);
    assert_eq!(backed(&mut io,&s,"restore-archive",&id,"").await.unwrap_err().code,"RESTORE_CAPACITY");assert!(io.writes.is_empty());
    io.disk.truncate(73728*512);
    let p=backed(&mut io,&s,"restore-archive",&id,"").await.unwrap();let pid=p["plan_id"].as_str().unwrap();
    assert_eq!(execute(&mut io,&s.0,&device(),pid,false,&mut |_|{}).await.unwrap_err().code,"CONFIRM_WRITE");assert!(io.writes.is_empty());
    let file=archive::path(&s.0,&id).unwrap();let mut bytes=fs::read(&file).unwrap();bytes[8]^=1;fs::write(&file,&bytes).unwrap();
    assert!(run(&mut io,&s,pid).await.is_err());assert!(io.writes.is_empty());
}
#[tokio::test] async fn archive_backup_nonhaos_full_only_and_full_read_required(){
    let s=Sandbox::new();let mut io=Fake::new();
    for kind in ["BOOT","HAOS","../FULL"]{assert!(archive::create(&mut io,&s.0,kind,&mut |_|{}).await.is_err());}
    io.restricted=true;assert!(archive::create(&mut io,&s.0,"FULL",&mut |_|{}).await.is_err());
    io.restricted=false;io.cc_reads=true;assert!(archive::create(&mut io,&s.0,"FULL",&mut |_|{}).await.is_err());assert!(io.writes.is_empty());
    assert!(!s.0.join("backup").exists());
    io.cc_reads=false;io.cap_remaining=Some(1);
    assert!(archive::create(&mut io,&s.0,"FULL",&mut |_|{}).await.is_err());
    let files=fs::read_dir(s.0.join("backup")).unwrap().map(|e|e.unwrap().file_name().to_string_lossy().into_owned()).collect::<Vec<_>>();
    assert_eq!(files.len(),1);assert!(files[0].ends_with(".partial"));
}
#[tokio::test] async fn archive_raw_import_roundtrip_and_firmware_package_rejection(){
    let s=Sandbox::new();let mut io=Fake::new();let original=io.disk.clone();let path=s.0.join("android-full.img");fs::write(&path,&original).unwrap();
    let imported=archive::import(&s.0,&path,&mut |_|{}).unwrap();let id=imported["id"].as_str().unwrap();assert!(id.starts_with("raw-"));
    io.disk.fill(0x13);let p=backed(&mut io,&s,"restore-archive",id,"").await.unwrap();
    run(&mut io,&s,p["plan_id"].as_str().unwrap()).await.unwrap();assert_eq!(io.disk,original);
    for magic in [b"RKFW".as_slice(),b"RKAF",&[0x3a,0xff,0x26,0xed]]{
        let mut bytes=original.clone();bytes[..4].copy_from_slice(magic);fs::write(&path,&bytes).unwrap();
        assert_eq!(archive::import(&s.0,&path,&mut |_|{}).unwrap_err().code,"BACKUP_FORMAT");
    }
}
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
#[tokio::test] async fn ready_maskrom_descriptor_keeps_backup_and_write_invariants(){
    let s=Sandbox::new();let mut io=Fake::healthy();let original=io.disk.clone();let mut d=device();d.mode="Maskrom".into();
    let p=plan(&mut io,&s.0,&d,"uboot","",&mut |_|{}).await.unwrap();
    assert!(io.writes.is_empty());let id=p["plan_id"].as_str().unwrap();
    assert_eq!(execute(&mut io,&s.0,&d,id,false,&mut |_|{}).await.unwrap_err().code,"CONFIRM_WRITE");assert!(io.writes.is_empty());
    io.sd=true;assert!(execute(&mut io,&s.0,&d,id,true,&mut |_|{}).await.is_err());assert!(io.writes.is_empty());io.sd=false;
    let result=execute(&mut io,&s.0,&d,id,true,&mut |_|{}).await.unwrap();assert_eq!(result["verified"],true);
    assert_eq!(&io.disk[..64*512],&original[..64*512]);assert_eq!(&io.disk[64*512+9687040..],&original[64*512+9687040..]);
}
#[tokio::test] async fn gpt_repair_only_changes_broken_copy(){
    let s=Sandbox::new();let mut io=Fake::healthy();let end=io.disk.len();io.disk[end-512+56]^=1;gpt::seal(&mut io.disk[end-512..]);
    let before=io.disk.clone();let id=make(&mut io,&s,"gpt-repair","").await;run(&mut io,&s,&id).await.unwrap();
    assert_eq!(&io.disk[..end-33*512],&before[..end-33*512]);assert!(io.writes.iter().all(|(l,_)|*l>=73728-33));
    assert_eq!(plan(&mut io,&s.0,&device(),"gpt-repair","",&mut |_|{}).await.unwrap()["no_changes"],true);
    io.disk[512+16]^=1;let id=make(&mut io,&s,"gpt-repair","").await;run(&mut io,&s,&id).await.unwrap();
    assert_eq!(check(&mut io).await.unwrap()["gpt"]["healthy"],true);
}
#[tokio::test] async fn restricted_loader_blocks_inspection_planning_and_execution(){
    let s=Sandbox::new();let image=s.image();let mut io=Fake::healthy();
    let id=make(&mut io,&s,"uboot","").await;
    io.restricted=true;
    assert_eq!(check(&mut io).await.unwrap_err().code,"LOADER_READ_RESTRICTED");
    for (op,source) in [("uboot",""),("gpt-repair",""),("install",image.as_str()),("restore","k11c-old")] {
        assert_eq!(plan(&mut io,&s.0,&device(),op,source,&mut |_|{}).await.unwrap_err().code,"LOADER_READ_RESTRICTED");
    }
    assert_eq!(run(&mut io,&s,&id).await.unwrap_err().code,"LOADER_READ_RESTRICTED");
    assert!(io.writes.is_empty());assert!(s.0.join("data/plans").join(format!("{id}.json")).exists());
}
#[tokio::test] async fn false_success_cc_is_not_a_repairable_gpt_or_write_authorization(){
    let s=Sandbox::new();let image=s.image();let mut io=Fake::healthy();
    let id=make(&mut io,&s,"uboot","").await;
    io.cc_reads=true; // success length and full-read bit, but fake data
    assert_eq!(check(&mut io).await.unwrap_err().code,"USB_READ_UNTRUSTED");
    for (op,source) in [("uboot",""),("gpt-repair",""),("install",image.as_str())] {
        assert_eq!(plan(&mut io,&s.0,&device(),op,source,&mut |_|{}).await.unwrap_err().code,"USB_READ_UNTRUSTED");
    }
    assert_eq!(run(&mut io,&s,&id).await.unwrap_err().code,"USB_READ_UNTRUSTED");assert!(io.writes.is_empty());
}
#[tokio::test] async fn capability_loss_immediately_before_write_leaves_disk_unchanged(){
    let s=Sandbox::new();let mut io=Fake::healthy();let id=make(&mut io,&s,"uboot","").await;
    io.cap_remaining=Some(1);let before=io.disk.clone();
    assert_eq!(run(&mut io,&s,&id).await.unwrap_err().code,"LOADER_READ_RESTRICTED");
    assert!(io.writes.is_empty());assert_eq!(io.disk,before);
}
#[tokio::test] async fn legacy_cc_backup_is_rejected_even_with_matching_hashes_and_success_flags(){
    let s=Sandbox::new();let mut io=Fake::healthy();let id=make(&mut io,&s,"uboot","").await;
    let p=load_plan(&s.0,&id).unwrap();let dir=s.0.join("data/backups").join(&p.backup);let file=dir.join("manifest.json");
    let mut m:Value=serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    m["format"]=1.into();m.as_object_mut().unwrap().remove("read_capability_hex");
    fs::write(&file,serde_json::to_vec(&m).unwrap()).unwrap();
    assert_eq!(catalog(&s.0).unwrap()[0]["restorable"],true,"Healthy old backups remain usable");
    let cc=vec![0xcc;33*512];fs::write(dir.join("gpt-backup.bin"),&cc).unwrap();
    m["files"][3]["sha256"]=hash_bytes(&cc).into();m["gpt"]["healthy"]=true.into();
    fs::write(&file,serde_json::to_vec(&m).unwrap()).unwrap();
    assert!(catalog(&s.0).unwrap().as_array().unwrap().is_empty());
    assert_eq!(snapshot(&s.0,&p.backup,&p.identity,&device()).err().unwrap().code,"USB_READ_UNTRUSTED");
    assert_eq!(run(&mut io,&s,&id).await.unwrap_err().code,"USB_READ_UNTRUSTED");assert!(io.writes.is_empty());
}
#[tokio::test] async fn capability_evidence_is_required_on_new_backups(){
    let s=Sandbox::new();let mut io=Fake::healthy();let id=make(&mut io,&s,"uboot","").await;
    let p=load_plan(&s.0,&id).unwrap();let file=s.0.join("data/backups").join(&p.backup).join("manifest.json");
    let mut m:Value=serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(m["format"],2);assert_eq!(m["read_capability_hex"],"3f07000000000000");
    for caps in ["3707000000000000","","zz07000000000000"] {
        m["read_capability_hex"]=caps.into();fs::write(&file,serde_json::to_vec(&m).unwrap()).unwrap();
        assert_eq!(run(&mut io,&s,&id).await.unwrap_err().code,"BACKUP_VERIFY");assert!(io.writes.is_empty());
    }
}
#[tokio::test] async fn missing_confirmation_changed_device_and_snapshot_block_all_writes(){
    let s=Sandbox::new();let mut io=Fake::healthy();let id=make(&mut io,&s,"uboot","").await;
    assert_eq!(execute(&mut io,&s.0,&device(),&id,false,&mut |_|{}).await.unwrap_err().code,"CONFIRM_WRITE");
    let mut d=device();d.location="PORT2".into();assert_eq!(execute(&mut io,&s.0,&d,&id,true,&mut |_|{}).await.unwrap_err().code,"DEVICE_CHANGED");
    io.sd=true;assert!(run(&mut io,&s,&id).await.is_err());io.sd=false;
    io.disk[0]^=1;assert_eq!(run(&mut io,&s,&id).await.unwrap_err().code,"DEVICE_CHANGED");assert!(io.writes.is_empty());
}
#[tokio::test]async fn fresh_install_requires_connectivity_receipt_and_selected_boot(){
 let s=Sandbox::new();let image=s.image();let mut io=Fake::healthy();
 fs::remove_file(s.0.join("data/components/installations").join(format!("{image}.json"))).unwrap();
 assert!(plan(&mut io,&s.0,&device(),"install",&image,&mut |_|{}).await.is_err());assert!(io.writes.is_empty());
 assert!(!s.0.join("data/backups").exists(),"Reject before even backing up");
}
#[tokio::test]async fn selected_boot_is_frozen_and_range_exact(){
 let s=Sandbox::new();let image=s.image();let mut io=Fake::healthy();let before=io.disk.clone();
 let selection=crate::components::selection(&s.0,&image).unwrap();let hash=selection.boot.asset.sha256;
 let id=make(&mut io,&s,"uboot",&hash).await;
 run(&mut io,&s,&id).await.unwrap();
 assert_eq!(&io.disk[..64*512],&before[..64*512]);assert_eq!(&io.disk[64*512+9687040..],&before[64*512+9687040..]);
}
#[tokio::test] async fn source_plan_backup_and_firmware_tampering_block_all_writes(){
    for kind in 0..4{
        let s=Sandbox::new();let image=s.image();let mut io=Fake::healthy();let id=make(&mut io,&s,"install",&image).await;
        match kind{
            0=>{let p=s.0.join("data/images/prepared").join(format!("{image}.img"));let mut b=fs::read(&p).unwrap();b[100000]^=1;fs::write(p,b).unwrap();},
            1=>{let p=s.0.join("data/plans").join(format!("{id}.json"));let mut b=fs::read(&p).unwrap();b.push(b' ');fs::write(p,b).unwrap();},
            2=>{let p=load_plan(&s.0,&id).unwrap();fs::write(s.0.join("data/backups").join(p.backup).join("gpt-primary.bin"),b"bad").unwrap();},
            _=>{fs::write(s.0.join("data/components/assets").join(FIRMWARE_SHA256),b"bad").unwrap();}
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
    async fn capability(&mut self)->Result<[u8;8]>{Ok([0x3f,7,0,0,0,0,0,0])}
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
    let components=PathBuf::from(std::env::var("K11C_TEST_COMPONENTS").expect("set components resource path"));
    let s=Sandbox::new();
    let resource=s.0.join("resources/components");fs::create_dir_all(resource.join("assets")).unwrap();
    fs::copy(components.join("catalog.json"),resource.join("catalog.json")).unwrap();
    for f in fs::read_dir(components.join("assets")).unwrap(){let f=f.unwrap();fs::copy(f.path(),resource.join("assets").join(f.file_name())).unwrap();}
    struct Offline;impl crate::images::Transport for Offline{fn get(&self,_:&str)->Result<crate::images::Response<'_>>{Err(fail("IMAGE_NETWORK","integration offline bundle"))}}
    let cancel=AtomicBool::new(false);let mut noop=|_|{};let mut job=crate::images::Job::new(&cancel,&mut noop);
    let raw=crate::images::import(&path,&s.0,&mut job).unwrap();
    let prepared=crate::components::prepare(&Offline,&s.0,raw,&mut job).unwrap();
    let id=prepared["sha256"].as_str().unwrap().to_owned();let mut input=locked(&PathBuf::from(prepared["path"].as_str().unwrap())).unwrap();let size=input.metadata().unwrap().len();
    let mut official=locked(&path).unwrap();let mut official_head=vec![0;34*512];official.read_exact(&mut official_head).unwrap();
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
        if i<7{let e=&official_head[1024+i*128..1024+(i+1)*128];assert_eq!(source_hash,hash_range(&mut official,gpt::u64at(e,32)*512,len).unwrap());println!("OFFICIAL_PARTITION_{}_IDENTICAL sha256={source_hash}",i+1);}
        else{assert_eq!(source_hash,crate::components::selection(&s.0,&id).unwrap().seed.data_sha256);println!("CONNECTIVITY_DATA_IDENTICAL sha256={source_hash}");}
    }
    assert_eq!(check(&mut io).await.unwrap()["gpt"]["healthy"],true);
    println!("OFFICIAL_INSTALL_PASS image_sha256={id} firmware={FIRMWARE_SHA256} writes={} target_sectors={sectors}",io.writes.len());
    drop(io);
}
