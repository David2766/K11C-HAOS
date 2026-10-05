//! Manufacturer firmware: the pinned Rockchip CLI owns RKFW installation.
//! Only read-only SFI is exposed during selection; UF is behind a one-use plan.
use crate::{fail, hash_bytes, Result, usb::Device, workflow::{Identity, Progress, UsbIo}};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::{self, File, OpenOptions}, io::{Read, Seek, SeekFrom, Write}, path::{Path, PathBuf}, process::{Command, Stdio}, time::{Duration, Instant}};
use std::os::windows::{fs::OpenOptionsExt, process::CommandExt};

pub const TOOL_SHA256:&str="789c509dde39206d27b2f8915a5a169ff5eafb5b21fdb2108d0709bdda9ebde0";
pub const CONFIG_SHA256:&str="f08ed32575209de48aca01cf3e88337bc72dd8ed2191921f80c62390400c4c08";
const LIMIT:u64=128*1024*1024*1024;
fn error(e:impl std::fmt::Display)->crate::Failure{fail("FACTORY_IMAGE",e)}
pub fn valid_id(id:&str)->bool{id.len()==64&&id.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))}
pub fn locked(path:&Path)->Result<File>{let f=OpenOptions::new().read(true).share_mode(1).open(path).map_err(error)?;if !f.metadata().map_err(error)?.is_file(){return Err(error("Expected a regular image file"));}Ok(f)}
pub fn digest(file:&mut File,progress:&mut impl FnMut(Progress))->Result<String>{
    file.seek(SeekFrom::Start(0)).map_err(error)?;let size=file.metadata().map_err(error)?.len();
    let mut hash=Sha256::new();let mut buf=vec![0;1024*1024];let mut done=0;
    while done<size{let n=file.read(&mut buf).map_err(error)?;if n==0{return Err(error("Truncated image"));}hash.update(&buf[..n]);done+=n as u64;progress(Progress::new("factory-check",done,size));}
    Ok(format!("{:x}",hash.finalize()))
}
fn bounded(file:&mut File,offset:u64,len:u64)->Result<Vec<u8>>{
    if len>64*1024||offset.checked_add(len).is_none_or(|end|end>file.metadata().map(|m|m.len()).unwrap_or(0)){return Err(error("Invalid firmware metadata bounds"));}
    let mut bytes=vec![0;len as usize];file.seek(SeekFrom::Start(offset)).map_err(error)?;file.read_exact(&mut bytes).map_err(error)?;Ok(bytes)
}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
pub struct Entry {pub name:String,pub offset:u64,pub bytes:u64,pub sparse:bool}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
pub struct Partition {pub name:String,pub lba:u32,pub sectors:Option<u32>}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
pub struct Image {pub sha256:String,pub path:PathBuf,pub bytes:u64,pub format:String,pub os:String,pub entries:Vec<Entry>,pub partitions:Vec<Partition>}
impl Image {pub fn report(&self)->Value{json!({"id":self.sha256,"sha256":self.sha256,"path":self.path,"filename":self.path.file_name().map(|n|n.to_string_lossy()),"bytes":self.bytes,"format":self.format,"os":self.os,"partitions":self.partitions,"tool":if self.format=="RKFW"{"Rockchip upgrade_tool"}else{"RockUSB"}})}}
pub fn tool_bundle(base:&Path)->Result<PathBuf>{
    let dir=base.join("resources/rockchip");
    crate::verify_file(&dir.join("upgrade_tool.exe"),TOOL_SHA256)?;
    crate::verify_file(&dir.join("config.ini"),CONFIG_SHA256)?;
    Ok(dir.join("upgrade_tool.exe"))
}
#[derive(Clone,Copy,Debug,PartialEq)]
pub enum ToolAction{Info,Install}
fn tool_args(action:ToolAction,path:&Path)->Vec<std::ffi::OsString>{
    let mut args=vec![if action==ToolAction::Info{"SFI"}else{"UF"}.into(),path.as_os_str().to_owned()];
    if action==ToolAction::Install{args.push("-noreset".into());}args
}
fn tool_success(action:ToolAction,exit:bool,text:&str)->bool{
    let text=text.to_ascii_lowercase();
    exit&&!text.contains("fail")&&!text.contains("error")&&match action{ToolAction::Info=>text.contains("type:update firmware")&&text.contains("entry count:"),ToolAction::Install=>text.contains("upgrade firmware ok.")}
}
pub fn run_tool(base:&Path,action:ToolAction,path:&Path,progress:&mut impl FnMut(Progress))->Result<String>{
    let exe=tool_bundle(base)?;let mut child=Command::new(&exe).args(tool_args(action,path)).current_dir(exe.parent().unwrap())
        .creation_flags(0x08000000).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e|fail("FACTORY_TOOL",e))?;
    let _job=match crate::win::contain_child(child.id()){Ok(job)=>job,Err(e)=>{let _=child.kill();let _=child.wait();return Err(e);}};
    let (tx,rx)=std::sync::mpsc::sync_channel::<Vec<u8>>(64);
    let readers=[Box::new(child.stdout.take().unwrap()) as Box<dyn Read+Send>,Box::new(child.stderr.take().unwrap()) as Box<dyn Read+Send>].into_iter().map(|mut pipe|{
        let tx=tx.clone();std::thread::spawn(move||{let mut buf=[0;4096];loop{let n=pipe.read(&mut buf)?;if n==0{break;}if tx.send(buf[..n].to_vec()).is_err(){break;}}Ok::<_,std::io::Error>(())})
    }).collect::<Vec<_>>();drop(tx);
    let start=Instant::now();let bound=Duration::from_secs(if action==ToolAction::Info{120}else{86400});let mut log=Vec::new();let mut last=0;
    let result=(||->Result<bool>{loop{
        for chunk in rx.try_iter(){if log.len()+chunk.len()>1024*1024{return Err(fail("FACTORY_TOOL","Tool output exceeds bound"));}log.extend_from_slice(&chunk);}
        if start.elapsed().as_secs()!=last{last=start.elapsed().as_secs();progress(Progress::new(if action==ToolAction::Info{"factory-check"}else{"factory-write"},0,0));}
        if let Some(status)=child.try_wait().map_err(error)?{return Ok(status.success());}
        if start.elapsed()>bound{return Err(fail("FACTORY_TOOL","Tool timeout; installation may be incomplete"));}
        std::thread::sleep(Duration::from_millis(50));
    }})();
    if result.is_err(){let _=child.kill();let _=child.wait();}
    // Drain before joining: a chatty tool must not deadlock its bounded pipe readers.
    let mut overflow=false;
    for chunk in rx {if log.len()+chunk.len()<=1024*1024{log.extend_from_slice(&chunk);}else{overflow=true;}}
    for reader in readers{reader.join().map_err(|_|error("Tool reader stopped"))?.map_err(error)?;}
    if overflow{return Err(fail("FACTORY_TOOL","Tool output exceeds bound"));}
    let text=String::from_utf8_lossy(&log).replace('\r',"");
    if !tool_success(action,result?,&text){return Err(fail("FACTORY_TOOL",text));}Ok(text)
}
fn hex(s:&str)->Result<u64>{u64::from_str_radix(s.trim().trim_start_matches("0x"),16).map_err(error)}
pub fn entries(text:&str,file_bytes:u64)->Result<Vec<Entry>>{
    if !text.contains("Chip Tag:3566")&&!text.contains("Chip Tag:3568"){return Err(error("Not RK3566/RK3568 firmware"));}
    let mut rows=Vec::new();let mut count=None;
    for line in text.lines(){
        if let Some(n)=line.trim().strip_prefix("Entry Count:"){count=Some(n.trim().parse::<usize>().map_err(error)?);}
        if !line.trim().starts_with("EntryNo="){continue;}
        let mut file=None;let mut name=None;let mut offset=None;let mut size=None;let mut sparse=false;
        for field in line.split(';').map(str::trim){if let Some((key,value))=field.split_once('='){match key.trim(){"file"=>file=Some(value.trim()),"partition"=>name=Some(value.trim()),"offset"=>offset=Some(hex(value)?),"size"=>size=Some(hex(value)?),"type"=>sparse=value.trim()=="sparse image",_=>{}}}}
        let raw_name=name.or(file).ok_or_else(||error("Missing firmware entry name"))?;
        let name=if ["bootloader","loader"].contains(&raw_name){"MiniLoaderAll.bin"}else{raw_name.rsplit(['/', '\\']).next().unwrap()}.to_owned();
        let (offset,bytes)=(offset.ok_or_else(||error("Missing entry offset"))?,size.ok_or_else(||error("Missing entry size"))?);
        if bytes==0||offset.checked_add(bytes).is_none_or(|end|end>file_bytes)||rows.len()>=128||rows.iter().any(|e:&Entry|e.name==name){return Err(error("Invalid firmware entries"));}
        rows.push(Entry{name,offset,bytes,sparse});
    }
    if count!=Some(rows.len())||!rows.iter().any(|e|e.name=="parameter")||!rows.iter().any(|e|e.name=="MiniLoaderAll.bin")||!["uboot","boot"].iter().all(|name|rows.iter().any(|e|e.name==*name))||!rows.iter().any(|e|["rootfs","super","system"].contains(&e.name.as_str())){return Err(fail("FACTORY_FORMAT","Select a complete manufacturer system image"));}
    let mut spans=rows.iter().map(|e|(e.offset,e.offset+e.bytes)).collect::<Vec<_>>();spans.sort_unstable();
    if spans.windows(2).any(|p|p[0].1>p[1].0){return Err(error("Overlapping firmware entries"));}Ok(rows)
}
pub fn parameter(text:&str)->Result<Vec<Partition>>{
    if !text.lines().any(|l|l.trim()=="TYPE: GPT"){return Err(error("Unsupported firmware partition table"));}
    let list=text.lines().find_map(|l|l.split_once("mtdparts=").map(|(_,v)|v)).and_then(|v|v.split_once(':').map(|(_,v)|v)).ok_or_else(||error("Missing firmware partition table"))?;
    let mut parts=vec![];let mut end=34u64;
    for item in list.split(','){
        let (range,label)=item.trim().split_once('(').ok_or_else(||error("Invalid partition"))?;
        let (size,start)=range.split_once('@').ok_or_else(||error("Invalid partition range"))?;
        let name=label.strip_suffix(')').ok_or_else(||error("Invalid partition label"))?.split(':').next().unwrap().to_owned();
        let lba=u32::try_from(hex(start)?).map_err(error)?;let sectors=if size=="-"{None}else{Some(u32::try_from(hex(size)?).map_err(error)?)};
        if (lba as u64)<end||sectors==Some(0)||parts.len()>=128||parts.iter().any(|p:&Partition|p.name==name)||name.is_empty()||!name.bytes().all(|b|b.is_ascii_alphanumeric()||b"_-".contains(&b)){return Err(error("Invalid partition bounds"));}
        end=sectors.map(|n|lba as u64+n as u64).unwrap_or(u64::MAX);parts.push(Partition{name,lba,sectors});
    }
    if parts.is_empty(){return Err(error("Empty partition table"));}Ok(parts)
}
fn parameter_payload(bytes:&[u8])->Result<&str>{
    // Rockchip parameter files may be PARM + LE size + text + vendor CRC.
    // Layout/polynomial match rkdeveloptool MakeParamBuffer / CRC_32.
    let text=if bytes.starts_with(b"PARM"){
        if bytes.len()<12{return Err(error("Truncated parameter wrapper"));}
        let len=crate::gpt::u32at(bytes,4) as usize;
        if len.checked_add(12)!=Some(bytes.len()){return Err(error("Invalid parameter wrapper length"));}
        let data=&bytes[8..8+len];
        let mut crc=0u32;for b in data{crc^=(*b as u32)<<24;for _ in 0..8{crc=(crc<<1)^if crc&0x80000000!=0{0x04c10db7}else{0};}}
        if crc!=crate::gpt::u32at(bytes,8+len){return Err(error("Invalid parameter wrapper checksum"));}data
    }else{bytes};
    std::str::from_utf8(text).map(str::trim).map_err(error)
}
fn inspect(base:&Path,path:&Path,file:&mut File,progress:&mut impl FnMut(Progress))->Result<Image>{
    let bytes=file.metadata().map_err(error)?.len();
    if bytes<512||bytes>LIMIT{return Err(fail("FACTORY_FORMAT","Select a complete manufacturer system image"));}
    let header=bounded(file,0,512)?;
    let (format,os,entries,partitions)=if &header[..4]==b"RKFW"{
        let output=run_tool(base,ToolAction::Info,path,progress)?;let entries=entries(&output,bytes)?;
        let p=entries.iter().find(|e|e.name=="parameter").unwrap();let data=bounded(file,p.offset,p.bytes)?;
        let partitions=parameter(parameter_payload(&data)?.trim_end_matches('\0'))?;
        for entry in &entries{if !["package-file","MiniLoaderAll.bin","parameter"].contains(&entry.name.as_str())&&!partitions.iter().any(|p|p.name==entry.name){return Err(error("Firmware entry has no partition"));}}
        let os=if entries.iter().any(|e|e.name=="rootfs"){"Linux"}else{"Android"};("RKFW",os,entries,partitions)
    }else{
        raw_layout(file)?;("RAW","Linux / other",vec![],vec![])
    };
    let sha256=digest(file,progress)?;
    let image=Image{sha256:sha256.clone(),path:fs::canonicalize(path).map_err(error)?,bytes,format:format.into(),os:os.into(),entries,partitions};
    Ok(image)
}
pub fn import(base:&Path,path:&Path,progress:&mut impl FnMut(Progress))->Result<Value>{
    let mut file=locked(path)?;let image=inspect(base,path,&mut file,progress)?;
    let dir=base.join("data/factory-images");fs::create_dir_all(&dir).map_err(error)?;
    fs::write(dir.join(format!("{}.json",image.sha256)),serde_json::to_vec(&image).map_err(error)?).map_err(error)?;
    Ok(image.report())
}
pub fn open(base:&Path,id:&str,progress:&mut impl FnMut(Progress))->Result<(Image,File)>{
    if !valid_id(id){return Err(error("Invalid manufacturer image ID"));}
    let p=base.join("data/factory-images").join(format!("{id}.json"));let mut r=locked(&p)?;
    if r.metadata().map_err(error)?.len()>128*1024{return Err(error("Invalid image receipt"));}
    let mut bytes=vec![];r.read_to_end(&mut bytes).map_err(error)?;let image:Image=serde_json::from_slice(&bytes).map_err(error)?;
    let mut file=locked(&image.path)?;
    let actual=inspect(base,&image.path,&mut file,progress)?;
    if image.sha256!=id||actual!=image{return Err(fail("SOURCE_CHANGED","Manufacturer image or receipt changed"));}
    Ok((image,file))
}
pub fn raw_layout(file:&mut File)->Result<(Vec<u8>,Vec<u8>,u32)>{
    let bytes=file.metadata().map_err(error)?.len();
    if bytes%512!=0||bytes<=34849*512||bytes/512>u32::MAX as u64{return Err(fail("FACTORY_FORMAT","Select a complete manufacturer system image"));}
    let a=bounded(file,0,34*512)?;let b=bounded(file,bytes-33*512,33*512)?;let sectors=(bytes/512) as u32;
    if a[510..512]!=[0x55,0xaa]{return Err(fail("FACTORY_FORMAT","Select a complete manufacturer system image"));}
    if &a[512..520]==b"EFI PART"{
        if crate::gpt::check(&a,&b,sectors)["healthy"]!=true{return Err(fail("FACTORY_FORMAT","Invalid full disk image"));}
        if a[1024..].windows(12).any(|s|s==b"h\0a\0s\0s\0o\0s\0"){return Err(fail("FACTORY_FORMAT","Use HAOS installation for official HAOS images"));}
    }else{
        let mut spans=vec![];
        for e in a[446..510].chunks_exact(16){if e[4]==0{continue;}let start=crate::gpt::u32at(e,8) as u64;let count=crate::gpt::u32at(e,12) as u64;
            if [0xee,0x05,0x0f,0x85].contains(&e[4])||start==0||count==0||start+count>sectors as u64{return Err(fail("FACTORY_FORMAT","Invalid full disk partition table"));}spans.push((start,start+count));}
        spans.sort_unstable();if spans.is_empty()||spans.windows(2).any(|p|p[0].1>p[1].0){return Err(fail("FACTORY_FORMAT","Select a complete manufacturer system image"));}
    }Ok((a,b,sectors))
}

