//! Release-resolved firmware and factory-data composition. No USB/network writes.
use crate::{fail, Result, images::{self,Transport,Job,Temporary,PreparedImage},gpt};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use std::{fs::{self,File},io::{Read,Write,Seek,SeekFrom},path::{Path,PathBuf}};

pub const CATALOG_URL:&str="https://raw.githubusercontent.com/David2766/K11C-HAOS/main/installer-catalog.json";
pub const REPOSITORY:&str="https://github.com/David2766/K11C-HAOS";
const LIMIT:u64=16*1024*1024*1024;
fn io(e:impl std::fmt::Display)->crate::Failure{fail("COMPONENT_IO",e)}
pub fn hash_id(s:&str)->bool{s.len()==64&&s.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Asset{pub url:String,pub sha256:String,pub bytes:u64}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Boot{pub revision:String,pub haos:Vec<String>,pub asset:Asset}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Seed{
 pub haos:String,pub kernel:String,pub official_raw_sha256:String,pub app_version:String,
 pub image_digest:String,pub image_id:String,pub catalog_commit:String,pub config_sha256:String,
 pub state:Value,pub state_sha256:String,pub containerd:String,pub locally_verified:bool,
 #[serde(default)]pub data_sha256:String,#[serde(default)]pub data_bytes:u64,
 pub supervisor:String,pub docker:String,pub uboot_sha256:String,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Catalog{pub schema:u32,pub board:String,pub arch:String,pub boot:Boot,pub seeds:Vec<Seed>}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Selection{pub schema:u32,pub image:String,pub seed:Seed,pub boot:Boot}
fn asset(a:&Asset)->Result<()>{
 let url=reqwest::Url::parse(&a.url).map_err(io)?;
 let allowed=url.host_str()==Some("github.com")&&url.path().starts_with("/David2766/K11C-HAOS/releases/download/")||url.host_str()==Some("raw.githubusercontent.com")&&url.path().starts_with("/David2766/K11C-HAOS/");
 if !hash_id(&a.sha256)||a.bytes==0||a.bytes>LIMIT||url.scheme()!="https"||!allowed||!url.username().is_empty()||url.password().is_some()||url.port().is_some()||url.query().is_some()||url.fragment().is_some()||url.path().contains("..") {
  return Err(fail("COMPONENT_CATALOG","Invalid component asset"));
 }Ok(())
}
impl Catalog{
 pub fn validate(&self)->Result<()>{
  if self.schema!=2||self.board!="kickpi,k11c"||self.arch!="aarch64"{return Err(fail("COMPONENT_CATALOG","Wrong board, architecture or catalog schema"));}
  validate_boot(&self.boot)?;
  let mut seen=std::collections::HashSet::new();
  for s in &self.seeds{
   let slug="157e89e9_k11c_connectivity";let state=&s.state;let user=&state["user"][slug];let system=&state["system"][slug];
   if !s.locally_verified||s.containerd!="2.3.4"||!hash_id(&s.config_sha256)||s.catalog_commit.len()!=40||!s.catalog_commit.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))||!s.image_id.starts_with("sha256:")||!hash_id(s.image_id.trim_start_matches("sha256:"))||crate::hash_bytes(&serde_json::to_vec(state).map_err(io)?)!=s.state_sha256||state["system"].as_object().map(|m|m.len())!=Some(1)||state["user"].as_object().map(|m|m.len())!=Some(1)||system["version"]!=s.app_version||system["slug"]!="k11c_connectivity"||system["repository"]!="157e89e9"||system["startup"]!="system"||system["arch"]!=json!(["aarch64"])||system["image"]!="ghcr.io/david2766/k11c-haos-connectivity"||user!=&json!({"version":s.app_version,"image":"ghcr.io/david2766/k11c-haos-connectivity","options":{"enabled":true},"boot":"auto","protected":false})||!images::valid_version(&s.haos)||!images::valid_version(&s.app_version)||!hash_id(&s.official_raw_sha256)||!s.kernel.ends_with("-haos")||!self.boot.haos.contains(&s.haos)||s.uboot_sha256!=self.boot.asset.sha256||!seen.insert(&s.haos)||!s.image_digest.starts_with("ghcr.io/david2766/k11c-haos-connectivity@sha256:")||!hash_id(s.image_digest.rsplit(':').next().unwrap_or(""))||s.supervisor.is_empty()||s.docker.is_empty(){
    return Err(fail("COMPONENT_CATALOG","Incomplete or contradictory compatibility profile"));
   }
  }Ok(())
 }
 pub fn select(&self,raw:&str)->Result<&Seed>{
  self.validate()?;
  self.seeds.iter().find(|s|s.official_raw_sha256==raw).ok_or_else(||fail("COMPONENT_INCOMPATIBLE","No tested factory package for this exact official HAOS image. Select a supported version."))
 }
}
fn validate_boot(b:&Boot)->Result<()>{
 asset(&b.asset)?;
 if b.revision.is_empty()||b.asset.bytes%512!=0||b.asset.bytes>(34816-64)*512||b.haos.is_empty(){return Err(fail("COMPONENT_CATALOG","Invalid K11C firmware layout"));}Ok(())
}
pub fn catalog(net:&impl Transport,base:&Path)->Result<Value>{
 let cache=base.join("data/components/catalog.json");
 let remote=images::json_from(net,CATALOG_URL);
 let (v,source)=match remote {
  Ok(v)=>{let c:Catalog=serde_json::from_value(v.clone()).map_err(io)?;c.validate()?;save_json(&cache,&v)?;(v,"github")},
  Err(e)=>{
   if !["IMAGE_NETWORK","IMAGE_RATE_LIMIT"].contains(&e.code.as_str()){return Err(e);}
   let p=if cache.exists(){cache}else{base.join("resources/components/catalog.json")};
   let v=read_json(&p).map_err(|_|fail("COMPONENT_UNAVAILABLE","No release catalog or verified offline catalog available"))?;
   (v,"cached")
  }
 };
 let c:Catalog=serde_json::from_value(v).map_err(io)?;c.validate()?;
 Ok(json!({"catalog":c,"source":source}))
}
fn read_json(p:&Path)->Result<Value>{let f=File::open(p).map_err(io)?;if f.metadata().map_err(io)?.len()>1024*1024{return Err(fail("COMPONENT_CATALOG","Metadata too large"));}serde_json::from_reader(f).map_err(io)}
fn save_json(p:&Path,v:&Value)->Result<()>{
 fs::create_dir_all(p.parent().unwrap()).map_err(io)?;
 let (temp,mut out)=Temporary::new(p.parent().unwrap(),"json")?;
 out.write_all(&serde_json::to_vec(v).map_err(io)?).and_then(|_|out.sync_all()).map_err(io)?;drop(out);
 // File replacement is limited to this app-owned metadata path.
 if p.exists(){fs::remove_file(p).map_err(io)?;}fs::rename(&temp.path,p).map_err(io)
}
fn cache_path(base:&Path,hash:&str)->PathBuf{base.join("data/components/assets").join(hash)}
pub fn acquire(net:&impl Transport,base:&Path,a:&Asset,job:&mut Job<'_>)->Result<PathBuf>{
 asset(a)?;let target=cache_path(base,&a.sha256);
 for p in [&target,&base.join("resources/components/assets").join(&a.sha256)]{
  if p.is_file()&&images::file_hash(p,job,"verify-component")?==(a.bytes,a.sha256.clone()){return Ok(p.clone());}
 }
 let (temp,mut file)=Temporary::new(target.parent().unwrap(),"download")?;let mut r=net.get(&a.url)?;
 if r.length.is_some_and(|n|n!=a.bytes){return Err(fail("COMPONENT_HASH","Component size mismatch"));}
 let (n,h)=images::stream_copy(&mut r.body,&mut file,LIMIT,Some(a.bytes),job,"download-component")?;drop(file);
 if h!=a.sha256{return Err(fail("COMPONENT_HASH","Component SHA-256 mismatch"));}
 images::publish(&temp,&target,n,&h,job)?;Ok(target)
}
pub fn prepare_boot(net:&impl Transport,base:&Path,job:&mut Job<'_>)->Result<Value>{
 let report=catalog(net,base)?;let c:Catalog=serde_json::from_value(report["catalog"].clone()).map_err(io)?;
 let p=acquire(net,base,&c.boot.asset,job)?;
 // Cache a bundled fallback too, so the selection has one immutable asset path.
 cache_copy(base,&c.boot.asset,&p,job)?;
 save_json(&base.join("data/components/boots").join(format!("{}.json",c.boot.asset.sha256)),&json!(c.boot))?;
 Ok(json!({"sha256":c.boot.asset.sha256,"revision":c.boot.revision,"source":report["source"]}))
}
fn cache_copy(base:&Path,a:&Asset,p:&Path,job:&mut Job<'_>)->Result<()> {
 let target=cache_path(base,&a.sha256);if p==target{return Ok(());}
 let (temp,mut f)=Temporary::new(target.parent().unwrap(),"asset")?;
 let (n,h)=images::stream_copy(&mut File::open(p).map_err(io)?,&mut f,LIMIT,Some(a.bytes),job,"cache-component")?;drop(f);
 if h!=a.sha256{return Err(fail("COMPONENT_HASH","Asset changed"));}images::publish(&temp,&target,n,&h,job)?;Ok(())
}
pub fn boot_bytes(base:&Path,id:&str)->Result<Vec<u8>>{
 if !hash_id(id){return Err(fail("COMPONENT_SELECTION","Select a verified U-Boot release"));}
 let b:Boot=serde_json::from_value(read_json(&base.join("data/components/boots").join(format!("{id}.json")))?).map_err(io)?;
 validate_boot(&b)?;if b.asset.sha256!=id{return Err(fail("COMPONENT_HASH","Boot selection changed"));}
 let file=File::open(cache_path(base,id)).map_err(io)?;
 let mut data=vec![];file.take(b.asset.bytes+1).read_to_end(&mut data).map_err(io)?;
 if data.len() as u64!=b.asset.bytes||crate::hash_bytes(&data)!=id{return Err(fail("COMPONENT_HASH","U-Boot changed"));}Ok(data)
}
pub fn selection(base:&Path,id:&str)->Result<Selection>{
 if !hash_id(id){return Err(fail("COMPONENT_SELECTION","Invalid installation image"));}
 let s:Selection=serde_json::from_value(read_json(&base.join("data/components/installations").join(format!("{id}.json")))?).map_err(|_|fail("COMPONENT_SELECTION","Prepare HAOS with Connectivity before installation"))?;
 let c=Catalog{schema:2,board:"kickpi,k11c".into(),arch:"aarch64".into(),boot:s.boot.clone(),seeds:vec![s.seed.clone()]};c.validate()?;
 if s.schema!=2||s.image!=id||!hash_id(&s.seed.data_sha256)||s.seed.data_bytes<2048||s.seed.data_bytes%512!=0||s.seed.data_bytes>LIMIT{return Err(fail("COMPONENT_SELECTION","Installation selection changed"));}Ok(s)
}
/// Compose offline. The original official image and seven OS partitions are immutable.
pub fn prepare(net:&impl Transport,base:&Path,raw:PreparedImage,job:&mut Job<'_>)->Result<Value>{
 prepare_with(net,base,raw,job,&crate::native_seed::Native)
}
fn prepare_with(net:&impl Transport,base:&Path,raw:PreparedImage,job:&mut Job<'_>,builder:&impl crate::native_seed::Builder)->Result<Value>{
 let report=catalog(net,base)?;let c:Catalog=serde_json::from_value(report["catalog"].clone()).map_err(io)?;
 let mut s=c.select(&raw.sha256)?.clone();
 // Hold a read-only sharing lock across preparation and composition.
 use std::os::windows::fs::OpenOptionsExt;
 let _source_lock=std::fs::OpenOptions::new().read(true).share_mode(1).open(&raw.path).map_err(io)?;
 let fw=acquire(net,base,&c.boot.asset,job)?;cache_copy(base,&c.boot.asset,&fw,job)?;
 save_json(&base.join("data/components/boots").join(format!("{}.json",c.boot.asset.sha256)),&json!(c.boot))?;
 let (data_temp,data)=Temporary::new(&base.join("data/components/work"),"ext4")?;drop(data);fs::remove_file(&data_temp.path).map_err(io)?;
 let receipt=builder.build(base,&raw.path,&s,&data_temp.path,job)?;
 // Composition checks exact length and SHA256 while copying these same bytes.
 if receipt.bytes<2048||receipt.bytes>LIMIT||receipt.bytes%512!=0||!hash_id(&receipt.sha256){return Err(fail("COMPONENT_HASH","Prepared data digest mismatch"));}
 s.data_bytes=receipt.bytes;s.data_sha256=receipt.sha256;
 let prepared=compose(base,&raw,&s,&data_temp.path,job)?;
 let record=Selection{schema:2,image:prepared.sha256.clone(),seed:s.clone(),boot:c.boot.clone()};
 save_json(&base.join("data/components/installations").join(format!("{}.json",prepared.sha256)),&json!(record))?;
 let mut v=json!(prepared);v["connectivity"]=json!(s.app_version);v["uboot"]=json!(c.boot.revision);v["catalog_source"]=report["source"].clone();v["kernel"]=json!(s.kernel);v["preparation_seconds"]=json!(receipt.seconds);Ok(v)
}
fn compose(base:&Path,raw:&PreparedImage,s:&Seed,data:&Path,job:&mut Job<'_>)->Result<PreparedImage>{
 if images::file_hash(&raw.path,job,"verify-official")?.1!=s.official_raw_sha256{return Err(fail("COMPONENT_HASH","Official image changed"));}
 images::inspect(&raw.path,job.cancel)?;
 let mut src=File::open(&raw.path).map_err(io)?;let mut head=vec![0;34*512];src.read_exact(&mut head).map_err(io)?;
 let start=gpt::u64at(&head,1024+7*128+32)*512;
 let mut datafile=File::open(data).map_err(io)?;datafile.seek(SeekFrom::Start(1024)).map_err(io)?;let mut sb=[0u8;136];datafile.read_exact(&mut sb).map_err(io)?;
 if sb[56..58]!=[0x53,0xef]||&sb[120..131]!=b"hassos-data"{return Err(fail("COMPONENT_DATA","Expected hassos-data ext4 filesystem"));}
 let sectors=(start+s.data_bytes)/512+33;
 if sectors*512>32*1024*1024*1024{return Err(fail("IMAGE_SIZE","Factory image too large"));}
 let (head,tail)=resize_data_gpt(head,sectors,s.data_bytes)?;
 let dir=base.join("data/images/prepared");let (temp,mut out)=Temporary::new(&dir,"img")?;
 out.write_all(&head).map_err(io)?;
 src.seek(SeekFrom::Start(head.len() as u64)).map_err(io)?;
 images::stream_copy(&mut src.take(start-head.len() as u64),&mut out,LIMIT,Some(start-head.len() as u64),job,"copy-official-os")?;
 datafile.seek(SeekFrom::Start(0)).map_err(io)?;
 let (_,copied)=images::stream_copy(&mut datafile,&mut out,LIMIT,Some(s.data_bytes),job,"add-connectivity")?;
 if copied!=s.data_sha256{return Err(fail("COMPONENT_HASH","Data changed during composition"));}
 out.write_all(&tail).and_then(|_|out.sync_all()).map_err(io)?;drop(out);
 images::inspect(&temp.path,job.cancel)?;
 let (bytes,hash)=images::file_hash(&temp.path,job,"verify-installation")?;let path=dir.join(format!("{hash}.img"));
 let reused=images::publish(&temp,&path,bytes,&hash,job)?;
 Ok(PreparedImage{path,filename:raw.filename.clone(),version:Some(s.haos.clone()),platform:raw.platform.clone(),bytes,sha256:hash,source_sha256:raw.sha256.clone(),verification:"official_os_factory_data".into(),partitions:8,reused_download:raw.reused_download,reused_image:reused})
}
fn resize_data_gpt(mut a:Vec<u8>,sectors:u64,data_bytes:u64)->Result<(Vec<u8>,Vec<u8>)>{
 let e=1024+7*128;let start=gpt::u64at(&a,e+32);
 if sectors>u32::MAX as u64||sectors!=start+data_bytes/512+33{return Err(fail("IMAGE_GPT","Invalid factory layout"));}
 gpt::put64(&mut a,e+40,sectors-34);gpt::put32(&mut a,458,sectors as u32-1);
 let crc=gpt::crc(&a[1024..]);let h=&mut a[512..1024];gpt::put64(h,32,sectors-1);gpt::put64(h,48,sectors-34);gpt::put32(h,88,crc);gpt::seal(h);
 let mut b=vec![0;33*512];b[..16384].copy_from_slice(&a[1024..]);b[16384..].copy_from_slice(&a[512..1024]);let h=&mut b[16384..];gpt::put64(h,24,sectors-1);gpt::put64(h,32,1);gpt::put64(h,72,sectors-33);gpt::seal(h);
 if gpt::check(&a,&b,sectors as u32)["healthy"]!=true{return Err(fail("IMAGE_GPT","Invalid composed GPT"));}Ok((a,b))
}

#[cfg(test)] pub(crate) mod tests;
