use super::*;
use std::time::{SystemTime,UNIX_EPOCH};
struct Sandbox(PathBuf);
impl Sandbox{fn new()->Self{let p=std::env::temp_dir().join(format!("k11c-factory-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&p).unwrap();Self(p)}}
impl Drop for Sandbox{fn drop(&mut self){fs::remove_dir_all(&self.0).unwrap();}}
fn device()->Device{Device{instance_id:"K11C".into(),vid:0x2207,pid:0x350a,mode:"Loader".into(),binding:true,location:"PORT1".into(),interface_path:None}}
fn table(sectors:u32)->(Vec<u8>,Vec<u8>){
    use crate::gpt::*;
    let mut a=vec![0;34*512];a[510..512].copy_from_slice(&[0x55,0xaa]);a[450]=0xee;put32(&mut a,454,1);put32(&mut a,458,sectors-1);
    a[1024]=1;put64(&mut a,1024+32,34816);put64(&mut a,1024+40,34823);
    for (i,n) in "rootfs".encode_utf16().enumerate(){a[1080+i*2..1082+i*2].copy_from_slice(&n.to_le_bytes());}
    let ec=crc(&a[1024..]);let h=&mut a[512..1024];h[..8].copy_from_slice(b"EFI PART");put32(h,8,0x10000);put32(h,12,92);put64(h,24,1);put64(h,32,sectors as u64-1);put64(h,40,34);put64(h,48,sectors as u64-34);put64(h,72,2);put32(h,80,128);put32(h,84,128);put32(h,88,ec);seal(h);
    let mut b=vec![0;33*512];b[..16384].copy_from_slice(&a[1024..]);b[16384..].copy_from_slice(&a[512..1024]);let h=&mut b[16384..];put64(h,24,sectors as u64-1);put64(h,32,1);put64(h,72,sectors as u64-33);seal(h);
    assert_eq!(check(&a,&b,sectors)["healthy"],true);(a,b)
}
pub fn disk(sectors:u32)->Vec<u8>{let (a,b)=table(sectors);let mut bytes=vec![0xa5;sectors as usize*512];bytes[..a.len()].copy_from_slice(&a);let n=bytes.len();bytes[n-b.len()..].copy_from_slice(&b);bytes}
fn info(os:&str)->String{
    let names=["MiniLoaderAll.bin","parameter","uboot","boot",if os=="Linux"{"rootfs"}else{"super"}];
    let mut out="Type:Update Firmware\nChip Tag:3568 Version:1\nEntry Count:5\n".to_owned();
    for (i,name) in names.iter().enumerate(){out+=&format!("EntryNo={i}; file={name}; partition={name}; type=image; offset=0x{:x}; size=0x200\n",0x200+i*0x200);}out
}
#[test] fn android_and_linux_use_same_vendor_parser(){
    for os in ["Android","Linux"]{assert_eq!(entries(&info(os),8192).unwrap().len(),5);}
    for bad in [info("Linux").replace("Chip Tag:3568","Chip Tag:3588"),info("Linux").replace("rootfs","misc"),info("Linux").replace("Entry Count:5","Entry Count:6"),info("Linux").replace("offset=0x600","offset=0x400")]{assert!(entries(&bad,8192).is_err());}
    assert!(entries(&info("Linux"),1024).is_err());
    assert_eq!(entries(&info("Linux").replace("partition=MiniLoaderAll.bin","partition=bootloader"),8192).unwrap()[0].name,"MiniLoaderAll.bin");
    let p=parameter("TYPE: GPT\nCMDLINE: mtdparts=rk29xxnand:0x8@0x8800(rootfs),-@0x9000(userdata:grow)\n").unwrap();assert_eq!(p[0].lba,34816);assert_eq!(p[1].sectors,None);
    for bad in ["TYPE: MBR\nCMDLINE: mtdparts=x:0x1@0x8800(rootfs)","TYPE: GPT\nCMDLINE: mtdparts=x:0x20@0x8800(rootfs),0x10@0x8810(boot)","TYPE: GPT\nCMDLINE: mtdparts=x:-@0x8800(rootfs),0x10@0x9000(boot)"]{assert!(parameter(bad).is_err());}
}
#[test] fn rockchip_parameter_wrapper_checks_length_and_checksum(){
    let data=b"TYPE: GPT\nCMDLINE:mtdparts=rk29xxnand:0x8@0x8800(rootfs)\n";
    let mut crc=0u32;for b in data{crc^=(*b as u32)<<24;for _ in 0..8{crc=(crc<<1)^if crc&0x80000000!=0{0x04c10db7}else{0};}}
    let mut b=b"PARM".to_vec();b.extend((data.len() as u32).to_le_bytes());b.extend(data);b.extend(crc.to_le_bytes());assert_eq!(parameter(parameter_payload(&b).unwrap()).unwrap()[0].name,"rootfs");
    assert_eq!(parameter_payload(data).unwrap(),std::str::from_utf8(data).unwrap().trim());
    let n=b.len();b[n-1]^=1;assert!(parameter_payload(&b).is_err());b[n-1]^=1;b[4]^=1;assert!(parameter_payload(&b).is_err());assert!(parameter_payload(b"PARM").is_err());
}
#[test] fn vendor_commands_and_success_are_fixed(){
    assert_eq!(tool_args(ToolAction::Info,Path::new("C:/image with spaces.img")),["SFI","C:/image with spaces.img"].map(std::ffi::OsString::from));
    assert_eq!(tool_args(ToolAction::Install,Path::new("C:/image.img")),["UF","C:/image.img","-noreset"].map(std::ffi::OsString::from));
    assert!(tool_success(ToolAction::Info,true,&info("Linux")));
    assert!(tool_success(ToolAction::Install,true,"Upgrade firmware ok.\n"));
    for s in ["", "Upgrade firmware ok.\nERROR", "Upgrade firmware fail."]{assert!(!tool_success(ToolAction::Install,true,s));}
    assert!(!tool_success(ToolAction::Install,false,"Upgrade firmware ok."));
}
#[test] fn actual_vendor_bundle_digest_guard(){
    let base=Path::new(env!("CARGO_MANIFEST_DIR")).join("..");tool_bundle(&base).unwrap();
    let s=Sandbox::new();fs::create_dir_all(s.0.join("resources/rockchip")).unwrap();
    for n in ["upgrade_tool.exe","config.ini"]{fs::copy(base.join("resources/rockchip").join(n),s.0.join("resources/rockchip").join(n)).unwrap();}
    tool_bundle(&s.0).unwrap();fs::write(s.0.join("resources/rockchip/config.ini"),b"rb_check_off=1").unwrap();assert_eq!(tool_bundle(&s.0).unwrap_err().code,"RESOURCE_HASH");
}
#[test] fn source_and_receipt_revalidated_and_partition_images_rejected(){
    let s=Sandbox::new();let path=s.0.join("linux.img");let original=disk(36864);fs::write(&path,&original).unwrap();
    let report=import(&s.0,&path,&mut |_|{}).unwrap();let id=report["id"].as_str().unwrap();assert_eq!(report["format"],"RAW");let (image,file)=open(&s.0,id,&mut |_|{}).unwrap();drop(file);
    let receipt=s.0.join("data/factory-images").join(format!("{id}.json"));let mut changed=image;changed.os="Wrong".into();fs::write(&receipt,serde_json::to_vec(&changed).unwrap()).unwrap();assert_eq!(open(&s.0,id,&mut |_|{}).err().unwrap().code,"SOURCE_CHANGED");
    import(&s.0,&path,&mut |_|{}).unwrap();let mut modified=original;modified[35000*512]^=1;fs::write(&path,&modified).unwrap();assert_eq!(open(&s.0,id,&mut |_|{}).err().unwrap().code,"SOURCE_CHANGED");
    for name in ["rootfs.img","boot.img"]{let p=s.0.join(name);fs::write(&p,vec![0;40000*512]).unwrap();assert_eq!(import(&s.0,&p,&mut |_|{}).unwrap_err().code,"FACTORY_FORMAT");}
}
#[test] fn generic_raw_layout_gpt_resize_and_mbr_validation(){
    let s=Sandbox::new();let p=s.0.join("raw.img");fs::write(&p,disk(36864)).unwrap();let mut f=locked(&p).unwrap();let (a,b,n)=raw_layout(&mut f).unwrap();
    let (large,tail)=crate::gpt::resized(&a,&b,n,40000).unwrap();assert_eq!(&large[1024..],&a[1024..]);assert_eq!(crate::gpt::check(&large,&tail,40000)["healthy"],true);assert!(crate::gpt::resized(&a,&b,n,n-1).is_err());drop(f);
    let mut bytes=vec![0;36864*512];bytes[510..512].copy_from_slice(&[0x55,0xaa]);bytes[450]=0x83;crate::gpt::put32(&mut bytes,454,2048);crate::gpt::put32(&mut bytes,458,34000);fs::write(&p,&bytes).unwrap();raw_layout(&mut locked(&p).unwrap()).unwrap();
    crate::gpt::put32(&mut bytes,458,40000);fs::write(&p,&bytes).unwrap();assert!(raw_layout(&mut locked(&p).unwrap()).is_err());
}
fn sparse_bytes()->Vec<u8>{
    let mut b=vec![0;28];b[..4].copy_from_slice(&[0x3a,0xff,0x26,0xed]);b[4..6].copy_from_slice(&1u16.to_le_bytes());b[8..10].copy_from_slice(&28u16.to_le_bytes());b[10..12].copy_from_slice(&12u16.to_le_bytes());crate::gpt::put32(&mut b,12,512);crate::gpt::put32(&mut b,16,3);crate::gpt::put32(&mut b,20,3);
    for (kind,data) in [(0xcac1u16,vec![0x77;512]),(0xcac2,vec![1,2,3,4]),(0xcac3,vec![])]{let mut h=vec![0;12];h[..2].copy_from_slice(&kind.to_le_bytes());crate::gpt::put32(&mut h,4,1);crate::gpt::put32(&mut h,8,12+data.len() as u32);b.extend(h);b.extend(data);}b
}
#[test] fn sparse_expansion_bounds_and_payload_ranges(){
    let s=Sandbox::new();let p=s.0.join("sparse");let bytes=sparse_bytes();fs::write(&p,&bytes).unwrap();let entry=Entry{name:"super".into(),offset:0,bytes:bytes.len() as u64,sparse:true};
    let mut checks=vec![];sparse(&mut locked(&p).unwrap(),&entry,34816,1536,&mut checks).unwrap();assert_eq!(checks.len(),2);assert_eq!(checks[0].lba,34816);assert_eq!(checks[1].lba,34817);assert_eq!(checks[1].fill,Some([1,2,3,4]));
    assert!(sparse(&mut locked(&p).unwrap(),&entry,34816,1024,&mut vec![]).is_err());
    let mut bad=bytes;crate::gpt::put32(&mut bad,28+8,1024);fs::write(&p,&bad).unwrap();assert!(sparse(&mut locked(&p).unwrap(),&entry,34816,1536,&mut vec![]).is_err());
}
#[test] fn vendor_target_is_unique_same_port_and_identity_scope(){
    let d=device();crate::usb::factory_device(&[d.clone()],&d,false).unwrap();
    assert!(crate::usb::factory_device(&[],&d,false).is_err());assert!(crate::usb::factory_device(&[d.clone(),d.clone()],&d,false).is_err());
    let mut other=d.clone();other.location="PORT2".into();assert!(crate::usb::factory_device(&[other],&d,true).is_err());
    let mut reconnect=d.clone();reconnect.instance_id="new".into();assert!(crate::usb::factory_device(&[reconnect.clone()],&d,false).is_err());crate::usb::factory_device(&[reconnect],&d,true).unwrap();
}
#[tokio::test] async fn confirmation_and_dispatch_gate_precede_usb_or_tool(){
    let s=Sandbox::new();let id="a".repeat(64);
    assert_eq!(execute(&s.0,&id,false,&mut |_|{}).await.unwrap_err().code,"CONFIRM_WRITE");
    assert_eq!(crate::usb::factory_execute(&id,false,&s.0,&mut |_|{}).await.unwrap_err().code,"CONFIRM_WRITE");
    for args in [vec!["factory-execute",id.as_str()],vec!["factory-execute",id.as_str(),"--yes"],vec!["factory-plan","id","port","bad"],vec!["factory-import","x","UF"]]{assert!(crate::parse_action(&args.iter().map(|s|s.to_string()).collect::<Vec<_>>()).is_err());}
    assert!(crate::parse_action(&["factory-execute",id.as_str(),crate::flash::confirmed_flag()].map(str::to_owned)).is_ok());
}
struct Fake{disk:Vec<u8>,restricted:bool,changed:bool,short:bool}
impl UsbIo for Fake{
    async fn identity(&mut self)->Result<Identity>{Ok(Identity{sectors:(self.disk.len()/512) as u32,storage:"emmc".into(),chip_hex:if self.changed{"changed"}else{"3566"}.into(),flash_id:"EMMC".into()})}
    async fn capability(&mut self)->Result<[u8;8]>{Ok([if self.restricted{0}else{8},0,0,0,0,0,0,0])}
    async fn read(&mut self,lba:u32,b:&mut[u8])->Result<u32>{let pos=lba as usize*512;b.copy_from_slice(&self.disk[pos..pos+b.len()]);Ok((b.len()-if self.short{512}else{0}) as u32)}
    async fn upload(&mut self,_:u16,_:&[u8])->Result<()>{panic!("Readback cannot upload")}
}
#[tokio::test] async fn every_vendor_payload_byte_and_gpt_are_checked(){
    let s=Sandbox::new();let path=s.0.join("payload");fs::write(&path,vec![0xa5;4096]).unwrap();let mut file=locked(&path).unwrap();
    let mut io=Fake{disk:disk(36864),restricted:false,changed:false,short:false};let identity=io.identity().await.unwrap();
    let image=Image{sha256:"a".repeat(64),path,bytes:4096,format:"RKFW".into(),os:"Linux".into(),entries:vec![Entry{name:"rootfs".into(),offset:0,bytes:4096,sparse:false}],partitions:vec![Partition{name:"rootfs".into(),lba:34816,sectors:Some(8)}]};let checks=[Check{lba:34816,bytes:4096,offset:0,fill:None}];
    readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await.unwrap();
    io.disk[34823*512+511]^=1;assert_eq!(readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await.unwrap_err().code,"WRITE_VERIFY");io.disk[34823*512+511]^=1;
    io.disk[(36864-1)*512+16]^=1;assert_eq!(readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await.unwrap_err().code,"WRITE_VERIFY");io.disk[(36864-1)*512+16]^=1;
    io.restricted=true;assert_eq!(readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await.unwrap_err().code,"LOADER_READ_RESTRICTED");io.restricted=false;
    io.changed=true;assert_eq!(readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await.unwrap_err().code,"DEVICE_CHANGED");io.changed=false;
    io.short=true;assert_eq!(readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await.unwrap_err().code,"SHORT_READ");
}
fn reseal_table(a:&mut [u8],sectors:u32)->Vec<u8>{
    use crate::gpt::*;
    let ec=crc(&a[1024..]);put32(&mut a[512..1024],88,ec);seal(&mut a[512..1024]);
    let mut b=vec![0;33*512];b[..16384].copy_from_slice(&a[1024..]);b[16384..].copy_from_slice(&a[512..1024]);
    let h=&mut b[16384..];put64(h,24,sectors as u64-1);put64(h,32,1);put64(h,72,sectors as u64-33);seal(h);b
}
fn grow_table(sectors:u32,end:u64)->(Vec<u8>,Vec<u8>){
    let (mut a,_)=table(sectors);let e=&mut a[1024+128..1024+256];e[0]=1;
    crate::gpt::put64(e,32,34824);crate::gpt::put64(e,40,end);
    for(i,n)in "userdata".encode_utf16().enumerate(){e[56+i*2..58+i*2].copy_from_slice(&n.to_le_bytes());}
    let b=reseal_table(&mut a,sectors);assert_eq!(crate::gpt::check(&a,&b,sectors)["healthy"],true);(a,b)
}
fn set_headers(io:&mut Fake,a:&[u8],b:&[u8]){
    io.disk[..a.len()].copy_from_slice(a);let len=io.disk.len();io.disk[len-b.len()..].copy_from_slice(b);
}
fn grow_image(path:PathBuf)->Image{
    Image{sha256:"a".repeat(64),path,bytes:4096,format:"RKFW".into(),os:"Linux".into(),
        entries:vec![Entry{name:"rootfs".into(),offset:0,bytes:4096,sparse:false}],
        partitions:vec![Partition{name:"rootfs".into(),lba:34816,sectors:Some(8)},Partition{name:"userdata".into(),lba:34824,sectors:None}]}
}
#[tokio::test] async fn grow_full_and_vendor_aligned_ends_pass_production_readback(){
    let s=Sandbox::new();let path=s.0.join("payload");fs::write(&path,vec![0xa5;4096]).unwrap();
    let image=grow_image(path.clone());let mut file=locked(&path).unwrap();
    for sectors in [36864,36865,36897]{
        let mut io=Fake{disk:disk(sectors),restricted:false,changed:false,short:false};let identity=io.identity().await.unwrap();
        let checks=verification(&image,&mut file,sectors).unwrap();let full=sectors as u64-34;
        for end in [full,((full+1)&!63)-1]{let(a,b)=grow_table(sectors,end);set_headers(&mut io,&a,&b);
            readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await.unwrap();}
    }
}
#[tokio::test] async fn grow_layout_still_rejects_fixed_changes_and_arbitrary_tail(){
    let s=Sandbox::new();let path=s.0.join("payload");fs::write(&path,vec![0xa5;4096]).unwrap();let image=grow_image(path.clone());let mut file=locked(&path).unwrap();
    let sectors=36864;let mut io=Fake{disk:disk(sectors),restricted:false,changed:false,short:false};let identity=io.identity().await.unwrap();let checks=verification(&image,&mut file,sectors).unwrap();
    let full=sectors as u64-34;let aligned=((full+1)&!63)-1;
    let(a,b)=grow_table(sectors,aligned);set_headers(&mut io,&a,&b);
    readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await.unwrap();
    for end in [full-1,aligned-1,aligned-64]{let(a,b)=grow_table(sectors,end);set_headers(&mut io,&a,&b);
        assert!(readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await.unwrap_err().detail.contains("partition layout changed"));}
    for change in ["fixed-end","fixed-start","fixed-name","grow-start","grow-name","count"]{
        let(mut a,_)=grow_table(sectors,aligned);
        match change{
            "fixed-end"=>crate::gpt::put64(&mut a,1024+40,34822),
            "fixed-start"=>crate::gpt::put64(&mut a,1024+32,34815),
            "fixed-name"=>a[1024+56]=b'X',
            "grow-start"=>crate::gpt::put64(&mut a,1024+128+32,34825),
            "grow-name"=>a[1024+128+56]=b'X',
            "count"=>a[1024+128..1024+256].fill(0),_=>unreachable!(),
        }
        let b=reseal_table(&mut a,sectors);assert_eq!(crate::gpt::check(&a,&b,sectors)["healthy"],true);set_headers(&mut io,&a,&b);
        let failure=readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await.unwrap_err();assert_eq!(failure.code,"WRITE_VERIFY");
        assert!(failure.detail.contains(if change=="count"{"partition count changed"}else{"partition layout changed"}));
    }
    let(a,b)=grow_table(sectors,aligned);set_headers(&mut io,&a,&b);let mut wrong=image.clone();wrong.partitions[0].sectors=None;
    assert_eq!(readback(&mut io,&identity,&wrong,&mut file,&checks,&mut |_|{}).await.unwrap_err().code,"WRITE_VERIFY");
}
fn sparse_grow_bytes(total:u32)->Vec<u8>{
    let mut b=vec![0;28];b[..4].copy_from_slice(&[0x3a,0xff,0x26,0xed]);b[4..6].copy_from_slice(&1u16.to_le_bytes());b[8..10].copy_from_slice(&28u16.to_le_bytes());b[10..12].copy_from_slice(&12u16.to_le_bytes());
    crate::gpt::put32(&mut b,12,512);crate::gpt::put32(&mut b,16,total);crate::gpt::put32(&mut b,20,2);
    for(kind,count,data)in[(0xcac1u16,1,vec![0xa5;512]),(0xcac3,total-1,vec![])]{
        let mut h=vec![0;12];h[..2].copy_from_slice(&kind.to_le_bytes());crate::gpt::put32(&mut h,4,count);crate::gpt::put32(&mut h,8,12+data.len() as u32);b.extend(h);b.extend(data);
    }b
}
#[tokio::test] async fn grow_actual_capacity_checks_raw_and_full_sparse_extent(){
    let s=Sandbox::new();let path=s.0.join("payload");let sectors=36864;let aligned=((sectors as u64-33)&!63)-1;let capacity=(aligned-34824+1) as u32;
    let mut io=Fake{disk:disk(sectors),restricted:false,changed:false,short:false};let identity=io.identity().await.unwrap();let(a,b)=grow_table(sectors,aligned);set_headers(&mut io,&a,&b);
    for sparse in [false,true]{for count in [capacity,capacity+1]{
        let payload=if sparse{sparse_grow_bytes(count)}else{vec![0xa5;count as usize*512]};let mut bytes=vec![0xa5;4096];bytes.extend(&payload);fs::write(&path,&bytes).unwrap();
        let mut image=grow_image(path.clone());image.bytes=bytes.len() as u64;image.entries.push(Entry{name:"userdata".into(),offset:4096,bytes:payload.len() as u64,sparse});
        let mut file=locked(&path).unwrap();let checks=verification(&image,&mut file,sectors).unwrap();let result=readback(&mut io,&identity,&image,&mut file,&checks,&mut |_|{}).await;
        if count==capacity{result.unwrap();}else{let e=result.unwrap_err();assert_eq!(e.code,"WRITE_VERIFY");assert!(e.detail.contains("payload exceeds actual partition"));}
    }}
}
#[test] fn grow_end_uses_physical_capacity_not_shortened_gpt_limit(){
    let s=Sandbox::new();let path=s.0.join("payload");fs::write(&path,vec![0xa5;4096]).unwrap();let image=grow_image(path.clone());let mut file=locked(&path).unwrap();
    let sectors=36864;let end=(((sectors as u64-33)&!63)-1)-64;let(mut a,_)=grow_table(sectors,end);crate::gpt::put64(&mut a,512+48,end);let b=reseal_table(&mut a,sectors);
    assert_eq!(crate::gpt::check(&a,&b,sectors)["healthy"],true);
    assert!(verify_layout(&a,&b,sectors,&image,&mut file).unwrap_err().detail.contains("partition layout changed"));
}
#[test]
#[ignore="Requires K11C_TEST_FACTORY_IMAGE and K11C_TEST_FACTORY_GPT_DIR; PC files only, no USB commands"]
fn captured_manufacturer_gpt_matches_package_with_grow_alignment(){
    let path=PathBuf::from(std::env::var_os("K11C_TEST_FACTORY_IMAGE").expect("Set K11C_TEST_FACTORY_IMAGE"));
    let captures=PathBuf::from(std::env::var_os("K11C_TEST_FACTORY_GPT_DIR").expect("Set K11C_TEST_FACTORY_GPT_DIR"));let sectors=61071360;
    let s=Sandbox::new();let base=Path::new(env!("CARGO_MANIFEST_DIR")).join("..");fs::create_dir_all(s.0.join("resources/rockchip")).unwrap();
    for n in ["upgrade_tool.exe","config.ini"]{fs::copy(base.join("resources/rockchip").join(n),s.0.join("resources/rockchip").join(n)).unwrap();}
    let mut file=locked(&path).unwrap();let image=inspect(&s.0,&path,&mut file,&mut |_|{}).unwrap();validate_capacity(&image,sectors).unwrap();
    let a=fs::read(captures.join("gpt-primary.bin")).unwrap();let b=fs::read(captures.join("gpt-backup.bin")).unwrap();
    assert_eq!(image.partitions.len(),15);assert_eq!(image.partitions.last().unwrap().sectors,None);
    let end=crate::gpt::u64at(&a,1024+14*128+40);assert_eq!(sectors as u64-34-end,31);assert_eq!((end+1)%64,0);
    verify_layout(&a,&b,sectors,&image,&mut file).unwrap();
    let mut fixed=image.clone();let p=fixed.partitions.last_mut().unwrap();p.sectors=Some(sectors-33-p.lba);
    assert!(verify_layout(&a,&b,sectors,&fixed,&mut file).unwrap_err().detail.contains("partition layout changed"));
    fs::write(base.join("test-results/factory-layout-regression.json"),serde_json::to_vec_pretty(&json!({"source_sha256":image.sha256,"primary_sha256":hash_bytes(&a),"backup_sha256":hash_bytes(&b),"partitions":15,"unused_tail_sectors":31,"accepted":true,"fixed_extent_mismatch_rejected":true,"physical_usb_writes":0})).unwrap()).unwrap();
}
#[test]
#[ignore="Requires K11C_TEST_FACTORY_IMAGE; read-only vendor SFI, no USB commands"]
fn actual_manufacturer_package_reopens_and_expands_all_payload_bounds(){
    let path=PathBuf::from(std::env::var_os("K11C_TEST_FACTORY_IMAGE").expect("Set K11C_TEST_FACTORY_IMAGE"));
    let s=Sandbox::new();let base=Path::new(env!("CARGO_MANIFEST_DIR")).join("..");fs::create_dir_all(s.0.join("resources/rockchip")).unwrap();
    for n in ["upgrade_tool.exe","config.ini"]{fs::copy(base.join("resources/rockchip").join(n),s.0.join("resources/rockchip").join(n)).unwrap();}
    let report=import(&s.0,&path,&mut |_|{}).unwrap();let id=report["id"].as_str().unwrap();let (image,mut f)=open(&s.0,id,&mut |_|{}).unwrap();
    assert_eq!(image.format,"RKFW");validate_capacity(&image,61071360).unwrap();let checks=verification(&image,&mut f,61071360).unwrap();assert!(checks.len()>4);assert!(checks.iter().all(|c|c.lba as u64+c.bytes/512<=61071360-33));
    let out=base.join("test-results");fs::create_dir_all(&out).unwrap();fs::write(out.join(format!("factory-source-{id}.json")),serde_json::to_vec_pretty(&json!({"source":report,"reopened_and_hashed":true,"payload_ranges":checks.len(),"payload_bytes":checks.iter().map(|c|c.bytes).sum::<u64>(),"physical_usb_writes":0})).unwrap()).unwrap();
}
