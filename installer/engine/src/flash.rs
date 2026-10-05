//! Two-phase, bounded storage transactions shared by CLI and native GUI.
use crate::{fail, hash_bytes, Result, gpt, workflow::{self, UsbIo, Identity, Progress, Part, read_exact}, usb::Device};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{fs::{self,File,OpenOptions},io::{Read,Write,Seek,SeekFrom,Cursor},path::{Path,PathBuf},sync::atomic::AtomicBool,time::Instant};
use crate::pipeline::{self,ReadAhead,HashWorker,Block};
use std::os::windows::fs::OpenOptionsExt;

pub const FIRMWARE:&str="u-boot-k11c-dfi-r24.bin";
pub const FIRMWARE_SHA256:&str="8a98fbc73fa93eb106a1202cece0bcb3d05355c4f21cab5c151f5b3d900f6260";
const RESERVE:usize=34816*512;
const CHUNK:usize=pipeline::CHUNK;
const CONFIRM:&str="--confirmed-k11c-write";
pub fn confirmed_flag()->&'static str{CONFIRM}
#[allow(async_fn_in_trait)]
pub trait FlashIo:UsbIo{async fn write(&mut self,lba:u32,bytes:&[u8])->Result<u32>;}

#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
pub struct Range {pub label:String,pub lba:u32,pub bytes:u64,pub sha256:String}
enum Payload {Bytes(Vec<u8>),Image{file:File,offset:u64,bytes:u64},Archive}
struct Segment {range:Range,payload:Payload}
struct Work {segments:Vec<Segment>,primary:Vec<u8>,tail:Vec<u8>,archive:Option<crate::archive::Archive>}
#[derive(Debug,Serialize,Deserialize)]
struct Plan {format:u32,operation:String,source:String,device:Device,identity:Identity,backup:String,ranges:Vec<Range>,#[serde(default)] target_boot_sha256:Option<String>,#[serde(default,skip_serializing_if="Option::is_none")] attempt:Option<u64>}
#[derive(Clone,Copy)]
enum Recovery<'a>{Legacy,Backed(&'a str),DirectRestore}

