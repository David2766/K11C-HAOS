use crate::{Result, fail, hash_bytes};
use serde::{Serialize, Deserialize};
use serde_json::{Value,json};
use crate::workflow::{UsbIo,Identity,Progress,read_exact};
use std::{path::Path,time::{Duration,Instant}};

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct Device {
    pub instance_id: String, pub vid: u16, pub pid: u16, pub mode: String,
    pub binding: bool, pub location: String,
    #[serde(skip)] pub interface_path: Option<String>,
}
pub fn guard(device: &Device) -> Result<()> {
    if device.vid != 0x2207 || ![0x350a,0x300a].contains(&device.pid) {return Err(fail("UNSUPPORTED_DEVICE","Not an RK356x USB ID"));}
    if device.mode != "Loader" {return Err(fail("LOADER_REQUIRED","Storage reads require Loader mode; nothing was uploaded"));}
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
    if info.sectors<68 {return Err(fail("INVALID_CAPACITY",info.sectors));}
    let mut primary=vec![0u8;1024];
    read_exact(&mut io,0,&mut primary).await?;
    let mut backup=vec![0u8;512];
    read_exact(&mut io,info.sectors-1,&mut backup).await?;
    Ok(json!({"instance_id":id,"storage":info.storage,"chip_hex":info.chip_hex,
        "bytes":info.sectors as u64*512,"sectors":info.sectors,"sector_size":512,
        "primary_gpt_signature":&primary[512..520]==b"EFI PART", "backup_gpt_signature":&backup[..8]==b"EFI PART",
        "primary_sha256":hash_bytes(&primary),"backup_sha256":hash_bytes(&backup),
        "scope":"Read-only identity/capacity/header observation; not full GPT validation","persistent_writes":0}))
}
struct WindowsIo {dev:rockusb::windows::Device}
impl WindowsIo {
    fn open(path:&str)->Result<Self>{Ok(Self{dev:rockusb::windows::Device::from_interface_path(path).map_err(|e|fail("USB_OPEN",e))?})}
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
    async fn read(&mut self,lba:u32,bytes:&mut[u8])->Result<u32>{self.dev.read_lba(lba,bytes).await.map_err(|e|fail("USB_READ",e))}
    async fn upload(&mut self,area:u16,bytes:&[u8])->Result<()>{self.dev.write_maskrom_area(area,bytes).await.map_err(|e|fail("USB_UPLOAD",e))}
}
impl crate::flash::FlashIo for WindowsIo {
    async fn write(&mut self,lba:u32,bytes:&[u8])->Result<u32>{
        if bytes.is_empty()||bytes.len()%512!=0||bytes.len()>1024*1024{return Err(fail("WRITE_RANGE","Invalid transport write chunk"));}
        self.dev.write_lba(lba,bytes).await.map_err(|e|fail("USB_WRITE",e))
    }
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
pub async fn storage_execute(plan_id:&str,confirmed:bool,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value>{
    if !confirmed{return Err(fail("CONFIRM_WRITE","Explicit confirmation required"));}
    let target=crate::flash::target(base,plan_id)?;let (d,mut io)=selected(&target.instance_id).await?;
    crate::flash::execute(&mut io,base,&d,plan_id,confirmed,progress).await
}
pub async fn gpt_check(id:&str)->Result<Value>{let (_,mut io)=selected(id).await?;crate::flash::check(&mut io).await}
fn prepare_guard(d:&Device,location:&str,confirmed:bool)->Result<()> {
    if !confirmed {return Err(fail("CONFIRM_BOARD","Confirm the connected board is a K11C"));}
    if d.location != location {return Err(fail("DEVICE_CHANGED","USB port changed; select the device again"));}
    if d.vid!=0x2207||![0x350a,0x300a].contains(&d.pid){return Err(fail("UNSUPPORTED_DEVICE","Not an RK356x USB ID"));}
    if !d.binding {return Err(fail("DRIVER_BINDING","Rockusb binding required"));}
    if !["Maskrom","Loader"].contains(&d.mode.as_str()){return Err(fail("USB_MODE","Unknown device mode"));}
    Ok(())
}
fn same_port_loader(all:&[Device],location:&str)->Result<Option<Device>> {
    let at_port:Vec<_>=all.iter().filter(|d|d.location==location).collect();
    if at_port.len()>1{return Err(fail("AMBIGUOUS_DEVICE","Multiple devices at the selected USB location"));}
    if let Some(d)=at_port.first() {
        if d.mode=="Loader" {guard(d)?;return Ok(Some((*d).clone()));}
    }
    Ok(None)
}
pub async fn prepare(id:&str,location:&str,confirmed:bool,base:&Path,progress:&mut impl FnMut(Progress))->Result<Value> {
    let all=devices().await?;
    let d=all.iter().find(|d|d.instance_id==id).ok_or_else(||fail("DEVICE_GONE","Selected device disconnected"))?;
    prepare_guard(d,location,confirmed)?;
    if d.mode=="Loader" {return Ok(json!({"device":d,"observation":inspect(id).await?,"uploaded":false}));}
    let payload=crate::loader::load(base)?;
    let path=d.interface_path.as_ref().ok_or_else(||fail("DRIVER_BINDING","Missing USB interface"))?;
    {
        let mut io=WindowsIo::open(path)?;
        crate::loader::upload(&mut io,&payload,progress).await?;
    }
    progress(Progress::new("reconnect",0,0));
    let start=Instant::now();
    while start.elapsed()<Duration::from_secs(25) {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if let Some(next)=same_port_loader(&devices().await?,location)? {
            let observation=inspect(&next.instance_id).await?;
            return Ok(json!({"device":next,"observation":observation,"uploaded":true}));
        }
    }
    Err(fail("LOADER_RECONNECT","Loader did not reappear on the same USB port"))
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
    #[test] fn only_supported_loader_binding_can_be_read() {
        guard(&valid()).unwrap();
        let mut d=valid();d.vid=0;assert!(guard(&d).is_err());
        let mut d=valid();d.pid=0x330c;assert!(guard(&d).is_err());
        let mut d=valid();d.mode="Maskrom".into();assert!(guard(&d).is_err());
        let mut d=valid();d.binding=false;assert!(guard(&d).is_err());
    }
    #[test] fn no_accidental_sd_or_other_storage() {
        require_emmc("emmc").unwrap();
        for storage in ["sd0","spinor","ram","unknown",""] {assert!(require_emmc(storage).is_err());}
    }
    #[test] fn preparation_confirmation_and_physical_selection(){
        let d=valid();prepare_guard(&d,"test",true).unwrap();
        assert!(prepare_guard(&d,"test",false).is_err());assert!(prepare_guard(&d,"other-port",true).is_err());
        let mut m=d.clone();m.mode="Maskrom".into();prepare_guard(&m,"test",true).unwrap();
        assert!(same_port_loader(&[m],"test").unwrap().is_none());
        assert!(same_port_loader(&[d.clone()],"other-port").unwrap().is_none());
        assert!(same_port_loader(&[d.clone(),d.clone()],"test").is_err());
        assert_eq!(same_port_loader(&[d],"test").unwrap().unwrap().instance_id,"test");
    }
}
