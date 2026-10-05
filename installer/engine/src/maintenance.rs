//! Authenticated advanced maintenance via the existing HA Supervisor API.
//! Tokens remain in memory; the normal offline installation does not use this.
use crate::{fail,Result,components::{Catalog,Seed}};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use std::time::Duration;
pub const SLUG:&str="157e89e9_k11c_connectivity";
pub struct Client{url:reqwest::Url,token:String,http:reqwest::Client}
#[allow(async_fn_in_trait)]
pub trait Api{async fn call(&self,method:&str,path:&str,body:Value)->Result<Value>;}
impl Client{
 pub fn new(address:&str,token:String)->Result<Self>{
  let mut url=reqwest::Url::parse(address).map_err(|_|fail("HA_ADDRESS","Enter the Home Assistant URL"))?;
  if !["http","https"].contains(&url.scheme())||!url.username().is_empty()||url.password().is_some()||url.query().is_some()||url.fragment().is_some()||!matches!(url.path(),""|"/")||token.trim().len()<20||token.contains(['\r','\n']){return Err(fail("HA_ADDRESS","Invalid Home Assistant address or token"));}
  url.set_path("/api/hassio/");
  let http=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(900)).build().map_err(|_|fail("HA_NETWORK","Could not initialize connection"))?;
  Ok(Self{url,token,http})
 }
}
impl Api for Client{
 async fn call(&self,method:&str,path:&str,body:Value)->Result<Value>{
  // Paths originate only in this module, never from IPC. Never send the token to redirects.
  if path.starts_with('/')||path.contains(".."){return Err(fail("HA_ADDRESS","Invalid API path"));}
  let url=self.url.join(path).map_err(|_|fail("HA_ADDRESS","Invalid API path"))?;
  let request=match method{"GET"=>self.http.get(url),"POST"=>self.http.post(url).json(&body),_=>return Err(fail("COMMAND_NOT_ALLOWED","Invalid method"))};
  let mut response=request.bearer_auth(&self.token).send().await.map_err(|_|fail("HA_NETWORK","Home Assistant did not respond; inspect app state before retrying"))?;
  if !response.status().is_success(){return Err(fail(if matches!(response.status().as_u16(),401|403){"HA_AUTH"}else{"HA_API"},format!("{method} {path}: HTTP {}",response.status())));}
  let mut bytes=vec![];
  while let Some(chunk)=response.chunk().await.map_err(|_|fail("HA_NETWORK","Incomplete API response"))?{if bytes.len()+chunk.len()>2*1024*1024{return Err(fail("HA_API","Response too large"));}bytes.extend_from_slice(&chunk);}
  let v:Value=serde_json::from_slice(&bytes).map_err(|_|fail("HA_API","Invalid API response"))?;
  if v["result"]!="ok"{return Err(fail("HA_API",format!("{method} {path}: Supervisor rejected operation")));}Ok(v["data"].clone())
 }
}
#[derive(Clone,Serialize,Deserialize,Debug,PartialEq)]
pub struct Target{pub haos:String,pub kernel:String,pub hostname:String,pub installed:bool,pub version:Option<String>}
#[derive(Clone,Serialize,Deserialize,Debug)]
pub struct Plan{pub target:Target,pub operation:String,pub candidate:Option<Seed>}
fn str_value(v:&Value,key:&str)->Result<String>{v[key].as_str().map(str::to_owned).ok_or_else(||fail("HA_API",format!("Missing {key}")))}
async fn target(api:&impl Api)->Result<Target>{
 let os=api.call("GET","os/info",Value::Null).await?;
 let host=api.call("GET","host/info",Value::Null).await?;
 let apps=api.call("GET","addons",Value::Null).await?;
 let list=apps["addons"].as_array().or_else(||apps["apps"].as_array()).ok_or_else(||fail("HA_API","Missing app list"))?;
 let app=list.iter().find(|a|a["slug"]==SLUG);
 if os["board"]!="generic-aarch64"{return Err(fail("HA_TARGET","Expected K11C generic-aarch64 HAOS"));}
 Ok(Target{haos:str_value(&os,"version")?,kernel:str_value(&host,"kernel")?,hostname:str_value(&host,"hostname")?,installed:app.is_some(),version:app.and_then(|a|a["version"].as_str()).map(str::to_owned)})
}
async fn store_candidate(api:&impl Api,s:&Seed)->Result<()>{
 let repo=api.call("GET","store/repositories/157e89e9",Value::Null).await?;
 if repo["source"]!=crate::components::REPOSITORY{return Err(fail("HA_CATALOG_CHANGED","Repository identity changed"));}
 let info=api.call("GET",&format!("store/addons/{SLUG}"),Value::Null).await?;
 if info["version_latest"]!=s.app_version||info["repository"]!="157e89e9"||info["slug"]!=SLUG||!info["arch"].as_array().is_some_and(|a|a.iter().any(|v|v=="aarch64")){return Err(fail("HA_CATALOG_CHANGED","Store app differs from the compatible release; refresh before reinstalling"));}Ok(())
}
pub async fn plan(api:&impl Api,catalog:Option<&Catalog>,operation:&str)->Result<Plan>{
 if !["remove","reinstall"].contains(&operation){return Err(fail("COMMAND_NOT_ALLOWED","Select remove or reinstall"));}
 let t=target(api).await?;
 let candidate=if operation=="reinstall"{
  let catalog=catalog.ok_or_else(||fail("COMPONENT_UNAVAILABLE","Compatible release catalog required for reinstall"))?;
  catalog.validate()?;
  let s=catalog.seeds.iter().find(|s|s.haos==t.haos&&s.kernel==t.kernel).ok_or_else(||fail("COMPONENT_INCOMPATIBLE","No matching HAOS/kernel release"))?.clone();
  store_candidate(api,&s).await?;Some(s)
 }else{None};
 Ok(Plan{target:t,operation:operation.into(),candidate})
}
pub async fn execute(api:&impl Api,p:&Plan,confirmed:bool)->Result<Value>{
 if !confirmed{return Err(fail("CONFIRM_WRITE","Confirm K11C and the app data removal"));}
 if !["remove","reinstall"].contains(&p.operation.as_str()){return Err(fail("COMMAND_NOT_ALLOWED","Unknown maintenance operation"));}
 if target(api).await?!=p.target{return Err(fail("HA_TARGET","HAOS or app changed after confirmation; refresh first"));}
 if let Some(s)=&p.candidate{store_candidate(api,s).await?;}
 else if p.operation=="reinstall"{return Err(fail("COMPONENT_INCOMPATIBLE","Candidate missing"));}
 let backup=if p.target.installed{
  let b=api.call("POST","backups/new/partial",json!({"name":"K11C Connectivity before maintenance","addons":[SLUG],"folders":[],"homeassistant":false,"background":false})).await?;
  let id=str_value(&b,"slug")?;
  if id.is_empty()||!id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'){return Err(fail("HA_BACKUP","Invalid backup identifier"));}
  let info=api.call("GET",&format!("backups/{id}/info"),Value::Null).await?;
  let list=info["addons"].as_array().or_else(||info["apps"].as_array()).ok_or_else(||fail("HA_BACKUP","Backup contains no app information"))?;
  if !list.iter().any(|a|a["slug"]==SLUG){return Err(fail("HA_BACKUP","Connectivity missing from backup"));}
  Some(id)
 }else{None};
 let work=async{
  if p.target.installed{api.call("POST",&format!("addons/{SLUG}/uninstall"),json!({"remove_config":true})).await?;}
  if p.operation=="reinstall"{
   let s=p.candidate.as_ref().unwrap();
   store_candidate(api,s).await?;
   api.call("POST",&format!("store/addons/{SLUG}/install"),json!({"background":false})).await?;
   let fresh=api.call("GET",&format!("addons/{SLUG}/info"),Value::Null).await?;
   if fresh["version"]!=s.app_version||fresh["repository"]!="157e89e9"{return Err(fail("HA_CATALOG_CHANGED","Installed version changed; app left stopped"));}
   let mut defaults=fresh["options"].clone();
   if !defaults.is_object(){return Err(fail("HA_API","Missing fresh default options"));}
   defaults["enabled"]=json!(true);
   api.call("POST",&format!("addons/{SLUG}/options"),json!({"options":defaults,"boot":"auto"})).await?;
   api.call("POST",&format!("addons/{SLUG}/security"),json!({"protected":false})).await?;
   api.call("POST",&format!("addons/{SLUG}/start"),json!({})).await?;
   let after=api.call("GET",&format!("addons/{SLUG}/info"),Value::Null).await?;
   if after["state"]!="started"||after["boot"]!="auto"||after["options"]["enabled"]!=true||after["protected"]!=false{return Err(fail("HA_VERIFY","App has not reached the requested state"));}
  }else if target(api).await?.installed{return Err(fail("HA_VERIFY","App still installed"));}
  Ok(json!({"operation":p.operation,"backup":backup,"reboot_recommended":true}))
 }.await;
 work.map_err(|e:crate::Failure|fail(&e.code,format!("{}; recovery backup={}",e.detail,backup.unwrap_or_else(||"none (not previously installed)".into()))))
}

