use super::*;
use std::{collections::HashMap,io::Cursor,sync::atomic::AtomicBool};
struct Net(HashMap<String,Vec<u8>>);
impl Transport for Net{
 fn get(&self,url:&str)->Result<images::Response<'_>>{
  let b=self.0.get(url).ok_or_else(||fail("IMAGE_NETWORK","offline"))?;
  Ok(images::Response{length:Some(b.len() as u64),body:Box::new(Cursor::new(b))})
 }
}
pub(crate) fn sample(raw:&str,fw_hash:&str,fw_len:u64)->Catalog{
 let url="https://github.com/David2766/K11C-HAOS/releases/download/factory-test/";
 let state=json!({"system":{"157e89e9_k11c_connectivity":{"version":"0.5.6","image":"ghcr.io/david2766/k11c-haos-connectivity","slug":"k11c_connectivity","repository":"157e89e9","startup":"system","arch":["aarch64"]}},"user":{"157e89e9_k11c_connectivity":{"version":"0.5.6","image":"ghcr.io/david2766/k11c-haos-connectivity","options":{"enabled":true},"boot":"auto","protected":false}}});
 Catalog{schema:2,board:"kickpi,k11c".into(),arch:"aarch64".into(),boot:Boot{revision:"r24".into(),haos:vec!["18.3".into()],asset:Asset{url:format!("{url}uboot.bin"),sha256:fw_hash.into(),bytes:fw_len}},seeds:vec![Seed{
 haos:"18.3".into(),kernel:"6.18.52-haos".into(),official_raw_sha256:raw.into(),app_version:"0.5.6".into(),image_digest:format!("ghcr.io/david2766/k11c-haos-connectivity@sha256:{}","2".repeat(64)),data_sha256:"3".repeat(64),data_bytes:8192,image_id:format!("sha256:{}","3".repeat(64)),catalog_commit:"4".repeat(40),config_sha256:"5".repeat(64),state_sha256:crate::hash_bytes(&serde_json::to_vec(&state).unwrap()),state,containerd:"2.3.4".into(),locally_verified:true,supervisor:"2026.09.2".into(),docker:"29.7.2".into(),uboot_sha256:fw_hash.into()}]}
}
pub(crate) fn register_fixture(base:&Path,id:&str){
 let fw=fs::read(base.join("resources/firmware").join(crate::flash::FIRMWARE)).unwrap();
 let hash=crate::hash_bytes(&fw);let c=sample(id,&hash,fw.len() as u64);
 fs::create_dir_all(base.join("data/components/assets")).unwrap();fs::write(cache_path(base,&hash),fw).unwrap();
 save_json(&base.join("data/components/boots").join(format!("{hash}.json")),&json!(c.boot)).unwrap();
 save_json(&base.join("data/components/installations").join(format!("{id}.json")),&json!(Selection{schema:2,image:id.into(),seed:c.seeds[0].clone(),boot:c.boot})).unwrap();
}
#[test]fn exact_compatibility_no_newer_version_guess(){
 let c=sample(&"1".repeat(64),&"2".repeat(64),512);assert!(c.select(&"1".repeat(64)).is_ok());assert!(c.select(&"9".repeat(64)).is_err());
 for i in 0..10{let mut x=c.clone();match i{0=>x.arch="x86_64".into(),1=>x.seeds[0].uboot_sha256="3".repeat(64),2=>x.boot.haos.clear(),3=>x.seeds[0].image_digest="bad".into(),4=>x.boot.asset.url="https://evil.example/payload".into(),5=>x.seeds.push(x.seeds[0].clone()),6=>x.seeds[0].locally_verified=false,7=>x.seeds[0].state["user"]["157e89e9_k11c_connectivity"]["options"]["enabled"]=json!(false),8=>x.seeds[0].containerd="1.7".into(),_=>x.schema=1};assert!(x.validate().is_err(),"{i}");}
}
struct Build(Vec<u8>);
impl crate::native_seed::Builder for Build{fn build(&self,_:&Path,_:&Path,_:&Seed,path:&Path,_:&mut Job<'_>)->Result<crate::native_seed::Receipt>{fs::write(path,&self.0).unwrap();Ok(crate::native_seed::Receipt{bytes:self.0.len() as u64,sha256:crate::hash_bytes(&self.0),seconds:1.0,existing_images:8})}}
#[test]fn compose_preserves_os_and_identity_and_rejects_tampering(){
 let base=std::env::temp_dir().join(format!("k11c-component-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&base).unwrap();
 let cancel=AtomicBool::new(false);let mut noop=|_|{};let mut job=Job::new(&cancel,&mut noop);
 let fixture=crate::images::tests::fixture();let original=base.join("official.img");fs::write(&original,fixture).unwrap();
 let raw=images::import(&original,&base,&mut job).unwrap();let mut c=sample(&raw.sha256,&crate::hash_bytes(&vec![7u8;512]),512);
 let mut data=vec![0u8;4*1024*1024];data[1080..1082].copy_from_slice(&[0x53,0xef]);data[1144..1155].copy_from_slice(b"hassos-data");c.seeds[0].data_sha256=crate::hash_bytes(&data);c.seeds[0].data_bytes=data.len() as u64;
 let net=Net(HashMap::from([(CATALOG_URL.into(),serde_json::to_vec(&c).unwrap()),(c.boot.asset.url.clone(),vec![7;512])]));
 let prepared=prepare_with(&net,&base,raw.clone(),&mut job,&Build(data.clone())).unwrap();let id=prepared["sha256"].as_str().unwrap();let s=selection(&base,id).unwrap();assert_eq!(s.seed.haos,"18.3");assert_eq!(boot_bytes(&base,&s.boot.asset.sha256).unwrap(),vec![7;512]);
 let result=fs::read(prepared["path"].as_str().unwrap()).unwrap();let start=gpt::u64at(fixture,1024+7*128+32) as usize*512;
 assert_eq!(&result[34*512..start],&fixture[34*512..start],"all OS partitions and gaps");
 assert_eq!(&result[1024..1024+7*128],&fixture[1024..1024+7*128],"first seven GPT entries");
 assert_eq!(&result[1024+7*128..1024+7*128+40],&fixture[1024+7*128..1024+7*128+40]);
 assert_eq!(&result[start..start+data.len()],&data);assert_eq!(prepared["connectivity"],"0.5.6");
 fs::write(cache_path(&base,&s.boot.asset.sha256),vec![8;512]).unwrap();assert!(boot_bytes(&base,&s.boot.asset.sha256).is_err());
 let mut tampered=fs::read(&raw.path).unwrap();tampered[35000]^=1;fs::write(&raw.path,tampered).unwrap();assert!(prepare_with(&net,&base,raw,&mut job,&Build(data)).is_err());
 fs::remove_dir_all(base).unwrap();
}
#[test]fn cancelled_and_corrupt_download_never_published(){
 let base=std::env::temp_dir().join(format!("k11c-download-{}",std::process::id()));let a=sample(&"1".repeat(64),&"2".repeat(64),512).boot.asset;
 let n=Net(HashMap::from([(a.url.clone(),vec![0;512])]));let cancel=AtomicBool::new(false);let mut noop=|_|{};let mut job=Job::new(&cancel,&mut noop);
 assert!(acquire(&n,&base,&a,&mut job).is_err());assert!(!cache_path(&base,&a.sha256).exists());cancel.store(true,std::sync::atomic::Ordering::Relaxed);assert!(acquire(&n,&base,&a,&mut job).is_err());
 if base.exists(){fs::remove_dir_all(base).unwrap();}
}

#[test]#[ignore="Requires cached official HAOS and the Windows-native build inputs"]
fn actual_windows_native_production_path(){
 let root=PathBuf::from(std::env::var_os("K11C_NATIVE_TEST_BASE").expect("K11C_NATIVE_TEST_BASE"));
 let original=PathBuf::from(std::env::var_os("K11C_OFFICIAL_IMAGE").expect("K11C_OFFICIAL_IMAGE"));
 let c:Catalog=serde_json::from_slice(&fs::read(root.join("catalog.json")).unwrap()).unwrap();c.validate().unwrap();
 let cancel=AtomicBool::new(false);let mut notify=|p:images::Progress|eprintln!("{}",serde_json::to_string(&p).unwrap());let mut job=Job::new(&cancel,&mut notify);
 let before=images::file_hash(&original,&mut job,"source-before").unwrap();
 let raw=images::import(&original,&root,&mut job).unwrap();
 let net=Net(HashMap::from([(CATALOG_URL.into(),serde_json::to_vec(&c).unwrap())]));
 let began=std::time::Instant::now();let prepared=prepare(&net,&root,raw.clone(),&mut job).unwrap();
 let original_bytes=fs::read(&original).unwrap();let result=fs::read(prepared["path"].as_str().unwrap()).unwrap();
 let start=gpt::u64at(&original_bytes,1024+7*128+32) as usize*512;
 assert_eq!(&result[34*512..start],&original_bytes[34*512..start],"all stock OS payloads byte-identical");
 assert_eq!(&result[1024..1024+7*128],&original_bytes[1024..1024+7*128],"seven GPT entries unchanged");
 assert_eq!(images::file_hash(&original,&mut job,"source-after").unwrap(),before);
 assert!(selection(&root,prepared["sha256"].as_str().unwrap()).is_ok());
 save_json(&root.join("native-production-result.json"),&json!({"passed":true,"elapsed_s":began.elapsed().as_secs_f64(),"preparation":prepared,"source_unchanged":true,"os_partitions_unchanged":7,"emulation_used":false,"physical_board_tested":false})).unwrap();
}
