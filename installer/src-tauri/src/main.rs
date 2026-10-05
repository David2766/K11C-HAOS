#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use k11c_usb::{Result,fail,envelope,base_dir,verify_file,driver,win};
use serde_json::{Value,json};
use std::{io::{Write,Read},process::{Command,Stdio},os::windows::process::CommandExt,time::{Duration,Instant},sync::{Arc,atomic::AtomicBool}};
use tokio::sync::Mutex;
use tauri::{Emitter,Manager};

struct State { busy:Mutex<()>, attempted:Arc<AtomicBool>, image_cancel:Arc<AtomicBool>, writing:AtomicBool, maintenance:Mutex<Option<(k11c_usb::maintenance::Client,k11c_usb::maintenance::Plan)>> }
impl State {
    // A user action waits for an in-flight background device poll. Polling itself
    // uses try_lock and never queues behind a long user operation.
    async fn image_guard(&self)->Result<tokio::sync::MutexGuard<'_,()>> { Ok(self.busy.lock().await) }
}
fn helper_path()->Result<std::path::PathBuf> {
    let p=base_dir()?.join("k11c-usb.exe");
    let expected=option_env!("K11C_HELPER_SHA256").ok_or_else(||fail("BUILD_INCOMPLETE","Build with scripts/build.mjs"))?;
    verify_file(&p,expected)?;Ok(p)
}
fn run_helper(args: &[&str])->Result<Value> {
    run_helper_progress(args,None)
}
fn run_helper_progress(args:&[&str],app:Option<tauri::AppHandle>)->Result<Value>{
    let mut child=Command::new(helper_path()?).args(args).creation_flags(0x08000000)
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e|fail("HELPER_START",e))?;
    let _job=match win::contain_child(child.id()){Ok(job)=>job,Err(e)=>{let _=child.kill();let _=child.wait();return Err(e);}};
    // Drain pipes concurrently: diagnostics must never deadlock on pipe capacity.
    let out=child.stdout.take().unwrap(); let err=child.stderr.take().unwrap();
    let reader=std::thread::spawn(move|| {let mut b=Vec::new();std::io::Read::take(out,1024*1024).read_to_end(&mut b).map(|_|b)});
    let (tx,rx)=std::sync::mpsc::channel();
    let errors=std::thread::spawn(move|| {
        use std::io::BufRead;
        let mut bytes=Vec::new();
        for line in std::io::BufReader::new(err).lines() {
            let line=line?;
            if let Ok(v)=serde_json::from_str::<Value>(&line) {if v.get("phase").is_some(){let _=tx.send(v);continue;}}
            if bytes.len()<16384 {bytes.extend_from_slice(line.as_bytes());bytes.push(b'\n');}
        }
        Ok::<_,std::io::Error>(bytes)
    });
    let started=Instant::now(); let mut timed_out=false;
    let timeout=match args.first().copied(){Some("prepare")=>90,Some("backup"|"backup-catalog"|"gpt-check")=>300,Some("storage-plan")=>1200,Some("archive-backup"|"storage-plan-backed"|"storage-plan-direct-restore"|"storage-execute"|"factory-plan"|"factory-execute"|"factory-import")=>86400,_=>30};
    loop {
        for p in rx.try_iter(){if let Some(app)=&app{let _=app.emit("usb-progress",p);}}
        if child.try_wait().map_err(|e|fail("HELPER_WAIT",e))?.is_some(){break;}
        if started.elapsed()>Duration::from_secs(timeout) {let _=child.kill();let _=child.wait();timed_out=true;break;}
        std::thread::sleep(Duration::from_millis(50));
    }
    let bytes=reader.join().map_err(|_|fail("HELPER_READ","reader panicked"))?.map_err(|e|fail("HELPER_READ",e))?;
    let stderr=errors.join().ok().and_then(|r|r.ok()).unwrap_or_default();
    for p in rx.try_iter(){if let Some(app)=&app{let _=app.emit("usb-progress",p);}}
    if timed_out{return Err(fail("USB_TIMEOUT",format!("Operation timed out after {timeout} seconds. Inspect data/plans journals and backup/ (legacy: data/backups) before retrying; a write may be incomplete")));}
    let v:Value=serde_json::from_slice(&bytes).map_err(|e|fail("HELPER_RESPONSE",format!("{e}; {}",String::from_utf8_lossy(&stderr))))?;
    if v["ok"]!=true {return Err(serde_json::from_value(v["error"].clone()).unwrap_or_else(|_|fail("HELPER_RESPONSE","Invalid error")));}
    Ok(v["data"].clone())
}
fn record(operation:&str,value:&Value)->Result<()> {
    let dir=base_dir()?.join("data");std::fs::create_dir_all(&dir).map_err(|e|fail("PORTABLE_DATA",e))?;
    let p=dir.join("operations.jsonl");
    let mut f=std::fs::OpenOptions::new().append(true).create(true).open(p).map_err(|e|fail("PORTABLE_DATA",e))?;
    let epoch=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    writeln!(f,"{}",json!({"time":epoch,"operation":operation,"result":value})).map_err(|e|fail("PORTABLE_DATA",e))
}
async fn blocking<F>(op:&str,f:F)->Value where F:FnOnce()->Result<Value>+Send+'static {
    let result=tauri::async_runtime::spawn_blocking(f).await.unwrap_or_else(|e|Err(fail("WORKER",e)));
    let mut v=envelope(result);
    if let Err(e)=record(op,&v){v["history_warning"]=json!(e);}
    v
}
#[tauri::command]
async fn preflight(state:tauri::State<'_,State>)->Result<Value> {
    let _lock=state.busy.lock().await;Ok(blocking("preflight",||run_helper(&["preflight"])).await)
}
#[tauri::command]
async fn inspect_device(instance_id:String,state:tauri::State<'_,State>)->Result<Value> {
    let _lock=state.busy.lock().await;
    Ok(blocking("inspect",move||run_helper(&["inspect",&instance_id])).await)
}
#[tauri::command]
async fn list_devices(state:tauri::State<'_,State>)->Result<Value>{
    let Ok(_lock)=state.busy.try_lock() else{return Ok(envelope(Err(fail("DEVICE_BUSY","Operation in progress"))));};
    Ok(envelope(tauri::async_runtime::spawn_blocking(||run_helper(&["devices"])).await.unwrap_or_else(|e|Err(fail("WORKER",e)))))
}
#[tauri::command]
async fn prepare_device(instance_id:String,location:String,confirmed_k11c:bool,app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value>{
    if !confirmed_k11c{return Ok(envelope(Err(fail("CONFIRM_BOARD","Confirm the board model"))));}
    let _lock=state.busy.lock().await;
    Ok(blocking("prepare",move||run_helper_progress(&["prepare",&instance_id,&location,"--confirmed-k11c"],Some(app))).await)
}
#[tauri::command]
async fn backup_device(instance_id:String,kind:String,app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value>{
    let _lock=state.busy.lock().await;
    Ok(blocking("backup",move||run_helper_progress(&["archive-backup",&instance_id,&kind],Some(app))).await)
}
#[tauri::command]
async fn gpt_check(instance_id:String,state:tauri::State<'_,State>)->Result<Value>{
    let _lock=state.busy.lock().await;Ok(blocking("gpt-check",move||run_helper(&["gpt-check",&instance_id])).await)
}
#[tauri::command]
async fn backup_catalog(state:tauri::State<'_,State>)->Result<Value>{
    let _lock=state.busy.lock().await;Ok(blocking("backup-catalog",||run_helper(&["backup-catalog"])).await)
}
#[tauri::command]
async fn storage_plan(instance_id:String,location:String,operation:String,source:String,recovery:String,direct_restore:Option<bool>,app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value>{
    let _lock=state.busy.lock().await;
    Ok(blocking("storage-plan",move||{
        let args=storage_plan_args(&instance_id,&location,&operation,&source,&recovery,direct_restore.unwrap_or(false))?;
        run_helper_progress(&args,Some(app))
    }).await)
}
fn storage_plan_args<'a>(id:&'a str,location:&'a str,operation:&'a str,source:&'a str,recovery:&'a str,direct:bool)->Result<Vec<&'a str>>{
    if direct{
        if !recovery.is_empty()||!["restore","restore-archive"].contains(&operation){return Err(fail("COMMAND_NOT_ALLOWED","Direct restore cannot be used for installation or combined with a recovery backup"));}
        Ok(vec!["storage-plan-direct-restore",id,location,operation,source])
    }else{Ok(vec!["storage-plan-backed",id,location,operation,source,recovery])}
}
#[tauri::command]
async fn storage_execute(plan_id:String,confirmed:bool,app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value>{
    if !confirmed{return Ok(envelope(Err(fail("CONFIRM_WRITE","Explicit confirmation required"))));}
    let _lock=state.busy.lock().await;
    state.writing.store(true,std::sync::atomic::Ordering::SeqCst);
    let result=blocking("storage-execute",move||run_helper_progress(&["storage-execute",&plan_id,k11c_usb::flash::confirmed_flag()],Some(app))).await;
    state.writing.store(false,std::sync::atomic::Ordering::SeqCst);
    Ok(result)
}
#[tauri::command]
fn open_backups()->Value{envelope((||{
    let p=base_dir()?.join("backup");std::fs::create_dir_all(&p).map_err(|e|fail("BACKUP_IO",e))?;
    Command::new(win::windows_dir()?.join("explorer.exe")).arg(&p).spawn().map_err(|e|fail("OPEN_FOLDER",e))?;
    Ok(json!({"path":p}))
})())}
#[tauri::command]
async fn ensure_driver(state:tauri::State<'_,State>)->Result<Value> {
    let _lock=state.busy.lock().await;
    struct WindowsSetup;
    impl driver::SetupHost for WindowsSetup {
        fn probe(&mut self)->Result<driver::DriverStatus>{serde_json::from_value(run_helper(&["driver-status"])?).map_err(|e|fail("DRIVER_PROBE",e))}
        fn verify(&mut self)->Result<()>{driver::verify_bundle(&base_dir()?)?;helper_path()?;Ok(())}
        fn elevate(&mut self)->Result<u32>{win::elevate_helper(&helper_path()?)}
    }
    let attempted=state.attempted.clone();
    Ok(blocking("driver-setup",move||driver::ensure(&mut WindowsSetup,&attempted)).await)
}
#[tauri::command]
fn history()->Value {
    envelope((||{
        let p=base_dir()?.join("data/operations.jsonl");
        if !p.exists(){return Ok(json!([]));}
        let text=std::fs::read_to_string(p).map_err(|e|fail("HISTORY",e))?;
        Ok(json!(text.lines().rev().take(100).filter_map(|l|serde_json::from_str::<Value>(l).ok()).collect::<Vec<_>>()))
    })())
}
#[tauri::command]
async fn image_releases(state:tauri::State<'_,State>)->Result<Value> {
    let _lock=match state.image_guard().await {Ok(lock)=>lock,Err(e)=>return Ok(envelope(Err(e)))};
    Ok(blocking("image-releases",|| {
        let _lock=win::operation_lock()?;
        Ok(json!(k11c_usb::images::releases(&k11c_usb::images::Http::new()?)?))
    }).await)
}
#[tauri::command]
async fn image_download(version:String,app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value> {
    let _lock=match state.image_guard().await {Ok(lock)=>lock,Err(e)=>return Ok(envelope(Err(e)))};
    let cancel=state.image_cancel.clone();cancel.store(false,std::sync::atomic::Ordering::Relaxed);
    Ok(blocking("image-download",move|| {
        let _lock=win::operation_lock()?;
        let mut notify=|p:k11c_usb::images::Progress|{let _=app.emit("image-progress",p);};
        let mut job=k11c_usb::images::Job::new(&cancel,&mut notify);
        let net=k11c_usb::images::Http::new()?;let base=base_dir()?;
        let image=k11c_usb::images::download(&net,&version,&base,&mut job)?;
        k11c_usb::components::prepare(&net,&base,image,&mut job)
    }).await)
}
mod language;
#[tauri::command]
async fn backup_select(locale:Option<String>,window:tauri::WebviewWindow,app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value>{
    let _lock=state.busy.lock().await;
    Ok(blocking("backup-import",move||{
        let _lock=win::operation_lock()?;
        let (title,filter)=language::backup_picker(locale.as_deref());
        let Some(path)=rfd::FileDialog::new().set_parent(&window).set_title(title).add_filter(filter,&["k11cbackup","img"]).pick_file() else{return Ok(Value::Null);};
        crate::k11c_backup_import(&path,&app)
    }).await)
}
fn k11c_backup_import(path:&std::path::Path,app:&tauri::AppHandle)->Result<Value>{
    k11c_usb::archive::import(&base_dir()?,path,&mut |p:k11c_usb::workflow::Progress|{let _=app.emit("usb-progress",p);})
}
#[tauri::command]
async fn factory_select(locale:Option<String>,window:tauri::WebviewWindow,app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value>{
    let _lock=state.busy.lock().await;
    Ok(blocking("factory-import",move||{
        let (title,filter)=language::factory_picker(locale.as_deref());
        let Some(path)=rfd::FileDialog::new().set_parent(&window).set_title(title).add_filter(filter,&["img"]).pick_file() else{return Ok(Value::Null);};
        let path=path.to_str().ok_or_else(||fail("FACTORY_IMAGE","Invalid image path"))?;
        run_helper_progress(&["factory-import",path],Some(app))
    }).await)
}
#[tauri::command]
async fn factory_plan(instance_id:String,location:String,source:String,app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value>{
    let _lock=state.busy.lock().await;
    Ok(blocking("factory-plan",move||run_helper_progress(&["factory-plan",&instance_id,&location,&source],Some(app))).await)
}
#[tauri::command]
async fn factory_execute(plan_id:String,confirmed:bool,app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value>{
    if !confirmed{return Ok(envelope(Err(fail("CONFIRM_WRITE","Explicit confirmation required"))));}
    let _lock=state.busy.lock().await;state.writing.store(true,std::sync::atomic::Ordering::SeqCst);
    let result=blocking("factory-execute",move||run_helper_progress(&["factory-execute",&plan_id,k11c_usb::flash::confirmed_flag()],Some(app))).await;
    state.writing.store(false,std::sync::atomic::Ordering::SeqCst);Ok(result)
}
#[tauri::command]
async fn image_select(locale:Option<String>,window:tauri::WebviewWindow,app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value> {
    let _lock=match state.image_guard().await {Ok(lock)=>lock,Err(e)=>return Ok(envelope(Err(e)))};
    let cancel=state.image_cancel.clone();cancel.store(false,std::sync::atomic::Ordering::Relaxed);
    Ok(blocking("image-import",move|| {
        let _lock=win::operation_lock()?;
        let (title,filter)=language::image_picker(locale.as_deref());
        let Some(path)=rfd::FileDialog::new().set_parent(&window).set_title(title).add_filter(filter,&["img","xz"]).pick_file() else{return Ok(Value::Null);};
        let mut notify=|p:k11c_usb::images::Progress|{let _=app.emit("image-progress",p);};
        let mut job=k11c_usb::images::Job::new(&cancel,&mut notify);
        let base=base_dir()?;let image=k11c_usb::images::import(&path,&base,&mut job)?;
        k11c_usb::components::prepare(&k11c_usb::images::Http::new()?,&base,image,&mut job)
    }).await)
}
#[tauri::command]
async fn boot_prepare(app:tauri::AppHandle,state:tauri::State<'_,State>)->Result<Value>{
    let _lock=state.image_guard().await?;
    let cancel=state.image_cancel.clone();cancel.store(false,std::sync::atomic::Ordering::Relaxed);
    Ok(blocking("boot-prepare",move||{
        let _lock=win::operation_lock()?;
        let mut notify=|p:k11c_usb::images::Progress|{let _=app.emit("image-progress",p);};
        let mut job=k11c_usb::images::Job::new(&cancel,&mut notify);
        k11c_usb::components::prepare_boot(&k11c_usb::images::Http::new()?,&base_dir()?,&mut job)
    }).await)
}
#[tauri::command]
async fn connectivity_plan(address:String,token:String,operation:String,state:tauri::State<'_,State>)->Result<Value>{
    let _lock=state.busy.lock().await;
    *state.maintenance.lock().await=None;
    let result=async{
        let client=k11c_usb::maintenance::Client::new(&address,token)?;
        let catalog=if operation=="reinstall"{Some(tauri::async_runtime::spawn_blocking(||{
            let v=k11c_usb::components::catalog(&k11c_usb::images::Http::new()?,&base_dir()?)?;
            serde_json::from_value::<k11c_usb::components::Catalog>(v["catalog"].clone()).map_err(|e|fail("COMPONENT_CATALOG",e))
        }).await.map_err(|e|fail("WORKER",e))??)}else{None};
        let plan=k11c_usb::maintenance::plan(&client,catalog.as_ref(),&operation).await?;
        let value=json!(plan);
        *state.maintenance.lock().await=Some((client,plan));
        Ok(value)
    }.await;
    Ok(envelope(result))
}
#[tauri::command]
async fn connectivity_execute(confirmed:bool,state:tauri::State<'_,State>)->Result<Value>{
    if !confirmed{return Ok(envelope(Err(fail("CONFIRM_WRITE","Confirm removal of Connectivity settings and data"))));}
    let _lock=state.busy.lock().await;
    let Some((client,plan))=state.maintenance.lock().await.take() else{return Ok(envelope(Err(fail("HA_PLAN","Check the target first"))));};
    state.writing.store(true,std::sync::atomic::Ordering::SeqCst);
    let result=envelope(k11c_usb::maintenance::execute(&client,&plan,confirmed).await);
    state.writing.store(false,std::sync::atomic::Ordering::SeqCst);
    let _=record("connectivity-maintenance",&result);Ok(result)
}
#[tauri::command]
async fn connectivity_forget(state:tauri::State<'_,State>)->Result<Value>{
    *state.maintenance.lock().await=None;Ok(envelope(Ok(Value::Null)))
}
#[tauri::command]
fn image_cancel(state:tauri::State<'_,State>)->Value {
    state.image_cancel.store(true,std::sync::atomic::Ordering::Relaxed);
    envelope(Ok(json!({"requested":true})))
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn advanced_restore_dispatch_keeps_other_backups(){
        for op in ["restore","restore-archive"]{
            let a=storage_plan_args("id","port",op,"source","",true).unwrap();
            assert_eq!(a,vec!["storage-plan-direct-restore","id","port",op,"source"]);
            assert!(storage_plan_args("id","port",op,"source","backup",true).is_err());
        }
        for op in ["install","uboot","gpt-repair"]{assert!(storage_plan_args("id","port",op,"source","",true).is_err());}
        assert_eq!(storage_plan_args("id","port","restore-archive","source","FULL-test.k11cbackup",false).unwrap(),vec!["storage-plan-backed","id","port","restore-archive","source","FULL-test.k11cbackup"]);
    }
    #[tokio::test]
    async fn image_action_waits_for_poll_then_owns_lock() {
        let state=State{busy:Mutex::new(()),attempted:Arc::new(AtomicBool::new(false)),image_cancel:Arc::new(AtomicBool::new(false)),writing:AtomicBool::new(false),maintenance:Mutex::new(None)};
        let poll=state.busy.lock().await;
        let mut image=std::pin::pin!(state.image_guard());
        assert!(tokio::time::timeout(Duration::from_millis(20),&mut image).await.is_err());
        drop(poll);
        let guard=tokio::time::timeout(Duration::from_secs(1),image).await.unwrap().unwrap();
        assert!(state.busy.try_lock().is_err());
        drop(guard);assert!(state.busy.try_lock().is_ok());
    }
}
fn main() {
    tauri::Builder::default().manage(State{busy:Mutex::new(()),attempted:Arc::new(AtomicBool::new(false)),image_cancel:Arc::new(AtomicBool::new(false)),writing:AtomicBool::new(false),maintenance:Mutex::new(None)})
        .on_window_event(|window,event|{
            if let tauri::WindowEvent::CloseRequested{api,..}=event {
                if window.state::<State>().writing.load(std::sync::atomic::Ordering::SeqCst){api.prevent_close();let _=window.emit("write-close-blocked",());}
            }
        })
        .setup(|app|{
            let base=base_dir().map_err(|e|std::io::Error::other(e.detail))?;
            let data=base.join("data/webview");std::fs::create_dir_all(&data)?;
            tauri::WebviewWindowBuilder::from_config(app,&app.config().app.windows[0])?
                .data_directory(data).build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![preflight,list_devices,ensure_driver,inspect_device,prepare_device,backup_device,backup_select,open_backups,history,image_releases,image_download,image_select,image_cancel,boot_prepare,gpt_check,backup_catalog,storage_plan,storage_execute,factory_select,factory_plan,factory_execute,connectivity_plan,connectivity_execute,connectivity_forget])
        .run(tauri::generate_context!()).expect("K11C Installer could not start; WebView2 Runtime is required");
}
