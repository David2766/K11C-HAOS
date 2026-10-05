//! Fixed Windows helper. Only a new, app-owned data-partition copy is writable.
use crate::{fail,Result,components::Seed,images::{self,Job}};
use serde::{Serialize,Deserialize};
use serde_json::json;
use std::{path::Path,io::{BufRead,BufReader,Read},process::{Command,Stdio},sync::mpsc,time::{Instant,Duration}};

#[derive(Debug,Serialize,Deserialize)]
pub struct Receipt{pub bytes:u64,pub sha256:String,pub seconds:f64,pub existing_images:u64}
pub trait Builder{fn build(&self,base:&Path,raw:&Path,profile:&Seed,output:&Path,job:&mut Job<'_>)->Result<Receipt>;}
pub struct Native;
impl Builder for Native{
 fn build(&self,base:&Path,raw:&Path,profile:&Seed,output:&Path,job:&mut Job<'_>)->Result<Receipt>{
  use std::os::windows::process::CommandExt;
  let dir=base.join("resources/native");let exe=dir.join("k11c-seed.exe");
  let expected=option_env!("K11C_NATIVE_SHA256").unwrap_or("");
  if !crate::components::hash_id(expected){return Err(fail("COMPONENT_PREPARE","Native preparation helper not included in this build"));}
  crate::verify_file(&exe,expected)?;
  let (request,mut file)=images::Temporary::new(output.parent().unwrap(),"request")?;
  use std::io::Write;
  file.write_all(&serde_json::to_vec(&json!({"raw":raw,"cache":base.join("data/components/registry"),"profile":profile})).map_err(|e|fail("COMPONENT_PREPARE",e))?).map_err(|e|fail("COMPONENT_IO",e))?;drop(file);
  let mut child=Command::new(&exe).args(["prepare"]).arg(&request.path).arg(output).current_dir(&dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(0x08000000).spawn().map_err(|e|fail("COMPONENT_PREPARE",e))?;
  let guard=match crate::win::contain_child(child.id()){Ok(g)=>g,Err(e)=>{let _=child.kill();let _=child.wait();return Err(e)}};
  let (send,recv)=mpsc::sync_channel(64);
  let mut readers=vec![];
  let inputs:[(bool,Box<dyn Read+Send>);2]=[(false,Box::new(child.stdout.take().unwrap())),(true,Box::new(child.stderr.take().unwrap()))];
  for (stream,input) in inputs{
   let send=send.clone();readers.push(std::thread::spawn(move||{let mut r=BufReader::new(input);loop{let mut line=vec![];match r.by_ref().take(1024*1024+1).read_until(b'\n',&mut line){Ok(0)=>break,Ok(_) if line.len()<=1024*1024=>{if send.send((stream,line)).is_err(){break}},_=>break}}}));
  }
  drop(send);let began=Instant::now();let mut stdout=vec![];let mut detail=vec![];let mut stopped=None;
  loop{
   match recv.recv_timeout(Duration::from_millis(50)){
    Ok((stderr,line))=>{
     if stderr{if let Ok(v)=serde_json::from_slice::<serde_json::Value>(&line){if let(Some(phase),Some(n))=(v["phase"].as_str(),v["completed"].as_u64()){job.progress(phase,n,v["total"].as_u64(),true);continue;}}
      if detail.len()+line.len()<=256*1024{detail.extend(line)}
     }else if stdout.len()+line.len()<=1024*1024{stdout.extend(line)}else{stopped=Some(fail("COMPONENT_PREPARE","Native helper output exceeded limit"));break}
    },
    Err(mpsc::RecvTimeoutError::Disconnected)=>break,
    Err(mpsc::RecvTimeoutError::Timeout)=>{},
   }
   if let Err(e)=job.check(){stopped=Some(e);break}
   if began.elapsed()>Duration::from_secs(1800){stopped=Some(fail("COMPONENT_PREPARE","Native preparation timed out"));break}
  }
  // Closing the Job also terminates any filesystem-tool child on cancellation.
  drop(recv);if stopped.is_some(){drop(guard);let _=child.wait();for r in readers{let _=r.join();}return Err(stopped.unwrap())}
  let status=child.wait().map_err(|e|fail("COMPONENT_PREPARE",e))?;drop(guard);for r in readers{let _=r.join();}
  if !status.success(){return Err(fail("COMPONENT_PREPARE",String::from_utf8_lossy(&detail)))}
  let receipt:Receipt=serde_json::from_slice(&stdout).map_err(|e|fail("COMPONENT_PREPARE",e))?;
  if receipt.existing_images==0||receipt.bytes<2048||receipt.bytes%512!=0||!receipt.seconds.is_finite()||receipt.seconds<0.0||images::file_hash(output,job,"verify-component")?!=(receipt.bytes,receipt.sha256.clone()){return Err(fail("COMPONENT_HASH","Native preparation receipt mismatch"));}
  Ok(receipt)
 }
}