#[cfg(test)]mod tests{
 use super::*;use std::sync::Mutex;
 struct Fake{log:Mutex<Vec<String>>,installed:Mutex<bool>,bad_backup:bool,wrong_kernel:bool,ready:bool}
 impl Api for Fake{async fn call(&self,method:&str,path:&str,body:Value)->Result<Value>{
  self.log.lock().unwrap().push(format!("{method} {path} {body}"));
  Ok(match path{
   "os/info"=>json!({"board":"generic-aarch64","version":"18.3"}),"host/info"=>json!({"kernel":if self.wrong_kernel{"6.19-haos"}else{"6.18.52-haos"},"hostname":"k11c"}),
   "addons"=>json!({"addons":if *self.installed.lock().unwrap(){vec![json!({"slug":SLUG,"version":"0.5.6"})]}else{vec![]}}),
   "backups/new/partial"=>{assert_eq!(body["background"],false);json!({"slug":"abc123"})},
   "backups/abc123/info"=>json!({"addons":if self.bad_backup{vec![]}else{vec![json!({"slug":SLUG})]}}),
   x if x.ends_with("/uninstall")=>{assert_eq!(body["remove_config"],true);*self.installed.lock().unwrap()=false;json!({})},
   x if x.ends_with("/install")=>{*self.installed.lock().unwrap()=true;json!({})},
   "store/repositories/157e89e9"=>json!({"source":crate::components::REPOSITORY}),
   x if x==format!("store/addons/{SLUG}")=>json!({"version_latest":"0.5.6","slug":SLUG,"repository":"157e89e9","arch":["aarch64"]}),
   x if x.ends_with("/info")=>json!({"version":"0.5.6","repository":"157e89e9","state":if self.ready{"started"}else{"stopped"},"boot":"auto","protected":false,"options":{"enabled":true,"npu_enabled":false}}),
   _=>json!({})})
 }}
 fn fake()->Fake{Fake{log:Mutex::new(vec![]),installed:Mutex::new(true),bad_backup:false,wrong_kernel:false,ready:true}}
 #[tokio::test]async fn reinstall_uses_backup_clean_remove_defaults_then_autostart(){
  let f=fake();let c=crate::components::tests::sample(&"1".repeat(64),&"2".repeat(64),512);let p=plan(&f,Some(&c),"reinstall").await.unwrap();assert!(f.log.lock().unwrap().iter().all(|s|s.starts_with("GET")));
  execute(&f,&p,true).await.unwrap();let l=f.log.lock().unwrap();let a=l.iter().position(|s|s.contains("backups/abc123/info")).unwrap();let b=l.iter().position(|s|s.contains("/uninstall")).unwrap();let d=l.iter().position(|s|s.contains("/start")).unwrap();assert!(a<b&&b<d);assert!(!l.iter().any(|s|s.contains("/restore")));
 }
 #[tokio::test]async fn incompatible_or_unconfirmed_or_failed_backup_never_uninstalls(){
  let c=crate::components::tests::sample(&"1".repeat(64),&"2".repeat(64),512);
  let mut f=fake();f.wrong_kernel=true;assert!(plan(&f,Some(&c),"reinstall").await.is_err());assert!(f.log.lock().unwrap().iter().all(|s|s.starts_with("GET")));
  for bad in [false,true]{let mut f=fake();f.bad_backup=bad;let p=plan(&f,Some(&c),"reinstall").await.unwrap();assert!(execute(&f,&p,bad).await.is_err());assert!(!f.log.lock().unwrap().iter().any(|s|s.contains("/uninstall")));}
 }
 #[tokio::test]async fn stopped_app_not_reported_success(){let mut f=fake();f.ready=false;let c=crate::components::tests::sample(&"1".repeat(64),&"2".repeat(64),512);let p=plan(&f,Some(&c),"reinstall").await.unwrap();assert_eq!(execute(&f,&p,true).await.unwrap_err().code,"HA_VERIFY");}
 #[test]fn credentials_never_redirect_or_use_url_userinfo(){assert!(Client::new("http://admin:secret@192.168.1.2", "x".repeat(30)).is_err());assert!(Client::new("http://192.168.1.2/?token=x", "x".repeat(30)).is_err());}
 #[tokio::test]async fn removal_needs_no_release_catalog_and_backup_failure_blocks_it(){
  let f=fake();let p=plan(&f,None,"remove").await.unwrap();execute(&f,&p,true).await.unwrap();assert!(!*f.installed.lock().unwrap());
  assert!(!f.log.lock().unwrap().iter().any(|s|s.contains("store/")||s.contains("/install ")));
  let mut f=fake();f.bad_backup=true;let p=plan(&f,None,"remove").await.unwrap();assert!(execute(&f,&p,true).await.is_err());assert!(*f.installed.lock().unwrap());
 }
 #[tokio::test]async fn defaults_and_boot_are_actually_sent(){
  let f=fake();let c=crate::components::tests::sample(&"1".repeat(64),&"2".repeat(64),512);let p=plan(&f,Some(&c),"reinstall").await.unwrap();execute(&f,&p,true).await.unwrap();
  let l=f.log.lock().unwrap();let options=l.iter().find(|s|s.contains("/options ")).unwrap();assert!(options.contains("\"boot\":\"auto\""));assert!(options.contains("\"enabled\":true"));assert!(l.iter().any(|s|s.contains("/security ")&&s.contains("\"protected\":false")));
 }
}
