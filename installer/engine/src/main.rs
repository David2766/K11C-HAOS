use k11c_usb::{Action, Result, envelope, parse_action, base_dir, driver, usb};
use serde_json::{Value,json};

#[tokio::main]
async fn main() {
    let args:Vec<String>=std::env::args().skip(1).collect();
    let action=parse_action(&args);
    // Hold one OS-owned lock for the entire USB/driver operation, including
    // re-enumeration. Process death releases it; no stale lock files.
    let _lock=match k11c_usb::win::operation_lock(){Ok(lock)=>lock,Err(e)=>{println!("{}",envelope(Err(e)));std::process::exit(1)}};
    if matches!(action,Ok(Action::InstallDriver)) {
        match base_dir().and_then(|base|driver::install(&base)) {
            Ok(code)=>std::process::exit(code as i32),
            Err(e)=>{eprintln!("{}",envelope(Err(e)));std::process::exit(1)}
        }
    }
    let result:Result<Value>=async {
        match action? {
            Action::Preflight=>Ok(json!({"driver":driver::status()?,"devices":usb::devices().await?,"version":env!("CARGO_PKG_VERSION")})),
            Action::DriverStatus=>Ok(json!(driver::status()?)),
            Action::Devices=>Ok(json!(usb::devices().await?)),
            Action::GptCheck(id)=>usb::gpt_check(&id).await,
            Action::BackupCatalog=>k11c_usb::flash::catalog(&base_dir()?),
            Action::StoragePlan(id,location,op,source)=>usb::storage_plan(&id,&location,&op,&source,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())).await,
            Action::StoragePlanBacked(id,location,op,source,recovery)=>usb::storage_plan_backed(&id,&location,&op,&source,&recovery,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())).await,
            Action::StoragePlanDirectRestore(id,location,op,source)=>usb::storage_plan_direct_restore(&id,&location,&op,&source,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())).await,
            Action::StoragePlanWithoutBackup(id,location,op,source)=>usb::storage_plan_without_backup(&id,&location,&op,&source,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())).await,
            Action::ArchiveBackup(id,kind)=>usb::archive_backup(&id,&kind,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())).await,
            Action::StorageExecute(id)=>usb::storage_execute(&id,true,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())).await,
            Action::StorageExecuteGated(id)=>{
                usb::storage_execute_gated(&id,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap()),&mut ||{
                    use std::io::{Read,Write};
                    eprintln!("{}",serde_json::json!({"phase":"write-ready","completed":0,"total":0}));
                    std::io::stderr().flush().map_err(|e|k11c_usb::fail("STORAGE_CANCELLED",e))?;
                    let mut permit=[0];std::io::stdin().read_exact(&mut permit).map_err(|_|k11c_usb::fail("STORAGE_CANCELLED","Write permission was cancelled"))?;
                    if permit!=[b'G']{return Err(k11c_usb::fail("STORAGE_CANCELLED","Write permission was cancelled"));}Ok(())
                }).await
            },
            Action::FactoryImport(path)=>k11c_usb::factory::import(&base_dir()?,std::path::Path::new(&path),&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())),
            Action::FactoryPlan(id,location,source)=>usb::factory_plan(&id,&location,&source,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())).await,
            Action::FactoryExecute(id)=>usb::factory_execute(&id,true,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())).await,
            Action::Inspect(id)=>usb::inspect(&id).await,
            Action::Prepare(id,location)=>usb::prepare(&id,&location,true,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())).await,
            Action::Backup(id)=>usb::backup(&id,&base_dir()?,&mut |p|eprintln!("{}",serde_json::to_string(&p).unwrap())).await,
            action @ (Action::ImageReleases | Action::ImageDownload(_) | Action::ImageImport(_) | Action::InstallationImport(_) | Action::BootPrepare) => tokio::task::spawn_blocking(move|| {
                use k11c_usb::images;
                let cancel=std::sync::atomic::AtomicBool::new(false);
                let mut notify=|p|eprintln!("{}",serde_json::to_string(&p).unwrap());
                let mut job=images::Job::new(&cancel,&mut notify);
                match action {
                    Action::ImageReleases=>Ok(json!(images::releases(&images::Http::new()?)?)),
                    Action::ImageDownload(v)=>Ok(json!(images::download(&images::Http::new()?,&v,&base_dir()?,&mut job)?)),
                    Action::ImageImport(p)=>Ok(json!(images::import(std::path::Path::new(&p),&base_dir()?,&mut job)?)),
                    Action::InstallationImport(p)=>{let base=base_dir()?;let raw=images::import(std::path::Path::new(&p),&base,&mut job)?;k11c_usb::components::prepare(&images::Http::new()?,&base,raw,&mut job)},
                    Action::BootPrepare=>k11c_usb::components::prepare_boot(&images::Http::new()?,&base_dir()?,&mut job),
                    _=>unreachable!(),
                }
            }).await.map_err(|e|k11c_usb::fail("WORKER",e))?,
            Action::InstallDriver=>unreachable!(),
        }
    }.await;
    let exit=if result.is_ok(){0}else{1};
    println!("{}",envelope(result));std::process::exit(exit);
}