#[derive(Serialize,Deserialize)]
struct Plan {format:u32,operation:String,source:String,device:Device,identity:Identity,source_report:Value,target_headers:String}
fn plans(base:&Path)->PathBuf{base.join("data/factory-plans")}
pub async fn plan(io:&mut impl UsbIo,base:&Path,device:&Device,source:&str,progress:&mut impl FnMut(Progress))->Result<Value>{
    crate::usb::guard(device)?;let identity=io.identity().await?;crate::usb::require_emmc(&identity.storage)?;crate::workflow::require_full_read(io).await?;
    let (image,mut file)=open(base,source,progress)?;
    if image.format=="RAW"{return crate::flash::plan_restore_direct(io,base,device,"factory-raw",source,progress).await;}
    if image.format!="RKFW"{return Err(error("Expected RKFW firmware"));}
    tool_bundle(base)?;
    validate_capacity(&image,identity.sectors)?;
    // Build/check the entire sparse layout before any invocation that can write.
    verification(&image,&mut file,identity.sectors)?;
    let p=Plan{format:1,operation:"factory".into(),source:source.into(),device:device.clone(),identity:identity.clone(),source_report:image.report(),target_headers:headers_digest(io,&identity).await?};
    let bytes=serde_json::to_vec(&p).map_err(error)?;let id=hash_bytes(&bytes);let dir=plans(base);fs::create_dir_all(&dir).map_err(error)?;
    if dir.join(format!("{id}.used.json")).exists(){return Err(fail("PLAN_CHANGED","This installation was already attempted"));}
    let pending=dir.join(format!("{id}.json"));if pending.exists(){if fs::read(&pending).map_err(error)?!=bytes{return Err(fail("PLAN_CHANGED","Manufacturer plan changed"));}}else{let mut f=OpenOptions::new().write(true).create_new(true).open(pending).map_err(error)?;f.write_all(&bytes).and_then(|_|f.sync_all()).map_err(error)?;}
    let ranges=image.partitions.iter().map(|p|json!({"label":p.name,"lba":p.lba,"bytes":p.sectors.unwrap_or(identity.sectors-33-p.lba) as u64*512})).collect::<Vec<_>>();
    Ok(json!({"plan_id":id,"operation":"factory","device":device,"identity":identity,"source":image.report(),"backup_path":null,"ranges":ranges,"erases_user_data":true,"no_changes":false}))
}
fn validate_capacity(image:&Image,sectors:u32)->Result<()>{
    if sectors<=34849||image.partitions.iter().any(|p|p.lba as u64+p.sectors.unwrap_or(1) as u64>sectors as u64-33){return Err(fail("CAPACITY","Firmware partitions do not fit the eMMC"));}Ok(())
}
pub fn target(base:&Path,id:&str)->Result<Device>{Ok(load_plan(base,id)?.device)}
fn load_plan(base:&Path,id:&str)->Result<Plan>{
    if !valid_id(id){return Err(fail("PLAN_ID","Invalid manufacturer plan"));}let path=plans(base).join(format!("{id}.json"));let mut f=locked(&path)?;
    if f.metadata().map_err(error)?.len()>256*1024{return Err(fail("PLAN_CHANGED","Invalid manufacturer plan"));}let mut bytes=vec![];f.read_to_end(&mut bytes).map_err(error)?;
    if hash_bytes(&bytes)!=id{return Err(fail("PLAN_CHANGED","Manufacturer plan changed"));}let p:Plan=serde_json::from_slice(&bytes).map_err(error)?;
    if p.format!=1||p.operation!="factory"||!valid_id(&p.source){return Err(fail("PLAN_CHANGED","Invalid manufacturer plan"));}Ok(p)
}
pub async fn execute(base:&Path,id:&str,confirmed:bool,progress:&mut impl FnMut(Progress))->Result<Value>{
    if !confirmed{return Err(fail("CONFIRM_WRITE","Explicit confirmation required"));}let p=load_plan(base,id)?;
    let (image,mut file)=open(base,&p.source,progress)?;
    if image.report()!=p.source_report{return Err(fail("PLAN_CHANGED","Manufacturer image metadata changed"));}
    validate_capacity(&image,p.identity.sectors)?;let checks=verification(&image,&mut file,p.identity.sectors)?;
    crate::usb::factory_identity(&p.device,&p.identity,&p.target_headers).await?;
    let dir=plans(base);fs::rename(dir.join(format!("{id}.json")),dir.join(format!("{id}.used.json"))).map_err(error)?;
    let journal=dir.join(format!("{id}.journal.jsonl"));let mut log=OpenOptions::new().create_new(true).write(true).open(&journal).map_err(error)?;
    writeln!(log,"{}",json!({"state":"started","plan":p})).and_then(|_|log.sync_all()).map_err(error)?;
    let result:Result<Value>=async{
        let text=run_tool(base,ToolAction::Install,&image.path,progress)?;
        writeln!(log,"{}",json!({"state":"vendor-complete","output":text})).and_then(|_|log.sync_all()).map_err(error)?;
        crate::usb::factory_readback(&p.device,&p.identity,&image,&mut file,&checks,progress).await?;
        progress(Progress::new("write-complete",1,1));
        Ok(json!({"operation":"factory","verified":true,"backup_path":null,"journal":journal,"tool":"Rockchip upgrade_tool","verification":"Vendor Loader checks; independent partition payload readback and GPT layout validation. Sparse DONT_CARE regions are not payload.","source":image.report()}))
    }.await;
    match result{Ok(value)=>{writeln!(log,"{}",json!({"state":"complete","result":value})).and_then(|_|log.sync_all()).map_err(error)?;Ok(value)},Err(e)=>{let _=writeln!(log,"{}",json!({"state":"failed","error":e}));Err(fail(&e.code,format!("{}; journal={}",e.detail,journal.display())))}}
}
pub async fn headers_digest(io:&mut impl UsbIo,identity:&Identity)->Result<String>{
    let (a,b)=crate::archive::headers(io,identity).await?;
    crate::workflow::require_gpt_bytes(&a[512..])?;crate::workflow::require_gpt_bytes(&b)?;
    Ok(hash_bytes(&[a,b].concat()))
}

