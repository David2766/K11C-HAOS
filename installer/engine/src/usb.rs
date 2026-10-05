use crate::{Result, fail, hash_bytes};
use serde::{Serialize, Deserialize};
use serde_json::{Value,json};
use crate::workflow::{UsbIo,Identity,Progress};
use std::{path::Path,time::{Duration,Instant}};
use crate::readiness::{self,State};

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct Device {
    pub instance_id: String, pub vid: u16, pub pid: u16, pub mode: String,
    pub binding: bool, pub location: String,
    #[serde(skip)] pub interface_path: Option<String>,
}
pub fn guard(device: &Device) -> Result<()> {
    if device.vid != 0x2207 || ![0x350a,0x300a].contains(&device.pid) {return Err(fail("UNSUPPORTED_DEVICE","Not an RK356x USB ID"));}
    if !["Maskrom","Loader"].contains(&device.mode.as_str()) {return Err(fail("USB_MODE","Unknown USB descriptor mode"));}
    if !device.binding {return Err(fail("DRIVER_BINDING","Device is not accessible through official Rockusb"));}
    Ok(())
}
pub fn require_emmc(storage: &str) -> Result<()> {
    if storage != "emmc" {return Err(fail("NOT_EMMC",format!("Selected storage is {storage}; no storage switch was sent")));}
    Ok(())
}
pub async fn devices() -> Result<Vec<Device>> {
    let raw = nusb::list_devices().await.map_err(|e|fail("USB_ENUM",e))?;
    let candidates: Vec<_> = raw.filter(|d|d.vendor_id()==0x2207).collect();
    if candidates.is_empty() {return Ok(vec![]);}
    candidates.into_iter().map(|d|{
        let id=d.instance_id().to_string_lossy().into_owned();
        // Ask Windows about this exact instance, not every unrelated USB service.
        let path=if d.driver().is_some_and(|s|s.eq_ignore_ascii_case("rockusb")) {crate::win::usb_interface(&id)?}else{None};
        Ok(Device { instance_id:id,vid:d.vendor_id(),pid:d.product_id(),
            mode:if d.usb_version() & 1 == 0 {"Maskrom"} else {"Loader"}.into(),
            binding:path.is_some(),interface_path:path,
            location:format!("{} {:?}",d.bus_id(),d.port_chain()) })
    }).collect()
}
pub async fn inspect(id: &str) -> Result<Value> {
    // Re-enumerate at execution, so a stale UI selection cannot open another board.
    let all=devices().await?;
    let d=all.iter().find(|d|d.instance_id==id).ok_or_else(||fail("DEVICE_GONE","Selected device is no longer connected"))?;
    guard(d)?;
    let path=d.interface_path.as_ref().ok_or_else(||fail("DRIVER_BINDING","Missing interface"))?;
    let mut io=WindowsIo::open(path)?;
    let info=io.identity().await?;require_emmc(&info.storage)?;
    let capabilities=crate::workflow::require_full_read(&mut io).await?;
    if info.sectors<68 {return Err(fail("INVALID_CAPACITY",info.sectors));}
    let (primary,backup)=crate::archive::headers(&mut io,&info).await?;
    let os=crate::archive::classify(&primary,&backup,info.sectors);
    crate::workflow::require_gpt_bytes(&primary[512..])?;crate::workflow::require_gpt_bytes(&backup)?;
    Ok(json!({"ready":true,"instance_id":id,"location":d.location,"storage":info.storage,"chip_hex":info.chip_hex,
        "bytes":info.sectors as u64*512,"sectors":info.sectors,"sector_size":512,"read_capability_hex":capabilities,
        "primary_gpt_signature":&primary[512..520]==b"EFI PART", "backup_gpt_signature":&backup[32*512..32*512+8]==b"EFI PART",
        "os":os,"kinds":if os=="HAOS"{vec!["FULL","BOOT","HAOS"]}else{vec!["FULL"]},
        "primary_sha256":hash_bytes(&primary),"backup_sha256":hash_bytes(&backup),
        "scope":"Read-only identity/capacity/header observation; not full GPT validation","persistent_writes":0}))
}
struct WindowsIo {dev:rockusb::windows::Device,full_read:bool}
impl WindowsIo {
    fn open(path:&str)->Result<Self>{
        readiness::require_ready(readiness::probe(path)?)?;
        Self::transport(path)
    }
    // Only preparation may open the BootROM transport without readiness.
    fn transport(path:&str)->Result<Self>{Ok(Self{dev:rockusb::windows::Device::from_interface_path(path).map_err(|e|fail("USB_OPEN",e))?,full_read:false})}
}
impl UsbIo for WindowsIo {
    async fn identity(&mut self)->Result<Identity>{
        self.dev.test_unit_ready().await.map_err(|e|fail("USB_NOT_READY",e))?;
        let chip=self.dev.chip_info().await.map_err(|e|fail("CHIP_QUERY",e))?;
        let storage=self.dev.storage().await.map_err(|e|fail("STORAGE_QUERY",e))?.to_string();
        let flash=self.dev.flash_info().await.map_err(|e|fail("FLASH_QUERY",e))?;
        let id=self.dev.flash_id().await.map_err(|e|fail("FLASH_QUERY",e))?;
        Ok(Identity{sectors:flash.sectors(),storage,chip_hex:chip.inner().iter().map(|b|format!("{b:02x}")).collect(),flash_id:id.to_str().into()})
    }
    async fn capability(&mut self)->Result<[u8;8]>{
        self.full_read=false;
        let caps=self.dev.capability().await.map_err(|e|fail("LOADER_CAPABILITY",e))?;
        self.full_read=caps.read_lba();
        Ok(caps.inner().try_into().map_err(|_|fail("LOADER_CAPABILITY","Invalid capability length"))?)
    }
    async fn read(&mut self,lba:u32,bytes:&mut[u8])->Result<u32>{
        access_guard(self.full_read)?;
        self.dev.read_lba(lba,bytes).await.map_err(|e|fail("USB_READ",e))
    }
    async fn upload(&mut self,area:u16,bytes:&[u8])->Result<()>{self.dev.write_maskrom_area(area,bytes).await.map_err(|e|fail("USB_UPLOAD",e))}
}
impl crate::flash::FlashIo for WindowsIo {
    async fn write(&mut self,lba:u32,bytes:&[u8])->Result<u32>{
        access_guard(self.full_read)?;
        if bytes.is_empty()||bytes.len()%512!=0||bytes.len()>1024*1024{return Err(fail("WRITE_RANGE","Invalid transport write chunk"));}
        self.dev.write_lba(lba,bytes).await.map_err(|e|fail("USB_WRITE",e))
    }
}
fn access_guard(full_read:bool)->Result<()>{
    if !full_read {return Err(fail("LOADER_READ_RESTRICTED","Full read capability must be established before storage access"));}Ok(())
}
async fn selected(id:&str)->Result<(Device,WindowsIo)>{
    let d=devices().await?.into_iter().find(|d|d.instance_id==id).ok_or_else(||fail("DEVICE_GONE","Selected device disconnected"))?;
    guard(&d)?;let io=WindowsIo::open(d.interface_path.as_ref().ok_or_else(||fail("DRIVER_BINDING","Missing interface"))?)?;Ok((d,io))
}
pub async fn storage_plan(id:&str,location:&str,op:&str,source:&str,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value>{
    let (d,mut io)=selected(id).await?;
    if d.location!=location{return Err(fail("DEVICE_CHANGED","Physical USB location changed"));}
    crate::flash::plan(&mut io,base,&d,op,source,progress).await
}
pub async fn storage_plan_backed(id:&str,location:&str,op:&str,source:&str,recovery:&str,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value>{
    let (d,mut io)=selected(id).await?;
    if d.location!=location{return Err(fail("DEVICE_CHANGED","Physical USB location changed"));}
    crate::flash::plan_backed(&mut io,base,&d,op,source,recovery,progress).await
}
pub async fn archive_backup(id:&str,kind:&str,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value>{
    let (_,mut io)=selected(id).await?;crate::archive::create(&mut io,base,kind,progress).await
}
pub async fn storage_plan_direct_restore(id:&str,location:&str,op:&str,source:&str,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value>{
    let (d,mut io)=selected(id).await?;
    if d.location!=location{return Err(fail("DEVICE_CHANGED","Physical USB location changed"));}
    crate::flash::plan_restore_direct(&mut io,base,&d,op,source,progress).await
}
pub async fn storage_execute(plan_id:&str,confirmed:bool,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value>{
    if !confirmed{return Err(fail("CONFIRM_WRITE","Explicit confirmation required"));}
    let target=crate::flash::target(base,plan_id)?;let (d,mut io)=selected(&target.instance_id).await?;
    crate::flash::execute(&mut io,base,&d,plan_id,confirmed,progress).await
}
pub async fn storage_plan_without_backup(id:&str,location:&str,op:&str,source:&str,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value>{
    let (d,mut io)=selected(id).await?;
    if d.location!=location{return Err(fail("DEVICE_CHANGED","Physical USB location changed"));}
    crate::flash::plan_without_backup(&mut io,base,&d,op,source,progress).await
}
pub async fn storage_execute_gated(plan_id:&str,base:&Path,progress:&mut impl FnMut(Progress),gate:&mut impl FnMut()->Result<()>)->Result<Value>{
    let target=crate::flash::target(base,plan_id)?;let (d,mut io)=selected(&target.instance_id).await?;
    crate::flash::execute_with_gate(&mut io,base,&d,plan_id,true,progress,gate).await
}
pub async fn gpt_check(id:&str)->Result<Value>{let (_,mut io)=selected(id).await?;crate::flash::check(&mut io).await}
// The vendor CLI selects its only Rockchip device, not a Windows port argument.
// Refuse any second VID 2207 device, including unsupported/unbound ones.
pub(crate) fn factory_device(all:&[Device],expected:&Device,reconnected:bool)->Result<Device>{
    if all.len()!=1{return Err(fail("AMBIGUOUS_DEVICE","Connect only one Rockchip device for manufacturer installation"));}
    let d=&all[0];guard(d)?;
    if d.location!=expected.location||(!reconnected&&d.instance_id!=expected.instance_id){return Err(fail("DEVICE_CHANGED","Manufacturer installation USB target changed"));}Ok(d.clone())
}
pub async fn factory_plan(id:&str,location:&str,source:&str,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value>{
    let (d,mut io)=selected(id).await?;
    if d.location!=location{return Err(fail("DEVICE_CHANGED","Physical USB location changed"));}
    factory_device(&devices().await?,&d,false)?;
    crate::factory::plan(&mut io,base,&d,source,progress).await
}
pub async fn factory_identity(expected:&Device,identity:&Identity,headers:&str)->Result<()>{
    let d=factory_device(&devices().await?,expected,false)?;let (_,mut io)=selected(&d.instance_id).await?;
    if io.identity().await?!=*identity{return Err(fail("DEVICE_CHANGED","Manufacturer target identity changed"));}
    crate::workflow::require_full_read(&mut io).await?;
    if crate::factory::headers_digest(&mut io,identity).await?!=headers{return Err(fail("DEVICE_CHANGED","Target partition headers changed since preview"));}Ok(())
}
pub async fn factory_readback(expected:&Device,identity:&Identity,image:&crate::factory::Image,file:&mut std::fs::File,checks:&[crate::factory::Check],progress:&mut impl FnMut(Progress))->Result<()>{
    let d=factory_device(&devices().await?,expected,true)?;let (_,mut io)=selected(&d.instance_id).await?;
    crate::factory::readback(&mut io,identity,image,file,checks,progress).await
}
pub async fn factory_execute(id:&str,confirmed:bool,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value>{
    if !confirmed{return Err(fail("CONFIRM_WRITE","Explicit confirmation required"));}
    if !crate::factory::valid_id(id){return Err(fail("PLAN_ID","Invalid manufacturer plan ID"));}
    if base.join("data/factory-plans").join(format!("{id}.json")).exists(){
        crate::factory::execute(base,id,confirmed,progress).await
    }else{
        // RAW images share the established bounded write/readback engine.
        crate::flash::require_factory_plan(base,id)?;
        storage_execute(id,confirmed,base,progress).await
    }
}
fn prepare_guard(d:&Device,location:&str,confirmed:bool)->Result<()> {
    if !confirmed {return Err(fail("CONFIRM_BOARD","Confirm the connected board is a K11C"));}
    if d.location != location {return Err(fail("DEVICE_CHANGED","USB port changed; select the device again"));}
    if d.vid!=0x2207||![0x350a,0x300a].contains(&d.pid){return Err(fail("UNSUPPORTED_DEVICE","Not an RK356x USB ID"));}
    if !d.binding {return Err(fail("DRIVER_BINDING","Rockusb binding required"));}
    if !["Maskrom","Loader"].contains(&d.mode.as_str()){return Err(fail("USB_MODE","Unknown device mode"));}
    Ok(())
}
fn same_port_device(all:&[Device],location:&str)->Result<Option<Device>> {
    let at_port:Vec<_>=all.iter().filter(|d|d.location==location).collect();
    if at_port.len()>1{return Err(fail("AMBIGUOUS_DEVICE","Multiple devices at the selected USB location"));}
    if let Some(d)=at_port.first() {
        guard(d)?;return Ok(Some((*d).clone()));
    }
    Ok(None)
}
trait Preparation {
    async fn scan(&mut self)->Result<Vec<Device>>;
    fn probe(&mut self,d:&Device)->Result<State>;
    async fn observe(&mut self,d:&Device)->Result<Value>;
    async fn upload(&mut self,d:&Device,progress:&mut impl FnMut(Progress))->Result<()>;
    async fn wait(&mut self);
}
struct WindowsPreparation<'a>{base:&'a Path}
fn interface(d:&Device)->Result<&str>{d.interface_path.as_deref().ok_or_else(||fail("DRIVER_BINDING","Missing USB interface"))}
impl Preparation for WindowsPreparation<'_>{
    async fn scan(&mut self)->Result<Vec<Device>>{devices().await}
    fn probe(&mut self,d:&Device)->Result<State>{readiness::probe(interface(d)?)}
    async fn observe(&mut self,d:&Device)->Result<Value>{inspect(&d.instance_id).await}
    async fn upload(&mut self,d:&Device,p:&mut impl FnMut(Progress))->Result<()>{
        let payload=crate::loader::load(self.base)?;
        let mut io=WindowsIo::transport(interface(d)?)?;
        crate::loader::upload(&mut io,&payload,p).await
    }
    async fn wait(&mut self){tokio::time::sleep(Duration::from_millis(500)).await;}
}
fn prepared(d:&Device,observation:Value,uploaded:bool)->Result<Value>{
    if observation["ready"]!=true || observation["instance_id"]!=d.instance_id || observation["location"]!=d.location {
        return Err(fail("DEVICE_CHANGED","Readiness observation does not match the selected USB device and port"));
    }
    Ok(json!({"device":d,"observation":observation,"uploaded":uploaded}))
}
async fn prepare_with(host:&mut impl Preparation,id:&str,location:&str,confirmed:bool,limit:Duration,progress:&mut impl FnMut(Progress))->Result<Value>{
    let all=host.scan().await?;
    let d=all.iter().find(|d|d.instance_id==id).ok_or_else(||fail("DEVICE_GONE","Selected device disconnected"))?;
    prepare_guard(d,location,confirmed)?;
    progress(Progress::new("inspect",0,0));
    match host.probe(d)? {
        State::Ready=>return prepared(d,host.observe(d).await?,false),
        State::Unavailable if d.mode=="Maskrom"=>{},
        _=>return Err(fail("USB_NOT_READY","Device is present but not ready; RAM Loader was not sent"))
    }
    host.upload(d,progress).await?;
    progress(Progress::new("reconnect",0,0));
    let start=Instant::now();
    let mut visible=false;
    loop {
        host.wait().await;
        if let Some(next)=same_port_device(&host.scan().await?,location)? {
            visible=true;
            match host.probe(&next) {
                Ok(State::Ready)=>return prepared(&next,host.observe(&next).await?,true),
                Ok(_)=>{},
                Err(e) if e.code=="USB_OPEN"||e.code=="DEVICE_GONE"=>{},
                Err(e)=>return Err(e),
            }
        }
        if start.elapsed()>=limit {break;}
    }
    if visible {Err(fail("USB_NOT_READY","Device reappeared on the selected port but did not answer the Rockusb readiness query"))}
    else {Err(fail("LOADER_RECONNECT","Device did not reappear on the same USB port"))}
}
pub async fn prepare(id:&str,location:&str,confirmed:bool,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value> {
    prepare_with(&mut WindowsPreparation{base},id,location,confirmed,Duration::from_secs(25),progress).await
}
pub async fn backup(id:&str,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value> {
    let all=devices().await?;let d=all.iter().find(|d|d.instance_id==id).ok_or_else(||fail("DEVICE_GONE","Selected device disconnected"))?;
    guard(d)?;
    let mut io=WindowsIo::open(d.interface_path.as_ref().ok_or_else(||fail("DRIVER_BINDING","Missing interface"))?)?;
    crate::workflow::backup(&mut io,&base.join("data/backups"),d,progress).await
}
#[cfg(test)] mod tests {
    use super::*;
    fn valid()->Device {Device {instance_id:"test".into(),vid:0x2207,pid:0x350a,mode:"Loader".into(),binding:true,location:"test".into(),interface_path:None}}
    #[test] fn supported_descriptors_require_binding_and_separate_readiness() {
        guard(&valid()).unwrap();
        let mut d=valid();d.vid=0;assert!(guard(&d).is_err());
        let mut d=valid();d.pid=0x330c;assert!(guard(&d).is_err());
        let mut d=valid();d.mode="Maskrom".into();guard(&d).unwrap();
        d.mode="Unknown".into();assert!(guard(&d).is_err());
        let mut d=valid();d.binding=false;assert!(guard(&d).is_err());
    }
    #[test] fn no_accidental_sd_or_other_storage() {
        require_emmc("emmc").unwrap();
        for storage in ["sd0","spinor","ram","unknown",""] {assert!(require_emmc(storage).is_err());}
    }
    #[test] fn storage_transport_requires_read_capability_even_for_writes(){
        assert_eq!(access_guard(false).unwrap_err().code,"LOADER_READ_RESTRICTED");access_guard(true).unwrap();
    }
    #[test] fn preparation_confirmation_and_physical_selection(){
        let d=valid();prepare_guard(&d,"test",true).unwrap();
        assert!(prepare_guard(&d,"test",false).is_err());assert!(prepare_guard(&d,"other-port",true).is_err());
        let mut m=d.clone();m.mode="Maskrom".into();prepare_guard(&m,"test",true).unwrap();
        assert_eq!(same_port_device(&[m],"test").unwrap().unwrap().mode,"Maskrom");
        assert!(same_port_device(&[d.clone()],"other-port").unwrap().is_none());
        assert!(same_port_device(&[d.clone(),d.clone()],"test").is_err());
        assert_eq!(same_port_device(&[d],"test").unwrap().unwrap().instance_id,"test");
    }
    struct Host {first:Device,next:Vec<Device>,states:std::collections::VecDeque<State>,scans:u32,uploads:u32,observations:u32,bad_observation:bool}
    impl Host {fn new(mode:&str,states:&[State])->Self{let mut d=valid();d.mode=mode.into();Self{first:d.clone(),next:vec![d],states:states.iter().copied().collect(),scans:0,uploads:0,observations:0,bad_observation:false}}}
    impl Preparation for Host {
        async fn scan(&mut self)->Result<Vec<Device>>{self.scans+=1;Ok(if self.scans==1{vec![self.first.clone()]}else{self.next.clone()})}
        fn probe(&mut self,_:&Device)->Result<State>{Ok(self.states.pop_front().expect("unexpected extra probe"))}
        async fn observe(&mut self,d:&Device)->Result<Value>{self.observations+=1;Ok(json!({"ready":!self.bad_observation,"instance_id":d.instance_id,"location":d.location}))}
        async fn upload(&mut self,_:&Device,_:&mut impl FnMut(Progress))->Result<()>{self.uploads+=1;Ok(())}
        async fn wait(&mut self){}
    }
    async fn run(h:&mut Host)->Result<Value>{prepare_with(h,"test","test",true,Duration::ZERO,&mut |_|{}).await}
    #[tokio::test] async fn ready_device_never_reuploads_even_with_maskrom_descriptor(){
        for mode in ["Maskrom","Loader"] {let mut h=Host::new(mode,&[State::Ready]);let r=run(&mut h).await.unwrap();assert_eq!(r["uploaded"],false);assert_eq!(h.uploads,0);assert_eq!(h.observations,1);}
    }
    #[tokio::test] async fn bootrom_uploads_once_and_accepts_either_ready_descriptor(){
        for mode in ["Maskrom","Loader"] {let mut h=Host::new("Maskrom",&[State::Unavailable,State::Ready]);h.next[0].mode=mode.into();h.next[0].instance_id="reconnected".into();let r=run(&mut h).await.unwrap();assert_eq!(r["uploaded"],true);assert_eq!(h.uploads,1);assert_eq!(r["device"]["mode"],mode);}
    }
    #[tokio::test] async fn no_readiness_no_advance_and_no_repeat_upload(){
        for state in [State::Unavailable,State::Busy] {let mut h=Host::new("Maskrom",&[State::Unavailable,state]);assert_eq!(run(&mut h).await.unwrap_err().code,"USB_NOT_READY");assert_eq!(h.uploads,1);assert_eq!(h.observations,0);}
        for mode in ["Loader","Maskrom"] {let mut h=Host::new(mode,&[State::Busy]);assert!(run(&mut h).await.is_err());assert_eq!(h.uploads,0);}
        let mut h=Host::new("Loader",&[State::Unavailable]);assert!(run(&mut h).await.is_err());assert_eq!(h.uploads,0);
        let mut h=Host::new("Maskrom",&[State::Ready]);h.bad_observation=true;assert!(run(&mut h).await.is_err());assert_eq!(h.uploads,0);
    }
    #[tokio::test] async fn another_port_never_completes_preparation(){
        let mut h=Host::new("Maskrom",&[State::Unavailable]);h.next[0].location="other-port".into();assert_eq!(run(&mut h).await.unwrap_err().code,"LOADER_RECONNECT");assert_eq!(h.observations,0);assert_eq!(h.uploads,1);
    }
}
