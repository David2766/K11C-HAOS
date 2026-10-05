//! A bounded, read-only Rockusb readiness probe. USB descriptor mode is not
//! readiness: the RK356x RAM USB plug can retain the Maskrom descriptor.
use crate::{Result,fail};
use windows_sys::Win32::{Foundation::*,Storage::FileSystem::*,System::{IO::*,Threading::*}};

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum State { Ready, Unavailable, Busy }
pub fn require_ready(state:State)->Result<()>{
    if state!=State::Ready{return Err(fail("USB_NOT_READY","No successful Rockusb readiness response"));}Ok(())
}

fn response(bytes:&[u8],tag:&[u8])->Result<State>{
    if bytes.len()!=13 || &bytes[..4]!=b"USBS" || &bytes[4..8]!=tag {
        return Err(fail("USB_PROTOCOL","Invalid readiness response signature/tag/length"));
    }
    match bytes[12] {0=>Ok(State::Ready),1=>Ok(State::Busy),_=>Err(fail("USB_PROTOCOL","Invalid readiness status"))}
}
struct Handle(HANDLE);
impl Drop for Handle {fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
fn open(path:&str,access:u32)->Result<Handle>{
    let wide:Vec<u16>=path.encode_utf16().chain(Some(0)).collect();
    let h=unsafe{CreateFileW(wide.as_ptr(),access,FILE_SHARE_READ|FILE_SHARE_WRITE,std::ptr::null(),OPEN_EXISTING,FILE_FLAG_OVERLAPPED,std::ptr::null_mut())};
    if h==INVALID_HANDLE_VALUE{return Err(fail("USB_OPEN",std::io::Error::last_os_error()));}Ok(Handle(h))
}
// Unlike the upstream synchronous transport, cancellation gives bare BootROM
// (which may have no bulk command service yet) a bounded probe interval.
fn transfer(h:&Handle,bytes:&mut[u8],write:bool)->Result<Option<usize>>{
    let event=unsafe{CreateEventW(std::ptr::null(),1,0,std::ptr::null())};
    if event.is_null(){return Err(fail("USB_OPEN",std::io::Error::last_os_error()));}
    let event=Handle(event);
    let mut ov:OVERLAPPED=unsafe{std::mem::zeroed()};ov.hEvent=event.0;
    let mut done=0;
    let ok=unsafe{if write {WriteFile(h.0,bytes.as_ptr(),bytes.len() as u32,&mut done,&mut ov)}else{ReadFile(h.0,bytes.as_mut_ptr(),bytes.len() as u32,&mut done,&mut ov)}};
    if ok==0 {
        let code=unsafe{GetLastError()};
        if code!=ERROR_IO_PENDING {
            if [ERROR_GEN_FAILURE,ERROR_SEM_TIMEOUT].contains(&code){return Ok(None);}
            return Err(fail("USB_READ",format!("Readiness transfer: Windows error {code}")));
        }
        let wait=unsafe{WaitForSingleObject(event.0,1200)};
        if wait!=WAIT_OBJECT_0 {
            // Drain the cancellation before dropping the buffer/OVERLAPPED.
            unsafe{CancelIoEx(h.0,&ov);GetOverlappedResult(h.0,&ov,&mut done,1);}
            if wait==WAIT_TIMEOUT{return Ok(None);}
            return Err(fail("USB_READ","Readiness wait failed"));
        }
        if unsafe{GetOverlappedResult(h.0,&ov,&mut done,0)}==0 {
            let code=unsafe{GetLastError()};
            if [ERROR_GEN_FAILURE,ERROR_SEM_TIMEOUT].contains(&code){return Ok(None);}
            return Err(fail("USB_READ",format!("Readiness completion: Windows error {code}")));
        }
    }
    Ok(Some(done as usize))
}
pub fn probe(path:&str)->Result<State>{
    let input=open(&format!("{path}\\pipe00"),GENERIC_READ)?;
    let output=open(&format!("{path}\\pipe01"),GENERIC_WRITE)?;
    let mut request=[0u8;31];
    rockusb::protocol::CommandBlock::test_unit_ready(0).to_bytes(&mut request);
    match transfer(&output,&mut request,true)? {
        None=>return Ok(State::Unavailable),Some(31)=>{},Some(_)=>return Err(fail("USB_PROTOCOL","Short readiness request"))
    }
    let mut status=[0u8;13];
    match transfer(&input,&mut status,false)? {
        None=>Ok(State::Unavailable),Some(n)=>response(&status[..n.min(13)],&request[4..8])
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn only_matching_success_is_ready(){
        require_ready(State::Ready).unwrap();
        assert!(require_ready(State::Unavailable).is_err());assert!(require_ready(State::Busy).is_err());
        let mut b=*b"USBS1234\0\0\0\0\0";
        assert_eq!(response(&b,b"1234").unwrap(),State::Ready);
        b[12]=1;assert_eq!(response(&b,b"1234").unwrap(),State::Busy);
        b[12]=2;assert!(response(&b,b"1234").is_err());b[12]=0;
        assert!(response(&b,b"5678").is_err());assert!(response(&b[..12],b"1234").is_err());
        b[0]=0;assert!(response(&b,b"1234").is_err());
    }
}
