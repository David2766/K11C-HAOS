use crate::{Result, fail, verify_file, win};
use serde::{Serialize, Deserialize};
use serde_json::{Value,json};
use std::sync::atomic::{AtomicBool,Ordering};
use std::{path::Path, process::Command, os::windows::process::CommandExt};

pub const FILES: [(&str,&str);3] = [
    ("rockusb.inf","4fc1ea1a32e235f03805b0955dc06203be15738d0672d9679a9539ccc23e8887"),
    ("rockusb.cat","f9098e5a0067b5d01cb6f5be87f42a196503aee7e2d1fc41d573c80fd5c9f10f"),
    ("rockusb.sys","dbe50ec840008e3db5f4acd6063f622438f3bbb049e4db84e1721ea0425fcac1")
];
#[derive(Clone,Debug,Serialize,Deserialize,Default)]
pub struct DriverStatus { pub installed: bool, pub published_inf: Option<String>, pub version: Option<String> }
fn inf_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[255,254]) { String::from_utf16_lossy(&bytes[2..].chunks_exact(2).map(|b|u16::from_le_bytes([b[0],b[1]])).collect::<Vec<_>>()) }
    else { String::from_utf8_lossy(bytes).into_owned() }
}
pub fn supported_inf(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("{79dfc2a8-2574-493c-ae71-52958e41dd00}") && t.contains("usb\\vid_2207&pid_350a") && t.contains("rockusb.sys")
}
pub fn status() -> Result<DriverStatus> {
    let dir = win::windows_dir()?.join("INF");
    for entry in std::fs::read_dir(dir).map_err(|e| fail("DRIVER_PROBE",e))? {
        let entry = entry.map_err(|e|fail("DRIVER_PROBE",e))?;
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.starts_with("oem") || !name.ends_with(".inf") { continue; }
        let text = inf_text(&std::fs::read(entry.path()).map_err(|e|fail("DRIVER_PROBE",e))?);
        if !supported_inf(&text) { continue; }
        let Some(store) = win::store_inf(&entry.path()) else {continue};
        let Some(parent) = store.parent() else {continue};
        if !parent.join("rockusb.sys").is_file() || !win::signed(&parent.join("rockusb.cat")) {continue;}
        let version = text.lines().find(|l|l.trim().to_ascii_lowercase().starts_with("driverver"))
            .and_then(|l|l.split(',').nth(1)).map(|v|v.trim().to_owned());
        return Ok(DriverStatus {installed:true,published_inf:Some(name),version});
    }
    Ok(DriverStatus::default())
}
pub fn verify_bundle(base: &Path) -> Result<()> {
    let dir = base.join("resources/rockusb");
    for (name,digest) in FILES { verify_file(&dir.join(name),digest)?; }
    if !win::signed(&dir.join("rockusb.cat")) { return Err(fail("DRIVER_SIGNATURE", "Windows could not validate the bundled catalog")); }
    Ok(())
}
// This decision is shared by the real GUI and tests; no UI-only guard.
pub fn should_install(installed: bool, attempted: bool) -> bool { !installed && !attempted }
pub trait SetupHost {
    fn probe(&mut self)->Result<DriverStatus>;
    fn verify(&mut self)->Result<()>;
    fn elevate(&mut self)->Result<u32>;
}
pub fn ensure(host:&mut impl SetupHost,attempted:&AtomicBool)->Result<Value> {
    let before=host.probe()?;
    if !should_install(before.installed,attempted.load(Ordering::SeqCst)) {
        return Ok(json!({"state":if before.installed {"reused"} else {"not_retried"},"driver":before}));
    }
    // Process-local single attempt, including hash failures and UAC cancellation.
    if attempted.swap(true,Ordering::SeqCst) {return Ok(json!({"state":"not_retried"}));}
    host.verify()?;
    let code=host.elevate()?;
    if code==3010 {return Ok(json!({"state":"reboot_required"}));}
    if code!=0 {return Err(fail("DRIVER_INSTALL",format!("Helper exit={code}")));}
    let after=host.probe()?;
    if !after.installed {return Err(fail("DRIVER_NOT_VERIFIED","Installed package not found after helper success"));}
    Ok(json!({"state":"installed","driver":after}))
}
pub fn install_arguments(inf: &Path) -> Vec<std::ffi::OsString> {
    vec!["/add-driver".into(),inf.as_os_str().to_owned(),"/install".into()]
}
pub fn install(base: &Path) -> Result<u32> {
    if !should_install(status()?.installed,false) { return Ok(0); }
    verify_bundle(base)?;
    if !win::is_admin() { return Err(fail("ADMIN_REQUIRED", "UAC is required for driver installation")); }
    let executable = win::windows_dir()?.join("System32/pnputil.exe");
    let result = Command::new(executable).args(install_arguments(&base.join("resources/rockusb/rockusb.inf")))
        .creation_flags(0x08000000).output().map_err(|e|fail("DRIVER_INSTALL",e))?;
    let code = result.status.code().unwrap_or(1) as u32;
    if code == 3010 {return Ok(code);}
    if code != 0 {return Err(fail("DRIVER_INSTALL",format!("pnputil exit={code}; {}",String::from_utf8_lossy(&result.stdout))));}
    if !status()?.installed {return Err(fail("DRIVER_NOT_VERIFIED","Package was not found after installation"));}
    Ok(0)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn existing_driver_is_never_reinstalled() {
        assert!(!should_install(true,false)); assert!(!should_install(true,true));
        assert!(!should_install(false,true)); assert!(should_install(false,false));
    }
    #[test] fn installer_has_exact_non_destructive_arguments() {
        let p = Path::new("C:/portable 한글/resources/rockusb/rockusb.inf");
        assert_eq!(install_arguments(p),vec![std::ffi::OsString::from("/add-driver"),p.into(),"/install".into()]);
    }
    #[test] fn unrelated_driver_is_not_reused() {
        let sample = "{79DFC2A8-2574-493C-AE71-52958E41DD00} USB\\VID_2207&PID_350A rockusb.sys";
        assert!(supported_inf(sample));
        for part in ["{79DFC2A8-2574-493C-AE71-52958E41DD00}", "USB\\VID_2207&PID_350A", "rockusb.sys"] {assert!(!supported_inf(&sample.replace(part,"")));}
    }
    struct Mock { installed:bool, verified:bool, code:u32, cancelled:bool, appears:bool, calls:Vec<&'static str> }
    impl SetupHost for Mock {
        fn probe(&mut self)->Result<DriverStatus> {self.calls.push("probe");Ok(DriverStatus{installed:self.installed,..Default::default()})}
        fn verify(&mut self)->Result<()> {self.calls.push("verify");if self.verified {Ok(())}else{Err(fail("RESOURCE_HASH","altered"))}}
        fn elevate(&mut self)->Result<u32> {self.calls.push("elevate");if self.cancelled{return Err(fail("UAC_CANCELLED","1223"));}if self.appears{self.installed=true;}Ok(self.code)}
    }
    fn missing()->Mock {Mock{installed:false,verified:true,code:0,cancelled:false,appears:true,calls:vec![]}}
    #[test] fn installed_host_never_elevates() {
        let mut host=missing();host.installed=true;
        assert_eq!(ensure(&mut host,&AtomicBool::new(false)).unwrap()["state"],"reused");assert_eq!(host.calls,vec!["probe"]);
    }
    #[test] fn missing_host_installs_verifies_then_reuses() {
        let mut host=missing();let attempted=AtomicBool::new(false);
        assert_eq!(ensure(&mut host,&attempted).unwrap()["state"],"installed");
        assert_eq!(ensure(&mut host,&attempted).unwrap()["state"],"reused");
        assert_eq!(host.calls,vec!["probe","verify","elevate","probe","probe"]);
    }
    #[test] fn cancellation_is_not_a_retry_loop() {
        let mut host=missing();host.cancelled=true;let attempted=AtomicBool::new(false);
        assert_eq!(ensure(&mut host,&attempted).unwrap_err().code,"UAC_CANCELLED");
        assert_eq!(ensure(&mut host,&attempted).unwrap()["state"],"not_retried");
        assert_eq!(host.calls,vec!["probe","verify","elevate","probe"]);
    }
    #[test] fn tampered_bundle_never_elevates() {
        let mut host=missing();host.verified=false;
        assert_eq!(ensure(&mut host,&AtomicBool::new(false)).unwrap_err().code,"RESOURCE_HASH");assert_eq!(host.calls,vec!["probe","verify"]);
    }
    #[test] fn exit_zero_is_not_proof_of_install() {
        let mut host=missing();host.appears=false;
        assert_eq!(ensure(&mut host,&AtomicBool::new(false)).unwrap_err().code,"DRIVER_NOT_VERIFIED");
    }
    #[test] fn reboot_required_and_install_failure_are_distinct() {
        let mut host=missing();host.code=3010;host.appears=false;
        assert_eq!(ensure(&mut host,&AtomicBool::new(false)).unwrap()["state"],"reboot_required");
        assert_eq!(host.calls,vec!["probe","verify","elevate"]);
        host.code=5;assert_eq!(ensure(&mut host,&AtomicBool::new(false)).unwrap_err().code,"DRIVER_INSTALL");
    }
}