#[derive(Debug)]
pub struct Check {pub lba:u32,pub bytes:u64,pub offset:u64,pub fill:Option<[u8;4]>}
fn verification(image:&Image,file:&mut File,sectors:u32)->Result<Vec<Check>>{
    let mut out=vec![];
    for entry in &image.entries{
        let Some(part)=image.partitions.iter().find(|p|p.name==entry.name)else{continue;};let capacity=part.sectors.unwrap_or(sectors-33-part.lba) as u64*512;
        if entry.sparse {sparse(file,entry,part.lba,capacity,&mut out)?;}else{
            if entry.bytes%512!=0||entry.bytes>capacity{return Err(error("Partition payload exceeds its range"));}
            out.push(Check{lba:part.lba,bytes:entry.bytes,offset:entry.offset,fill:None});
        }
    }if out.is_empty(){return Err(error("No firmware payload"));}Ok(out)
}
fn sparse(file:&mut File,entry:&Entry,lba:u32,capacity:u64,out:&mut Vec<Check>)->Result<()>{
    let h=bounded(file,entry.offset,28)?;let u16at=|o|u16::from_le_bytes([h[o],h[o+1]]);
    let (fh,ch,block,total,chunks)=(u16at(8) as u64,u16at(10) as u64,crate::gpt::u32at(&h,12) as u64,crate::gpt::u32at(&h,16) as u64,crate::gpt::u32at(&h,20) as u64);
    if h[..4]!=[0x3a,0xff,0x26,0xed]||u16at(4)!=1||fh<28||fh>4096||ch<12||ch>4096||block==0||block%512!=0||total.checked_mul(block).is_none_or(|n|n>capacity)||chunks>1000000{return Err(error("Invalid sparse system image"));}
    let end=entry.offset+entry.bytes;let mut pos=entry.offset+fh;let mut expanded=0u64;
    for _ in 0..chunks{
        if pos+ch>end{return Err(error("Truncated sparse image"));}let h=bounded(file,pos,ch)?;let kind=u16::from_le_bytes([h[0],h[1]]);let n=crate::gpt::u32at(&h,4) as u64*block;let count=crate::gpt::u32at(&h,8) as u64;
        let payload=count.checked_sub(ch).ok_or_else(||error("Invalid sparse chunk"))?;
        if pos.checked_add(count).is_none_or(|n|n>end)||expanded.checked_add(n).is_none_or(|n|n>total*block){return Err(error("Sparse chunk exceeds bounds"));}
        match kind{
            0xcac1 if payload==n&&n>0=>out.push(Check{lba:lba+(expanded/512) as u32,bytes:n,offset:pos+ch,fill:None}),
            0xcac2 if payload==4&&n>0=>{let b=bounded(file,pos+ch,4)?;out.push(Check{lba:lba+(expanded/512) as u32,bytes:n,offset:0,fill:Some(b.try_into().unwrap())});},
            0xcac3 if payload==0&&n>0=>{},
            0xcac4 if payload==4&&n==0=>{},
            _=>return Err(error("Invalid sparse chunk type/length")),
        }
        if out.len()>100000{return Err(error("Too many sparse payload ranges"));}expanded+=n;pos+=count;
    }if pos!=end||expanded!=total*block{return Err(error("Sparse image length mismatch"));}Ok(())
}
pub async fn readback(io:&mut impl UsbIo,identity:&Identity,image:&Image,file:&mut File,checks:&[Check],progress:&mut impl FnMut(Progress))->Result<()>{
    crate::workflow::require_full_read(io).await?;if io.identity().await?!=*identity{return Err(fail("DEVICE_CHANGED","Device identity changed"));}
    let total=checks.iter().map(|c|c.bytes).sum();let mut done=0;let mut expected=vec![0;1024*1024];let mut actual=vec![0;1024*1024];
    for c in checks{file.seek(SeekFrom::Start(c.offset)).map_err(error)?;let mut offset=0;
        while offset<c.bytes{let len=(c.bytes-offset).min(expected.len() as u64) as usize;
            if let Some(fill)=c.fill{for b in expected[..len].chunks_exact_mut(4){b.copy_from_slice(&fill);}}else{file.read_exact(&mut expected[..len]).map_err(error)?;}
            crate::workflow::read_exact(io,c.lba+(offset/512) as u32,&mut actual[..len]).await?;
            if expected[..len]!=actual[..len]{return Err(fail("WRITE_VERIFY",format!("Manufacturer payload mismatch at LBA {}",c.lba+offset as u32/512)));}
            offset+=len as u64;done+=len as u64;progress(Progress::new("verify-write",done,total));
        }
    }
    let (a,b)=crate::archive::headers(io,identity).await?;
    verify_layout(&a,&b,identity.sectors,image,file)
}
fn verify_layout(primary:&[u8],tail:&[u8],sectors:u32,image:&Image,file:&mut File)->Result<()>{
    if crate::gpt::check(primary,tail,sectors)["healthy"]!=true{return Err(fail("WRITE_VERIFY","Manufacturer GPT verification failed"));}
    let rows=primary[1024..].chunks_exact(128).filter(|e|e[..16].iter().any(|b|*b!=0)).collect::<Vec<_>>();
    if rows.len()!=image.partitions.len(){return Err(fail("WRITE_VERIFY","Manufacturer partition count changed"));}
    let full_end=sectors as u64-34;let mut actual=image.clone();
    for (i,(e,p)) in rows.iter().zip(&image.partitions).enumerate(){
        let name=String::from_utf16_lossy(&e[56..].chunks_exact(2).map(|b|u16::from_le_bytes([b[0],b[1]])).take_while(|n|*n!=0).collect::<Vec<_>>());
        let start=crate::gpt::u64at(e,32);let end=crate::gpt::u64at(e,40);
        let end_matches=match p.sectors{
            Some(count)=>end==p.lba as u64+count as u64-1,
            // The pinned vendor UF left a 31-sector tail on the real K11C:
            // exclusive grow end rounded down to 64 sectors (32 KiB).
            // Accept this shape or full capacity, never arbitrary shrinkage.
            None=>i+1==image.partitions.len()&&(end==full_end||end+1==((full_end+1)&!63)),
        };
        if name!=p.name||start!=p.lba as u64||!end_matches{return Err(fail("WRITE_VERIFY",format!("Manufacturer partition layout changed: {} expected start={}, size={:?}; actual {} start={}, end={}",p.name,p.lba,p.sectors,name,start,end)));}
        actual.partitions[i].sectors=Some((end-start+1) as u32);
    }
    // The full expanded sparse extent (including DONT_CARE), not just its
    // written chunks, must fit the partition the vendor actually created.
    verification(&actual,file,sectors).map_err(|e|fail("WRITE_VERIFY",format!("Manufacturer payload exceeds actual partition: {}",e.detail)))?;
    Ok(())
}

#[cfg(test)] pub(crate) mod tests;
