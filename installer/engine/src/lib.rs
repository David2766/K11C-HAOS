pub mod driver;
pub mod usb;
pub mod win;
pub mod loader;
pub mod workflow;
pub mod gpt;
pub mod images;
pub mod flash;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failure { pub code: String, pub detail: String }
pub type Result<T> = std::result::Result<T, Failure>;
pub fn fail(code: &str, detail: impl ToString) -> Failure {
    Failure { code: code.into(), detail: detail.to_string() }
}
pub fn envelope(result: Result<Value>) -> Value {
    match result { Ok(data) => json!({"ok":true,"data":data}), Err(error) => json!({"ok":false,"error":error}) }
}
pub fn base_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().map_err(|e| fail("PATH", e))?;
    Ok(exe.parent().ok_or_else(|| fail("PATH", "No executable directory"))?.to_owned())
}
pub fn hash_bytes(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
pub fn verify_file(path: &Path, expected: &str) -> Result<()> {
    let bytes = std::fs::read(path).map_err(|e| fail("RESOURCE_MISSING", format!("{}: {e}",path.display())))?;
    if hash_bytes(&bytes) != expected { return Err(fail("RESOURCE_HASH", path.display())); }
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum Action { Preflight, DriverStatus, Devices, Inspect(String), Prepare(String,String), Backup(String), InstallDriver, ImageReleases, ImageDownload(String), ImageImport(String), GptCheck(String), BackupCatalog, StoragePlan(String,String,String,String), StorageExecute(String) }
pub fn parse_action(args: &[String]) -> Result<Action> {
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["preflight"] => Ok(Action::Preflight),
        ["driver-status"] => Ok(Action::DriverStatus),
        ["devices"] => Ok(Action::Devices),
        ["backup-catalog"] => Ok(Action::BackupCatalog),
        ["gpt-check",id] if !id.is_empty()&&id.len()<=512&&!id.contains('\0') => Ok(Action::GptCheck((*id).into())),
        ["storage-plan",id,location,op,source] if !id.is_empty()&&id.len()<=512&&!id.contains('\0')&&!location.is_empty()&&location.len()<=256&&!location.contains('\0')&&["install","uboot","restore","gpt-repair"].contains(op)&&source.len()<=120&&!source.contains('\0') => Ok(Action::StoragePlan((*id).into(),(*location).into(),(*op).into(),(*source).into())),
        ["storage-execute",id,flag] if *flag==flash::confirmed_flag()&&id.len()==64&&id.bytes().all(|b|b.is_ascii_hexdigit()) => Ok(Action::StorageExecute((*id).into())),
        ["image-releases"] => Ok(Action::ImageReleases),
        ["image-download", version] if images::valid_version(version) => Ok(Action::ImageDownload((*version).into())),
        ["image-import", path] if !path.is_empty() && !path.contains('\0') => Ok(Action::ImageImport((*path).into())),
        ["driver-install-only"] => Ok(Action::InstallDriver),
        ["inspect", id] if id.len() <= 512 && !id.contains('\0') => Ok(Action::Inspect((*id).into())),
        ["backup", id] if !id.is_empty() && id.len()<=512 && !id.contains('\0') => Ok(Action::Backup((*id).into())),
        ["prepare", id, location, "--confirmed-k11c"] if !id.is_empty() && id.len()<=512 && !location.is_empty() && location.len()<=256 && !id.contains('\0') && !location.contains('\0') => Ok(Action::Prepare((*id).into(),(*location).into())),
        _ => Err(fail("COMMAND_NOT_ALLOWED", "Unsupported operation or arguments")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn no_arbitrary_commands_or_extra_arguments() {
        for args in [vec!["write", "0", "image"], vec!["driver-install-only", "evil.inf"], vec!["preflight", "extra"], vec!["erase"], vec!["shell", "cmd"]] {
            assert!(parse_action(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err());
        }
        assert_eq!(parse_action(&["preflight".into()]).unwrap(), Action::Preflight);
        assert!(parse_action(&["prepare".into(),"device".into(),"port".into()]).is_err());
        assert_eq!(parse_action(&["prepare".into(),"device".into(),"port".into(),"--confirmed-k11c".into()]).unwrap(),Action::Prepare("device".into(),"port".into()));
    }
    #[test] fn digest_guard_uses_file_bytes() {
        let root = std::env::temp_dir().join(format!("k11c-test-{}-한글", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let p = root.join("payload"); std::fs::write(&p,b"valid").unwrap();
        verify_file(&p,&hash_bytes(b"valid")).unwrap();
        assert!(verify_file(&p,&hash_bytes(b"tampered")).is_err());
        std::fs::remove_file(p).unwrap(); std::fs::remove_dir(root).unwrap();
    }
}
