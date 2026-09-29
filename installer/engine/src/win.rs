use crate::{Result, fail};
use std::{path::{Path, PathBuf}, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError},
    System::{SystemInformation::GetWindowsDirectoryW, Threading::{WaitForSingleObject, GetExitCodeProcess, INFINITE}},
    UI::{Shell::{ShellExecuteExW, SHELLEXECUTEINFOW, SEE_MASK_NOCLOSEPROCESS, IsUserAnAdmin}, WindowsAndMessaging::SW_HIDE},
    Devices::DeviceAndDriverInstallation::SetupGetInfDriverStoreLocationW,
    Security::WinTrust::{WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO, WTD_UI_NONE, WTD_CHOICE_FILE, WTD_STATEACTION_VERIFY, WTD_STATEACTION_CLOSE, WTD_CACHE_ONLY_URL_RETRIEVAL},
};
pub fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(Some(0)).collect()
}
pub fn windows_dir() -> Result<PathBuf> {
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() { return Err(fail("WINDOWS_PATH", "GetWindowsDirectoryW failed")); }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}
pub fn store_inf(inf: &Path) -> Option<PathBuf> {
    let input = wide(inf.as_os_str()); let mut buf = vec![0u16;32768];
    let mut required = 0;
    let ok = unsafe { SetupGetInfDriverStoreLocationW(input.as_ptr(),ptr::null(),ptr::null(),buf.as_mut_ptr(),buf.len() as u32,&mut required) };
    if ok == 0 || required < 2 { return None; }
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..required as usize-1])))
}
pub fn signed(path: &Path) -> bool {
    let p = wide(path.as_os_str());
    let mut file: WINTRUST_FILE_INFO = unsafe { std::mem::zeroed() };
    file.cbStruct = std::mem::size_of_val(&file) as u32; file.pcwszFilePath = p.as_ptr();
    let mut data: WINTRUST_DATA = unsafe { std::mem::zeroed() };
    data.cbStruct = std::mem::size_of_val(&data) as u32; data.dwUIChoice = WTD_UI_NONE;
    data.dwUnionChoice = WTD_CHOICE_FILE; data.Anonymous.pFile = &mut file;
    data.dwStateAction = WTD_STATEACTION_VERIFY; data.dwProvFlags = WTD_CACHE_ONLY_URL_RETRIEVAL;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let code = unsafe { WinVerifyTrust(ptr::null_mut(), &mut action, (&mut data as *mut WINTRUST_DATA).cast()) };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe { WinVerifyTrust(ptr::null_mut(), &mut action, (&mut data as *mut WINTRUST_DATA).cast()); }
    code == 0
}
pub fn is_admin() -> bool { unsafe { IsUserAnAdmin() != 0 } }
pub struct ChildJob(windows_sys::Win32::Foundation::HANDLE);
impl Drop for ChildJob{fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
pub fn contain_child(pid:u32)->Result<ChildJob>{
    use windows_sys::Win32::System::{JobObjects::{CreateJobObjectW,SetInformationJobObject,AssignProcessToJobObject,JobObjectExtendedLimitInformation,JOBOBJECT_EXTENDED_LIMIT_INFORMATION,JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE},Threading::{OpenProcess,PROCESS_SET_QUOTA,PROCESS_TERMINATE}};
    let handle=unsafe{CreateJobObjectW(ptr::null(),ptr::null())};
    if handle.is_null(){return Err(fail("HELPER_JOB",unsafe{GetLastError()}));}
    let job=ChildJob(handle);let mut info:JOBOBJECT_EXTENDED_LIMIT_INFORMATION=unsafe{std::mem::zeroed()};
    info.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe{SetInformationJobObject(handle,JobObjectExtendedLimitInformation,(&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),std::mem::size_of_val(&info) as u32)}==0{return Err(fail("HELPER_JOB",unsafe{GetLastError()}));}
    let process=unsafe{OpenProcess(PROCESS_SET_QUOTA|PROCESS_TERMINATE,0,pid)};
    if process.is_null(){return Err(fail("HELPER_JOB",unsafe{GetLastError()}));}
    let ok=unsafe{AssignProcessToJobObject(handle,process)};let error=unsafe{GetLastError()};unsafe{CloseHandle(process);}
    if ok==0{return Err(fail("HELPER_JOB",error));}Ok(job)
}
pub struct OperationLock(windows_sys::Win32::Foundation::HANDLE);
impl Drop for OperationLock {fn drop(&mut self){unsafe{windows_sys::Win32::System::Threading::ReleaseMutex(self.0);CloseHandle(self.0);}}}
pub fn operation_lock()->Result<OperationLock>{
    use windows_sys::Win32::{System::Threading::{CreateMutexW,WaitForSingleObject},Foundation::{WAIT_OBJECT_0,WAIT_ABANDONED}};
    let name=wide("Local\\K11CInstallerUsbOperation".as_ref());
    let h=unsafe{CreateMutexW(ptr::null(),0,name.as_ptr())};
    if h.is_null(){return Err(fail("OPERATION_LOCK",unsafe{GetLastError()}));}
    let result=unsafe{WaitForSingleObject(h,0)};
    if result!=WAIT_OBJECT_0&&result!=WAIT_ABANDONED {unsafe{CloseHandle(h);}return Err(fail("DEVICE_BUSY","Another K11C operation is running"));}
    Ok(OperationLock(h))
}
pub fn usb_interface(instance_id:&str)->Result<Option<String>> {
    use windows_sys::Win32::Devices::{Usb::GUID_DEVINTERFACE_USB_DEVICE,DeviceAndDriverInstallation::{CM_Get_Device_Interface_ListW,CM_Get_Device_Interface_List_SizeW,CM_GET_DEVICE_INTERFACE_LIST_PRESENT,CR_SUCCESS}};
    let id=wide(instance_id.as_ref());let mut len=0;
    let status=unsafe{CM_Get_Device_Interface_List_SizeW(&mut len,&GUID_DEVINTERFACE_USB_DEVICE,id.as_ptr(),CM_GET_DEVICE_INTERFACE_LIST_PRESENT)};
    if status!=CR_SUCCESS{return Err(fail("USB_INTERFACE",format!("CM list size status={status}")));}
    if len<=1{return Ok(None);}
    let mut paths=vec![0u16;len as usize];
    let status=unsafe{CM_Get_Device_Interface_ListW(&GUID_DEVINTERFACE_USB_DEVICE,id.as_ptr(),paths.as_mut_ptr(),len,CM_GET_DEVICE_INTERFACE_LIST_PRESENT)};
    if status!=CR_SUCCESS{return Err(fail("USB_INTERFACE",format!("CM list status={status}")));}
    let list:Vec<String>=paths.split(|c|*c==0).filter(|s|!s.is_empty()).map(String::from_utf16_lossy).collect();
    match list.as_slice(){[]=>Ok(None),[only]=>Ok(Some(only.clone())),_=>Err(fail("USB_INTERFACE","Multiple interfaces for the selected instance"))}
}
pub fn elevate_helper(helper: &Path) -> Result<u32> {
    let exe = wide(helper.as_os_str()); let verb = wide("runas".as_ref());
    let parameters = wide("driver-install-only".as_ref());
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of_val(&info) as u32; info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr(); info.lpFile = exe.as_ptr(); info.lpParameters = parameters.as_ptr(); info.nShow = SW_HIDE;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let code = unsafe { GetLastError() };
        return Err(fail(if code == 1223 {"UAC_CANCELLED"} else {"ELEVATION_FAILED"},code));
    }
    let mut exit = 1;
    unsafe { WaitForSingleObject(info.hProcess,INFINITE); GetExitCodeProcess(info.hProcess,&mut exit); CloseHandle(info.hProcess); }
    Ok(exit)
}