fn disk_io(e:impl std::fmt::Display)->crate::Failure{fail("TRANSACTION_IO",e)}
fn valid_id(s:&str)->bool{s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit()&&!b.is_ascii_uppercase())}
fn backup_id(s:&str)->bool{s.starts_with("k11c-")&&s.len()<120&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')}
fn locked(path:&Path)->Result<File>{
    // Deny write/delete sharing for the lifetime of a validated source handle.
    let f=OpenOptions::new().read(true).share_mode(1).open(path).map_err(disk_io)?;
    if !f.metadata().map_err(disk_io)?.is_file(){return Err(fail("SOURCE_FILE","Expected a regular file"));}Ok(f)
}
fn read_bounded(path:&Path,max:usize)->Result<Vec<u8>>{let mut f=locked(path)?;if f.metadata().map_err(disk_io)?.len()>max as u64{return Err(fail("SOURCE_FILE","File exceeds expected size"));}let mut b=vec![];f.read_to_end(&mut b).map_err(disk_io)?;Ok(b)}
fn hash_range(f:&mut File,offset:u64,bytes:u64)->Result<String>{
    f.seek(SeekFrom::Start(offset)).map_err(disk_io)?;let mut worker=HashWorker::new(bytes)?;let mut done=0;
    while done<bytes{let mut data=worker.buffer()?;let len=(bytes-done).min(CHUNK as u64) as usize;
        f.read_exact(&mut data[..len]).map_err(disk_io)?;worker.submit(Block{offset:done,len,data})?;done+=len as u64;}
    Ok(worker.finish()?.hash)
}
fn bytes(label:&str,lba:u32,b:Vec<u8>)->Segment{Segment{range:Range{label:label.into(),lba,bytes:b.len() as u64,sha256:hash_bytes(&b)},payload:Payload::Bytes(b)}}
fn firmware(base:&Path)->Result<Vec<u8>>{
    let b=read_bounded(&base.join("resources/firmware").join(FIRMWARE),RESERVE-64*512)?;
    if b.is_empty()||b.len()%512!=0||hash_bytes(&b)!=FIRMWARE_SHA256{return Err(fail("FIRMWARE_HASH","Bundled K11C r24 firmware is missing or changed"));}Ok(b)
}
fn validate_ranges(ranges:&[Range],sectors:u32)->Result<()> {
    let mut sorted=vec![];
    for r in ranges{
        let end=r.lba as u64+r.bytes/512;
        if r.bytes==0||r.bytes%512!=0||end>sectors as u64{return Err(fail("WRITE_RANGE","Write outside selected eMMC or unaligned length"));}
        sorted.push((r.lba as u64,end));
    }
    sorted.sort_unstable();if sorted.windows(2).any(|r|r[0].1>r[1].0){return Err(fail("WRITE_RANGE","Overlapping write ranges"));}Ok(())
}
struct Snapshot {primary:Vec<u8>,reserved:Vec<u8>,tail:Vec<u8>}
impl Snapshot {
    fn head(&self)->Vec<u8>{[self.primary.as_slice(),self.reserved.as_slice()].concat()}
    fn digest(&self)->String{let mut h=Sha256::new();h.update(&self.primary);h.update(&self.reserved);h.update(&self.tail);format!("{:x}",h.finalize())}
}
fn snapshot(base:&Path,id:&str,identity:&Identity,_device:&Device)->Result<Snapshot>{
    if crate::archive::valid_id(id){
        let a=crate::archive::open(base,id,&mut |_|{})?;
        if a.meta.identity.sectors!=identity.sectors{return Err(fail("BACKUP_DEVICE","Recovery capacity differs"));}
        let (head,tail)=a.boot_snapshot()?;
        return Ok(Snapshot{primary:head[..34*512].to_vec(),reserved:head[34*512..].to_vec(),tail});
    }
    if !backup_id(id){return Err(fail("BACKUP_ID","Select a completed backup"));}
    let dir=base.join("data/backups").join(id);
    let m:Value=serde_json::from_slice(&read_bounded(&dir.join("manifest.json"),128*1024)?).map_err(disk_io)?;
    let saved:Identity=serde_json::from_value(m["identity"].clone()).map_err(disk_io)?;
    if ![json!(1),json!(2)].contains(&m["format"])||m["complete"]!=true||m["kind"]!="k11c-boot-and-gpt"||m["usb_reread_verified"]!=true||saved!=*identity
        {
        return Err(fail("BACKUP_DEVICE","Backup does not match the selected USB device and eMMC capacity"));
    }
    if m["format"]==2 {
        let caps=m["read_capability_hex"].as_str().unwrap_or("");
        if caps.len()!=16||!caps.bytes().all(|b|b.is_ascii_hexdigit())||u8::from_str_radix(&caps[..2],16).unwrap_or(0)&8==0 {
            return Err(fail("BACKUP_VERIFY","Backup lacks full-read capability evidence"));
        }
    }
    let parts:Vec<Part>=serde_json::from_value(m["files"].clone()).map_err(disk_io)?;
    let expected=[("gpt-primary.bin",0,34),("reserved-gap.bin",34,30),("uboot-reserved.bin",64,34752),("gpt-backup.bin",identity.sectors-33,33)];
    if parts.len()!=expected.len(){return Err(fail("BACKUP_VERIFY","Incomplete backup file list"));}
    let mut payloads=vec![];
    for (p,(name,lba,sectors)) in parts.iter().zip(expected){
        if p.file!=name||p.lba!=lba||p.sectors!=sectors||p.bytes!=sectors as u64*512{return Err(fail("BACKUP_VERIFY","Backup ranges were changed"));}
        let b=read_bounded(&dir.join(name),sectors as usize*512)?;
        if b.len() as u64!=p.bytes||hash_bytes(&b)!=p.sha256{return Err(fail("BACKUP_VERIFY",name));}payloads.push(b);
    }
    workflow::require_gpt_bytes(&payloads[0][512..])?;workflow::require_gpt_bytes(&payloads[3])?;
    // Legacy manifests recorded only consistency, not read capability. Accept
    // their bytes only with a valid, matching GPT pair; never trust saved flags.
    if m["format"]==1 && gpt::check(&payloads[0],&payloads[3],identity.sectors)["healthy"]!=true {
        return Err(fail("BACKUP_VERIFY","Legacy backup cannot establish valid disk reads; take a new backup"));
    }
    Ok(Snapshot{primary:payloads[0].clone(),reserved:[payloads[1].as_slice(),payloads[2].as_slice()].concat(),tail:payloads[3].clone()})
}
async fn capture(io:&mut impl UsbIo,identity:&Identity)->Result<Snapshot>{
    let mut head=vec![0;RESERVE];let mut tail=vec![0;33*512];
    for (i,b) in head.chunks_mut(CHUNK).enumerate(){read_exact(io,(i*CHUNK/512) as u32,b).await?;}
    read_exact(io,identity.sectors-33,&mut tail).await?;
    workflow::require_gpt_bytes(&head[512..34*512])?;workflow::require_gpt_bytes(&tail)?;
    Ok(Snapshot{primary:head[..34*512].to_vec(),reserved:head[34*512..].to_vec(),tail})
}
fn build(base:&Path,op:&str,source:&str,identity:&Identity,device:&Device,current:&Snapshot)->Result<Work>{
    let mut segments=vec![];
    let mut archive=None;
    let (primary,tail)=match op {
        "factory-raw"=>{
            let (image,mut f)=crate::factory::open(base,source,&mut |_|{})?;
            if image.format!="RAW"{return Err(fail("FACTORY_FORMAT","Expected a full disk image"));}
            let (a,b,size)=crate::factory::raw_layout(&mut f)?;
            if size>identity.sectors{return Err(fail("CAPACITY","Manufacturer image does not fit the eMMC"));}
            if &a[512..520]==b"EFI PART"{
                let (primary,tail)=gpt::resized(&a,&b,size,identity.sectors)?;
                let offset=34*512;let count=(size as u64-67)*512;let hash=hash_range(&mut f,offset,count)?;
                segments.push(Segment{range:Range{label:"Manufacturer system".into(),lba:34,bytes:count,sha256:hash},payload:Payload::Image{file:f,offset,bytes:count}});
                if size<identity.sectors{segments.push(bytes("Old image GPT cleared",size-33,vec![0;(identity.sectors-size).min(33) as usize*512]));}
                segments.push(bytes("Backup GPT",identity.sectors-33,tail.clone()));segments.push(bytes("Primary GPT",0,primary.clone()));(primary,tail)
            }else{
                let count=image.bytes;let hash=hash_range(&mut f,0,count)?;let mut tail=vec![0;33*512];
                let tail_start=identity.sectors-33;
                if size>tail_start{let n=(size-tail_start) as usize*512;f.seek(SeekFrom::Start(tail_start as u64*512)).map_err(disk_io)?;f.read_exact(&mut tail[..n]).map_err(disk_io)?;}
                segments.push(Segment{range:Range{label:"Manufacturer system".into(),lba:0,bytes:count,sha256:hash},payload:Payload::Image{file:f,offset:0,bytes:count}});
                if size<identity.sectors{let start=size.max(tail_start);segments.push(bytes("Old GPT cleared",start,vec![0;(identity.sectors-start) as usize*512]));}(a,tail)
            }
        },
        "install"=>{
            if !valid_id(source){return Err(fail("IMAGE_ID","Select a prepared HAOS image"));}
            let path=base.join("data/images/prepared").join(format!("{source}.img"));let mut f=locked(&path)?;
            let len=f.metadata().map_err(disk_io)?.len();
            if len%512!=0||len<=34849*512||len>32*1024*1024*1024{return Err(fail("IMAGE_SIZE","Invalid prepared image size"));}
            if hash_range(&mut f,0,len)?!=source{return Err(fail("IMAGE_HASH","Prepared image changed"));}
            crate::images::inspect(&path,&AtomicBool::new(false))?;
            let mut a=vec![0;34*512];let mut b=vec![0;33*512];
            f.seek(SeekFrom::Start(0)).map_err(disk_io)?;f.read_exact(&mut a).map_err(disk_io)?;
            f.seek(SeekFrom::Start(len-33*512)).map_err(disk_io)?;f.read_exact(&mut b).map_err(disk_io)?;
            let (primary,tail)=gpt::relocated(&a,&b,(len/512) as u32,identity.sectors)?;
            let selection=crate::components::selection(base,source)?;
            let fw=crate::components::boot_bytes(base,&selection.boot.asset.sha256)?;
            let mut reserved=vec![0;RESERVE-34*512];reserved[30*512..30*512+fw.len()].copy_from_slice(&fw);
            // Copy every byte after the original pre-partition gap, including
            // partition gaps. Old image-end GPT is zeroed, not left as a third copy.
            let offset=2048*512;let count=len-offset-33*512;let hash=hash_range(&mut f,offset,count)?;
            segments.push(Segment{range:Range{label:"HAOS partitions".into(),lba:34816,bytes:count,sha256:hash},payload:Payload::Image{file:f,offset,bytes:count}});
            let old_tail=(len/512) as u32+32768-33;
            if old_tail<identity.sectors-33 {segments.push(bytes("Old image GPT cleared",old_tail,vec![0;((identity.sectors-33-old_tail).min(33)*512) as usize]));}
            segments.push(bytes("K11C boot firmware",34,reserved));
            segments.push(bytes("Backup GPT",identity.sectors-33,tail.clone()));
            segments.push(bytes("Primary GPT",0,primary.clone()));
            (primary,tail)
        },
        "uboot"=>{
            gpt::require_k11c(&current.primary,&current.tail,identity.sectors)?;
            let fw=if source.is_empty(){firmware(base)?}else{crate::components::boot_bytes(base,source)?};
            segments.push(bytes("K11C U-Boot",64,fw));
            (current.primary.clone(),current.tail.clone())
        },
        "restore-archive"=>{
            let a=crate::archive::open(base,source,&mut |_|{})?;
            if a.meta.identity.sectors!=identity.sectors{return Err(fail("RESTORE_CAPACITY","Full and partial backups require the original eMMC user-area capacity"));}
            if a.meta.kind!="FULL"{
                gpt::require_k11c(&current.primary,&current.tail,identity.sectors)?;
                if current.primary[1024..]!=a.meta.primary[1024..]{return Err(fail("RESTORE_LAYOUT","Partial restore requires the same HAOS partition layout; select FULL to replace the OS"));}
            }
            segments.extend(a.meta.ranges.iter().map(|r|Segment{range:r.clone(),payload:Payload::Archive}));
            let pair=if a.meta.kind=="HAOS"{(current.primary.clone(),current.tail.clone())}else{(a.meta.primary.clone(),a.meta.tail.clone())};
            archive=Some(a);pair
        },
        "restore"=>{
            let saved=snapshot(base,source,identity,device)?;
            gpt::require_k11c(&saved.primary,&saved.tail,identity.sectors)?;
            // A boot-only backup cannot put old partition contents back. Refuse
            // layout changes instead of presenting that as an OS/data restore.
            let (active,_)=gpt::repaired(&current.primary,&current.tail,identity.sectors)?;
            if active[1024..]!=saved.primary[1024..]{return Err(fail("RESTORE_LAYOUT","Partition layout changed; boot backup cannot restore the previous OS or user data"));}
            segments.push(bytes("Reserved gap",34,saved.reserved[..30*512].to_vec()));
            segments.push(bytes("U-Boot backup",64,saved.reserved[30*512..].to_vec()));
            segments.push(bytes("Backup GPT",identity.sectors-33,saved.tail.clone()));
            segments.push(bytes("Primary GPT",0,saved.primary.clone()));
            (saved.primary,saved.tail)
        },
        "gpt-repair"=>{
            if !source.is_empty(){return Err(fail("SOURCE_ID","GPT repair has no source file"));}
            let (p,t)=gpt::repaired(&current.primary,&current.tail,identity.sectors)?;
            if t!=current.tail{segments.push(bytes("Backup GPT",identity.sectors-33,t.clone()));}
            if p!=current.primary{segments.push(bytes("Primary GPT",0,p.clone()));}
            (p,t)
        },
        _=>return Err(fail("COMMAND_NOT_ALLOWED","Unknown storage operation")),
    };
    validate_ranges(&segments.iter().map(|s|s.range.clone()).collect::<Vec<_>>(),identity.sectors)?;
    Ok(Work{segments,primary,tail,archive})
}
fn save_new(path:&Path,bytes:&[u8])->Result<()>{let mut f=OpenOptions::new().write(true).create_new(true).open(path).map_err(disk_io)?;f.write_all(bytes).and_then(|_|f.sync_all()).map_err(disk_io)}
fn save_plan(base:&Path,p:&mut Plan)->Result<String>{
    let dir=base.join("data/plans");fs::create_dir_all(&dir).map_err(disk_io)?;
    loop{
        let bytes=serde_json::to_vec(p).map_err(disk_io)?;let id=hash_bytes(&bytes);
        // A plan is one-use, but its backup/source is reusable. Preserve old
        // attempts and advance only past consumed IDs. No salt on attempt zero
        // keeps old format-1 plans compatible; an unused preview stays stable.
        if dir.join(format!("{id}.used.json")).exists(){
            p.attempt=Some(p.attempt.unwrap_or(0).checked_add(1).ok_or_else(||fail("PLAN_CHANGED","Transaction attempt counter exhausted"))?);
            continue;
        }
        let pending=dir.join(format!("{id}.json"));
        if pending.exists(){
            if read_bounded(&pending,128*1024)?!=bytes{return Err(fail("PLAN_CHANGED","Pending plan changed"));}
        }else{save_new(&pending,&bytes)?;}
        return Ok(id);
    }
}
pub async fn plan(io:&mut impl UsbIo,base:&Path,device:&Device,operation:&str,source:&str,progress:&mut impl FnMut(Progress))->Result<Value>{
    plan_inner(io,base,device,operation,source,Recovery::Legacy,progress).await
}
pub async fn plan_backed(io:&mut impl UsbIo,base:&Path,device:&Device,operation:&str,source:&str,recovery:&str,progress:&mut impl FnMut(Progress))->Result<Value>{
    plan_inner(io,base,device,operation,source,Recovery::Backed(recovery),progress).await
}
pub async fn plan_restore_direct(io:&mut impl UsbIo,base:&Path,device:&Device,operation:&str,source:&str,progress:&mut impl FnMut(Progress))->Result<Value>{
    plan_inner(io,base,device,operation,source,Recovery::DirectRestore,progress).await
}
async fn plan_inner(io:&mut impl UsbIo,base:&Path,device:&Device,operation:&str,source:&str,recovery:Recovery<'_>,progress:&mut impl FnMut(Progress))->Result<Value>{
    if matches!(recovery,Recovery::DirectRestore)&&!["restore","restore-archive","factory-raw"].contains(&operation){return Err(fail("COMMAND_NOT_ALLOWED","Direct restore is only available for restoration"));}
    crate::usb::guard(device)?;let identity=io.identity().await?;crate::usb::require_emmc(&identity.storage)?;
    workflow::require_full_read(io).await?;
    if identity.sectors<=34849{return Err(fail("INVALID_CAPACITY",identity.sectors));}
    progress(Progress::new("preflight-write",0,0));
    let current=capture(io,&identity).await?;
    // All image/layout checks happen before creating the automatic recovery copy.
    let work=build(base,operation,source,&identity,device,&current)?;
    if work.segments.is_empty(){return Ok(json!({"no_changes":true,"operation":operation,"gpt":gpt::check(&work.primary,&work.tail,identity.sectors)}));}
    let backup=match recovery{
        Recovery::DirectRestore=>Value::Null,
        Recovery::Legacy=>workflow::backup(io,&base.join("data/backups"),device,progress).await?,
        Recovery::Backed("")=>{
            let kind=if ["uboot","gpt-repair","restore"].contains(&operation)&&gpt::require_k11c(&current.primary,&current.tail,identity.sectors).is_ok(){"BOOT"}else{"FULL"};
            crate::archive::create(io,base,kind,progress).await?
        },
        Recovery::Backed(id)=>{
            if id.starts_with("raw-"){return Err(fail("BACKUP_VERIFY","A raw import is not a current-device recovery backup"));}
            let a=crate::archive::open(base,id,progress)?;
            if a.meta.kind!="FULL"||a.meta.identity!=identity||!a.meta.usb_reread_verified{return Err(fail("BACKUP_DEVICE","Select the verified FULL backup of the current device"));}
            crate::archive::compare_device(io,&a.meta,progress).await?;
            json!({"path":crate::archive::path(base,id)?})
        }
    };
    let (backup_path,backup_name,target_boot_sha256)=if matches!(recovery,Recovery::DirectRestore){
        (None,String::new(),Some(current.digest()))
    }else{
        let path=PathBuf::from(backup["path"].as_str().ok_or_else(||fail("BACKUP_VERIFY","Missing backup path"))?);
        let name=path.file_name().unwrap().to_string_lossy().into_owned();
        let checked=snapshot(base,&name,&identity,device)?;
        if checked.head()!=current.head()||checked.tail!=current.tail{return Err(fail("DEVICE_CHANGED","Boot area changed during planning"));}
        (Some(path),name,None)
    };
    let mut p=Plan{format:1,operation:operation.into(),source:source.into(),device:device.clone(),identity:identity.clone(),backup:backup_name,ranges:work.segments.into_iter().map(|s|s.range).collect(),target_boot_sha256,attempt:None};
    let id=save_plan(base,&mut p)?;
    let firmware=if operation=="install"{json!(crate::components::selection(base,source)?.boot.revision)}else if operation=="uboot"{json!(if source.is_empty(){FIRMWARE_SHA256}else{source})}else{Value::Null};
    Ok(json!({"plan_id":id,"operation":operation,"device":device,"identity":identity,"backup_path":backup_path,"ranges":p.ranges,"write_bytes":p.ranges.iter().map(|r|r.bytes).sum::<u64>(),"erases_user_data":(["install","factory-raw"].contains(&operation)||work.archive.as_ref().is_some_and(|a|a.meta.kind=="FULL")),"firmware":firmware,"no_changes":false}))
}
fn load_plan(base:&Path,id:&str)->Result<Plan>{
    if !valid_id(id){return Err(fail("PLAN_ID","Invalid plan ID"));}
    let b=read_bounded(&base.join("data/plans").join(format!("{id}.json")),128*1024)?;
    if hash_bytes(&b)!=id{return Err(fail("PLAN_CHANGED","Storage plan changed"));}
    let p:Plan=serde_json::from_slice(&b).map_err(disk_io)?;
    if p.format!=1{return Err(fail("PLAN_ID","Unsupported plan format"));}
    if let Some(hash)=&p.target_boot_sha256{
        if !p.backup.is_empty()||!["restore","restore-archive","factory-raw"].contains(&p.operation.as_str())||!valid_id(hash){return Err(fail("PLAN_ID","Invalid direct restore plan"));}
    }else if p.backup.is_empty(){return Err(fail("PLAN_ID","Missing recovery backup"));}
    Ok(p)
}
pub fn target(base:&Path,id:&str)->Result<Device>{Ok(load_plan(base,id)?.device)}
pub fn require_factory_plan(base:&Path,id:&str)->Result<()>{if load_plan(base,id)?.operation!="factory-raw"{return Err(fail("COMMAND_NOT_ALLOWED","Expected a manufacturer RAW plan"));}Ok(())}
fn journal(f:&mut File,v:Value)->Result<()>{writeln!(f,"{v}").and_then(|_|f.sync_all()).map_err(disk_io)}
pub async fn execute(io:&mut impl FlashIo,base:&Path,device:&Device,id:&str,confirmed:bool,progress:&mut impl FnMut(Progress))->Result<Value>{
    let started=Instant::now();
    if !confirmed{return Err(fail("CONFIRM_WRITE","Confirm the selected K11C storage operation"));}
    crate::usb::guard(device)?;let p=load_plan(base,id)?;
    if p.device.instance_id!=device.instance_id||p.device.location!=device.location{return Err(fail("DEVICE_CHANGED","Selected USB instance or physical port changed"));}
    let identity=io.identity().await?;crate::usb::require_emmc(&identity.storage)?;
    workflow::require_full_read(io).await?;
    if identity.sectors<=34849{return Err(fail("INVALID_CAPACITY",identity.sectors));}
    if identity!=p.identity{return Err(fail("DEVICE_CHANGED","eMMC identity or capacity changed"));}
    let saved=if let Some(expected)=&p.target_boot_sha256{
        let observed=capture(io,&identity).await?;
        if observed.digest()!=*expected{return Err(fail("DEVICE_CHANGED","Boot/GPT data changed after the preview; prepare a new plan"));}
        observed
    }else{snapshot(base,&p.backup,&identity,device)?};
    if crate::archive::valid_id(&p.backup){
        let recovery=crate::archive::open(base,&p.backup,progress)?;
        crate::archive::compare_device(io,&recovery.meta,progress).await?;
    }
    progress(Progress::new("preflight-write",0,0));
    let work=build(base,&p.operation,&p.source,&identity,device,&saved)?;
    let ranges:Vec<_>=work.segments.iter().map(|s|s.range.clone()).collect();
    if ranges!=p.ranges{return Err(fail("PLAN_CHANGED","Inputs no longer match the confirmed write plan"));}
    let current=capture(io,&identity).await?;
    if current.head()!=saved.head()||current.tail!=saved.tail{return Err(fail("DEVICE_CHANGED","Boot/GPT data changed after the preview; prepare a new plan"));}
    if io.identity().await?!=identity{return Err(fail("DEVICE_CHANGED","Device changed before write"));}
    workflow::require_full_read(io).await?;
    // Claim before the first write. A failed transaction cannot be replayed.
    let dir=base.join("data/plans");fs::rename(dir.join(format!("{id}.json")),dir.join(format!("{id}.used.json"))).map_err(disk_io)?;
    let journal_path=dir.join(format!("{id}.journal.jsonl"));let mut log=OpenOptions::new().write(true).create_new(true).open(&journal_path).map_err(disk_io)?;
    let backup_path=if p.backup.is_empty(){None}else if crate::archive::valid_id(&p.backup){Some(crate::archive::path(base,&p.backup)?)}else{Some(base.join("data/backups").join(&p.backup))};
    journal(&mut log,json!({"state":"started","plan":p,"backup_path":backup_path}))?;
    let total=ranges.iter().map(|r|r.bytes).sum::<u64>();let mut done=0u64;
    let preflight_s=started.elapsed().as_secs_f64();
    let result:Result<Value>=async{
        let mut archive_reader:Option<Box<dyn Read+Send>>=work.archive.as_ref().map(|a|a.reader().map(|r|Box::new(r) as Box<dyn Read+Send>)).transpose()?;
        let mut write_s=0.0;let mut readback_s=0.0;let mut source_work_s=0.0;let mut source_wait_s=0.0;let mut usb_write_s=0.0;let mut readback_hash_s=0.0;
        for segment in work.segments {
            let r=&segment.range;let mut offset=0u64;
            let from_archive=matches!(&segment.payload,Payload::Archive);
            let source:Box<dyn Read+Send>=match segment.payload{
                Payload::Bytes(b)=>Box::new(Cursor::new(b)),
                Payload::Image{mut file,offset:start,bytes:count}=>{if count!=r.bytes{return Err(fail("WRITE_RANGE","Image segment size changed"));}file.seek(SeekFrom::Start(start)).map_err(disk_io)?;Box::new(file.take(count))},
                Payload::Archive=>archive_reader.take().ok_or_else(||fail("SOURCE_FILE","Missing archive stream"))?,
            };
            journal(&mut log,json!({"state":"writing","range":r}))?;
            let writing=Instant::now();let mut pipeline=ReadAhead::new(source,r.bytes)?;
            while offset<r.bytes{
                let block=pipeline.next()?;let n=block.len;
                let lba=r.lba+(offset/512) as u32;
                let transferring=Instant::now();let transferred=io.write(lba,&block.data[..n]).await?;
                usb_write_s+=transferring.elapsed().as_secs_f64();
                if transferred as usize!=n{return Err(fail("SHORT_WRITE",format!("LBA {lba}: {transferred}/{n}")));}
                pipeline.recycle(block);offset+=n as u64;done+=n as u64;
                progress(Progress::new("write",done,total));
            }
            source_wait_s+=pipeline.wait_s;
            let (reader,written)=pipeline.finish()?;source_work_s+=written.work_s;
            if written.hash!=r.sha256{return Err(fail("SOURCE_CHANGED","Source differs from validated write plan"));}
            if from_archive{archive_reader=Some(reader);}
            write_s+=writing.elapsed().as_secs_f64();
            // Full range reread after all its writes, rather than only checking
            // the same chunk while it might still live in a transport buffer.
            let reading=Instant::now();
            let actual=pipeline::read_digest(io,r.lba,r.bytes,&mut |offset|progress(Progress::new("verify-write",offset,r.bytes))).await?;
            readback_s+=reading.elapsed().as_secs_f64();readback_hash_s+=actual.work_s;
            if actual.hash!=r.sha256{return Err(fail("WRITE_VERIFY",format!("Readback mismatch: {}",r.label)));}
            journal(&mut log,json!({"state":"verified","range":r}))?;
        }
        let after=capture(io,&identity).await?;
        if after.primary!=work.primary||after.tail!=work.tail{return Err(fail("WRITE_VERIFY","GPT differs from intended pair"));}
        if !["restore-archive","factory-raw"].contains(&p.operation.as_str()){gpt::require_k11c(&after.primary,&after.tail,identity.sectors)?;}
        if p.operation=="uboot"{
            let start=30*512;let length=p.ranges.iter().find(|r|r.lba==64).ok_or_else(||fail("PLAN_CHANGED","U-Boot range missing"))?.bytes as usize;
            if after.reserved[..start]!=saved.reserved[..start]||after.reserved[start+length..]!=saved.reserved[start+length..]{return Err(fail("WRITE_VERIFY","U-Boot update modified bytes outside its range"));}
        }
        if p.operation=="gpt-repair"&&after.reserved!=saved.reserved{return Err(fail("WRITE_VERIFY","GPT repair modified reserved firmware"));}
        if io.identity().await?!=identity{return Err(fail("DEVICE_CHANGED","Identity changed during storage transaction"));}
        let timings=json!({"preflight_s":preflight_s,"write_s":write_s,"readback_s":readback_s,"total_s":started.elapsed().as_secs_f64(),"source_work_s":source_work_s,"source_wait_s":source_wait_s,"usb_write_s":usb_write_s,"readback_hash_s":readback_hash_s,"write_mib_s":total as f64/1048576.0/write_s.max(0.000001),"readback_mib_s":total as f64/1048576.0/readback_s.max(0.000001),"chunk_bytes":CHUNK,"buffers":pipeline::BUFFERS});
        journal(&mut log,json!({"state":"timings","timings":timings}))?;
        journal(&mut log,json!({"state":"complete","verified":true}))?;
        progress(Progress::new("write-complete",total,total));
        Ok(json!({"operation":p.operation,"verified":true,"backup_path":backup_path,"journal":journal_path,"written_bytes":total,"gpt":gpt::check(&after.primary,&after.tail,identity.sectors),"timings":timings}))
    }.await;
    match result {Ok(v)=>Ok(v),Err(e)=>{let _=journal(&mut log,json!({"state":"failed","error":e}));Err(fail(&e.code,format!("{}; backup={}; journal={}; stop and inspect before retrying",e.detail,backup_path.as_ref().map(|p|p.display().to_string()).unwrap_or_else(||"none (direct restore)".into()),journal_path.display())))}}
}
pub async fn check(io:&mut impl UsbIo)->Result<Value>{
    let identity=io.identity().await?;crate::usb::require_emmc(&identity.storage)?;
    workflow::require_full_read(io).await?;
    if identity.sectors<=34849{return Err(fail("INVALID_CAPACITY",identity.sectors));}
    let c=capture(io,&identity).await?;let g=gpt::check(&c.primary,&c.tail,identity.sectors);
    let repair=gpt::repaired(&c.primary,&c.tail,identity.sectors);
    Ok(json!({"identity":identity,"gpt":g,"repairable":g["healthy"]!=true&&repair.is_ok(),"repair_error":repair.err()}))
}
pub fn catalog(base:&Path)->Result<Value>{
    let mut rows=crate::archive::catalog(base)?;
    let dir=base.join("data/backups");if !dir.exists(){return Ok(json!(rows));}
    for entry in fs::read_dir(&dir).map_err(disk_io)?{
        let entry=entry.map_err(disk_io)?;let name=entry.file_name().to_string_lossy().into_owned();if !backup_id(&name){continue;}
        let result=(||->Result<Value>{
            let m:Value=serde_json::from_slice(&read_bounded(&entry.path().join("manifest.json"),128*1024)?).map_err(disk_io)?;
            let identity:Identity=serde_json::from_value(m["identity"].clone()).map_err(disk_io)?;let device:Device=serde_json::from_value(m["device"].clone()).map_err(disk_io)?;
            if identity.sectors<=34849{return Err(fail("BACKUP_VERIFY","Invalid capacity"));}
            let s=snapshot(base,&name,&identity,&device)?;let healthy=gpt::require_k11c(&s.primary,&s.tail,identity.sectors).is_ok();
            Ok(json!({"id":name,"path":entry.path(),"identity":identity,"device":device,"kind":"BOOT","legacy":true,"restorable":healthy}))
        })();
        if let Ok(r)=result{rows.push(r);}
    }
    rows.sort_by(|a,b|b["id"].as_str().cmp(&a["id"].as_str()));Ok(json!(rows))
}

#[cfg(test)] mod tests;
