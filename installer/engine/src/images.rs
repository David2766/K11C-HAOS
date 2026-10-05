//! Official-image acquisition and read-only inspection. No USB transport here.
use crate::{fail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::{self, File, OpenOptions}, io::{self, Read, Write, Seek, SeekFrom, BufReader}, path::{Path, PathBuf}, sync::atomic::{AtomicBool, Ordering}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

const API: &str = "https://api.github.com/repos/home-assistant/operating-system/releases";
const DOWNLOAD: &str = "https://github.com/home-assistant/operating-system/releases/download";
const MAX_INPUT: u64 = 16 * 1024 * 1024 * 1024;
const MAX_IMAGE: u64 = 32 * 1024 * 1024 * 1024;
const MAX_JSON: u64 = 16 * 1024 * 1024;
const CHUNK: usize = 256 * 1024;
const LABELS: [&str; 8] = ["hassos-boot", "hassos-kernel0", "hassos-system0", "hassos-kernel1", "hassos-system1", "hassos-bootstate", "hassos-overlay", "hassos-data"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release { pub version: String, pub filename: String, pub size: u64, pub sha256: Option<String>, pub published: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedImage {
    pub path: PathBuf, pub filename: String, pub version: Option<String>, pub platform: String,
    pub bytes: u64, pub sha256: String, pub source_sha256: String, pub verification: String,
    pub partitions: usize, pub reused_download: bool, pub reused_image: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Progress { pub phase: String, pub completed: u64, pub total: Option<u64> }
pub struct Job<'a> { pub cancel: &'a AtomicBool, pub notify: &'a mut dyn FnMut(Progress), last: Instant }
impl<'a> Job<'a> {
    pub fn new(cancel: &'a AtomicBool, notify: &'a mut dyn FnMut(Progress)) -> Self { Self { cancel, notify, last: Instant::now() - Duration::from_secs(1) } }
    pub fn check(&self) -> Result<()> { if self.cancel.load(Ordering::Relaxed) { Err(fail("IMAGE_CANCELLED", "Image operation cancelled")) } else { Ok(()) } }
    pub(crate) fn progress(&mut self, phase: &str, n: u64, total: Option<u64>, force: bool) {
        if force || self.last.elapsed() >= Duration::from_millis(150) { (self.notify)(Progress { phase: phase.into(), completed: n, total }); self.last = Instant::now(); }
    }
}

pub fn valid_version(v: &str) -> bool {
    let parts: Vec<_> = v.split('.').collect();
    v.len() <= 32 && (2..=3).contains(&parts.len()) && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}
fn official_url(release: &Release) -> String { format!("{DOWNLOAD}/{}/{}", release.version, release.filename) }
fn allowed_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https" && url.username().is_empty() && url.password().is_none() && url.port_or_known_default() == Some(443)
        && matches!(url.host_str(), Some("api.github.com" | "github.com" | "raw.githubusercontent.com" | "release-assets.githubusercontent.com" | "objects.githubusercontent.com"))
}

// Async HTTP behind a synchronous stream lets the image pipeline share one
// Read-based implementation with local files and deterministic test transports.
// Every request/chunk is time-bounded; TLS certificate validation stays enabled.
pub struct Http { runtime: tokio::runtime::Runtime, client: reqwest::Client }
pub struct Response<'a> { pub length: Option<u64>, pub body: Box<dyn Read + 'a> }
pub trait Transport { fn get(&self, url: &str) -> Result<Response<'_>>; }
impl Http {
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new().map_err(|e| fail("IMAGE_NETWORK", e))?;
        let client = reqwest::Client::builder().user_agent("K11C-Installer/0.3")
            .connect_timeout(Duration::from_secs(8)).https_only(true)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() < 8 && allowed_url(attempt.url()) { attempt.follow() }
                else { attempt.error("Unexpected download redirect") }
            })).build().map_err(|e| fail("IMAGE_NETWORK", e))?;
        Ok(Self { runtime, client })
    }
}
struct HttpRead<'a> { runtime: &'a tokio::runtime::Runtime, response: reqwest::Response, pending: Vec<u8>, offset: usize }
impl Read for HttpRead<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() { return Ok(0); }
        while self.offset == self.pending.len() {
            let chunk = self.runtime.block_on(async { tokio::time::timeout(Duration::from_secs(15), self.response.chunk()).await })
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Download stalled for 15 seconds"))?
                .map_err(io::Error::other)?;
            match chunk { Some(bytes) => { self.pending = bytes.to_vec(); self.offset = 0; }, None => return Ok(0) }
        }
        let n = buf.len().min(self.pending.len() - self.offset);
        buf[..n].copy_from_slice(&self.pending[self.offset..self.offset+n]); self.offset += n; Ok(n)
    }
}
impl Transport for Http {
    fn get(&self, url: &str) -> Result<Response<'_>> {
        let parsed = reqwest::Url::parse(url).map_err(|e| fail("IMAGE_URL", e))?;
        if !allowed_url(&parsed) { return Err(fail("IMAGE_URL", "Only official HTTPS release hosts are allowed")); }
        let response = self.runtime.block_on(async { tokio::time::timeout(Duration::from_secs(30), self.client.get(parsed).send()).await })
            .map_err(|_| fail("IMAGE_NETWORK", "Request timed out"))?.map_err(|e| fail("IMAGE_NETWORK", e))?;
        if response.status() != reqwest::StatusCode::OK { return Err(fail(if response.status().as_u16() == 403 || response.status().as_u16() == 429 { "IMAGE_RATE_LIMIT" } else { "IMAGE_NETWORK" }, format!("HTTP {}", response.status()))); }
        Ok(Response { length: response.content_length(), body: Box::new(HttpRead { runtime: &self.runtime, response, pending: vec![], offset: 0 }) })
    }
}

pub(crate) fn json_from(net: &impl Transport, url: &str) -> Result<serde_json::Value> {
    let response = net.get(url)?; let mut bytes = Vec::new();
    response.body.take(MAX_JSON+1).read_to_end(&mut bytes).map_err(|e| fail("IMAGE_NETWORK", e))?;
    if bytes.len() as u64 > MAX_JSON { return Err(fail("IMAGE_RELEASE", "Release metadata too large")); }
    serde_json::from_slice(&bytes).map_err(|e| fail("IMAGE_RELEASE", e))
}
fn parse_release(v: &serde_json::Value) -> Result<Option<Release>> {
    if v["draft"] != false || v["prerelease"] != false { return Ok(None); }
    let version = v["tag_name"].as_str().unwrap_or(""); if !valid_version(version) { return Ok(None); }
    let filename = format!("haos_generic-aarch64-{version}.img.xz");
    let Some(asset) = v["assets"].as_array().and_then(|a| a.iter().find(|a| a["name"] == filename)) else { return Ok(None); };
    let size = asset["size"].as_u64().unwrap_or(0);
    if size == 0 || size > MAX_INPUT { return Err(fail("IMAGE_RELEASE", "Invalid release asset size")); }
    let sha256 = match asset["digest"].as_str() {
        None => None,
        Some(s) if s.starts_with("sha256:") && s.len() == 71 && s[7..].bytes().all(|b| b.is_ascii_hexdigit()) => Some(s[7..].to_ascii_lowercase()),
        _ => return Err(fail("IMAGE_RELEASE", "Invalid release digest")),
    };
    let release = Release { version: version.into(), filename, size, sha256, published: v["published_at"].as_str().unwrap_or("").into() };
    if asset["browser_download_url"].as_str() != Some(official_url(&release).as_str()) { return Err(fail("IMAGE_URL", "Release asset URL does not match its version")); }
    Ok(Some(release))
}
pub fn releases(net: &impl Transport) -> Result<Vec<Release>> {
    let v = json_from(net, &format!("{API}?per_page=100"))?;
    let mut list = Vec::new();
    for value in v.as_array().ok_or_else(|| fail("IMAGE_RELEASE", "Expected release list"))? { if let Some(r) = parse_release(value)? { list.push(r); } }
    list.sort_by_cached_key(|r| std::cmp::Reverse(r.version.split('.').map(|s| s.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>()));
    list.dedup_by(|a,b| a.version == b.version);
    if list.is_empty() { return Err(fail("IMAGE_RELEASE", "No stable generic-aarch64 images in the release list")); }
    Ok(list)
}
fn resolve(net: &impl Transport, version: &str) -> Result<Release> {
    if !valid_version(version) { return Err(fail("IMAGE_RELEASE", "Invalid stable release version")); }
    let release = parse_release(&json_from(net, &format!("{API}/tags/{version}"))?)?.ok_or_else(|| fail("IMAGE_RELEASE", "Stable generic-aarch64 asset missing"))?;
    if release.version != version { return Err(fail("IMAGE_RELEASE", "Version mismatch")); } Ok(release)
}

fn open_regular(path: &Path) -> Result<File> {
    let f = File::open(path).map_err(|e| fail("IMAGE_FILE", e))?;
    let m = f.metadata().map_err(|e| fail("IMAGE_FILE", e))?;
    if !m.is_file() || m.len() == 0 || m.len() > MAX_IMAGE { return Err(fail("IMAGE_FILE", "Expected a nonempty regular image file, at most 32 GiB")); } Ok(f)
}
pub(crate) fn file_hash(path: &Path, job: &mut Job<'_>, phase: &str) -> Result<(u64,String)> {
    let mut f = open_regular(path)?; let total = f.metadata().map_err(|e| fail("IMAGE_FILE", e))?.len();
    let mut hash = Sha256::new(); let mut bytes = 0; let mut buffer = vec![0;CHUNK];
    job.progress(phase,0,Some(total),true);
    loop { job.check()?; let n = f.read(&mut buffer).map_err(|e| fail("IMAGE_FILE", e))?; if n == 0 { break; } bytes += n as u64; if bytes > MAX_IMAGE { return Err(fail("IMAGE_SIZE", "Image exceeds 32 GiB")); } hash.update(&buffer[..n]); job.progress(phase,bytes,Some(total),false); }
    job.check()?; if bytes != total { return Err(fail("IMAGE_CHANGED", "Image length changed while reading")); }
    Ok((bytes,format!("{:x}",hash.finalize())))
}
pub(crate) struct Temporary { pub(crate) path: PathBuf }
impl Temporary {
    pub(crate) fn new(dir: &Path, extension: &str) -> Result<(Self,File)> {
        fs::create_dir_all(dir).map_err(|e| fail("IMAGE_IO", e))?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let path = dir.join(format!("work-{stamp}-{}.partial.{extension}",std::process::id()));
        let f = OpenOptions::new().create_new(true).write(true).open(&path).map_err(|e| fail("IMAGE_IO", e))?;
        Ok((Self { path },f))
    }
}
impl Drop for Temporary { fn drop(&mut self) { let _ = fs::remove_file(&self.path); } }

pub(crate) fn stream_copy(input: &mut dyn Read, output: &mut File, limit: u64, expected: Option<u64>, job: &mut Job<'_>, phase: &str) -> Result<(u64,String)> {
    let mut buffer = vec![0;CHUNK]; let mut total = 0u64; let mut hash = Sha256::new();
    job.progress(phase,0,expected,true);
    loop {
        job.check()?; let count = input.read(&mut buffer).map_err(|e| fail(if phase == "download" {"IMAGE_NETWORK"} else {"IMAGE_DECODE"},e))?;
        if count == 0 { break; }
        total = total.checked_add(count as u64).ok_or_else(|| fail("IMAGE_SIZE", "Size overflow"))?;
        if total > limit || expected.is_some_and(|n| total > n) { return Err(fail("IMAGE_SIZE", "Image exceeds expected size")); }
        output.write_all(&buffer[..count]).map_err(|e| fail("IMAGE_IO",e))?; hash.update(&buffer[..count]); job.progress(phase,total,expected,false);
    }
    job.check()?;
    if total == 0 || expected.is_some_and(|n| total != n) { return Err(fail("IMAGE_SIZE", "Incomplete image transfer")); }
    output.sync_all().map_err(|e| fail("IMAGE_IO",e))?;
    Ok((total,format!("{:x}",hash.finalize())))
}
pub(crate) fn publish(temp: &Temporary, target: &Path, length: u64, hash: &str, job: &mut Job<'_>) -> Result<bool> {
    job.check()?;
    if target.exists() {
        if file_hash(target,job,"verify-cache")? == (length,hash.into()) { return Ok(true); }
        // Only an app-managed cache destination; never the user's source file.
        fs::remove_file(target).map_err(|e| fail("IMAGE_IO",e))?;
    }
    fs::rename(&temp.path,target).map_err(|e| fail("IMAGE_IO",e))?; Ok(false)
}

pub fn download(net: &impl Transport, version: &str, base: &Path, job: &mut Job<'_>) -> Result<PreparedImage> {
    job.check()?; job.progress("resolve",0,None,true); let release = resolve(net,version)?; job.check()?;
    let expected = release.sha256.as_deref().ok_or_else(|| fail("IMAGE_NO_DIGEST", "This release does not provide an official SHA256 digest"))?;
    let dir = base.join("data/images/downloads"); fs::create_dir_all(&dir).map_err(|e| fail("IMAGE_IO",e))?;
    let target = dir.join(&release.filename);
    let reused = if target.exists() && release.sha256.is_some() {
        let (length,hash) = file_hash(&target,job,"verify-download")?;
        length == release.size && Some(hash) == release.sha256
    } else { false };
    if !reused {
        let (temp,mut output) = Temporary::new(&dir,"xz")?;
        let mut response = net.get(&official_url(&release))?;
        if response.length.is_some_and(|n| n != release.size) { return Err(fail("IMAGE_SIZE", "Content-Length differs from release metadata")); }
        let (length,hash) = stream_copy(&mut response.body,&mut output,MAX_INPUT,Some(release.size),job,"download")?;
        drop(output);
        if release.sha256.as_ref().is_some_and(|s| *s != hash) { return Err(fail("IMAGE_HASH", "Official SHA256 mismatch")); }
        publish(&temp,&target,length,&hash,job)?;
    }
    let mut prepared = prepare(&target,base,job,Some(expected))?;
    prepared.version = Some(release.version); prepared.verification = "github_sha256".into(); prepared.reused_download = reused;
    Ok(prepared)
}

struct HashRead<R> { inner: R, hash: Sha256, bytes: u64 }
impl<R: Read> Read for HashRead<R> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> { let n = self.inner.read(b)?; self.hash.update(&b[..n]); self.bytes += n as u64; Ok(n) }
}
pub fn import(source: &Path, base: &Path, job: &mut Job<'_>) -> Result<PreparedImage> {
    prepare(source,base,job,None)
}
fn prepare(source: &Path, base: &Path, job: &mut Job<'_>, expected: Option<&str>) -> Result<PreparedImage> {
    job.check()?;
    let filename = source.file_name().and_then(|s| s.to_str()).ok_or_else(|| fail("IMAGE_FILE", "Missing file name"))?.to_owned();
    let name = filename.to_ascii_lowercase();
    if !name.ends_with(".img") && !name.ends_with(".img.xz") { return Err(fail("IMAGE_FORMAT", "Choose a .img or .img.xz file")); }
    let file = open_regular(source)?; let source_bytes = file.metadata().map_err(|e| fail("IMAGE_FILE",e))?.len();
    let mut reader = HashRead { inner: BufReader::new(file), hash: Sha256::new(), bytes: 0 };
    let dir = base.join("data/images/prepared"); let (temp,mut output) = Temporary::new(&dir,"img")?;
    let (length,hash) = if name.ends_with(".xz") {
        let stream = xz2::stream::Stream::new_stream_decoder(256*1024*1024,xz2::stream::CONCATENATED).map_err(|e| fail("IMAGE_DECODE",e))?;
        let mut decoder = xz2::read::XzDecoder::new_stream(&mut reader,stream);
        stream_copy(&mut decoder,&mut output,MAX_IMAGE,None,job,"expand")?
    } else { stream_copy(&mut reader,&mut output,MAX_IMAGE,Some(source_bytes),job,"copy")? };
    drop(output);
    if reader.bytes != source_bytes { return Err(fail("IMAGE_CHANGED", "Source changed or compressed stream is incomplete")); }
    let source_sha256 = format!("{:x}",reader.hash.finalize());
    if expected.is_some_and(|s| s != source_sha256) { return Err(fail("IMAGE_HASH", "Source changed after download verification")); }
    job.progress("inspect",0,None,true); inspect(&temp.path,job.cancel)?; job.check()?;
    // Verify the saved bytes, not just the buffers handed to the file writer.
    if file_hash(&temp.path,job,"verify-image")? != (length,hash.clone()) { return Err(fail("IMAGE_HASH", "Saved image differs from decoded stream")); }
    let target = dir.join(format!("{hash}.img")); let reused_image = publish(&temp,&target,length,&hash,job)?;
    job.progress("ready",length,Some(length),true);
    Ok(PreparedImage { path: target, filename, version: None, platform: "generic-aarch64".into(), bytes: length, sha256: hash, source_sha256, verification: "local_structure".into(), partitions: 8, reused_download: false, reused_image })
}

fn read_at(f: &mut File, offset: u64, size: usize) -> Result<Vec<u8>> { f.seek(SeekFrom::Start(offset)).map_err(|e| fail("IMAGE_FILE",e))?; let mut b=vec![0;size]; f.read_exact(&mut b).map_err(|e| fail("IMAGE_FILE",e))?; Ok(b) }
fn n64(b: &[u8], o: usize) -> u64 { u64::from_le_bytes(b[o..o+8].try_into().unwrap()) }
// Read-only partition view with bounded seeks and an I/O budget. Malformed FAT
// chains cannot run forever or escape the boot partition. It cannot write.
struct Partition<'a> { file: File, start: u64, length: u64, position: u64, budget: u32, cancel: &'a AtomicBool }
impl Partition<'_> {
    fn check(&mut self) -> io::Result<()> { if self.cancel.load(Ordering::Relaxed) { return Err(io::Error::other("Cancelled")); } self.budget = self.budget.checked_sub(1).ok_or_else(|| io::Error::other("FAT inspection budget exceeded"))?; Ok(()) }
}
impl Read for Partition<'_> {
    fn read(&mut self,b:&mut[u8])->io::Result<usize> { self.check()?; let len=b.len().min((self.length-self.position) as usize); self.file.seek(SeekFrom::Start(self.start+self.position))?; let n=self.file.read(&mut b[..len])?;self.position+=n as u64;Ok(n) }
}
impl Seek for Partition<'_> {
    fn seek(&mut self,from:SeekFrom)->io::Result<u64> { self.check()?; let n=match from { SeekFrom::Start(n)=>n as i128,SeekFrom::Current(n)=>self.position as i128+n as i128,SeekFrom::End(n)=>self.length as i128+n as i128 };if n<0||n>self.length as i128{return Err(io::Error::other("Seek outside boot partition"));}self.position=n as u64;Ok(self.position) }
}
impl Write for Partition<'_> { fn write(&mut self,_:&[u8])->io::Result<usize>{Err(io::Error::new(io::ErrorKind::PermissionDenied,"Read-only image"))}fn flush(&mut self)->io::Result<()>{Ok(())} }
pub fn inspect(path: &Path, cancel: &AtomicBool) -> Result<()> {
    let mut file = open_regular(path)?; let bytes = file.metadata().map_err(|e| fail("IMAGE_FILE",e))?.len();
    if bytes % 512 != 0 || bytes / 512 <= 34849 || bytes > MAX_IMAGE { return Err(fail("IMAGE_FORMAT", "Invalid raw image size")); }
    let primary = read_at(&mut file,0,34*512)?; let tail = read_at(&mut file,bytes-33*512,33*512)?;
    let gpt = crate::gpt::check(&primary,&tail,(bytes/512) as u32);
    if gpt["healthy"] != true || gpt["partitions"] != 8 { return Err(fail("IMAGE_GPT", gpt)); }
    for (i,label) in LABELS.iter().enumerate() {
        let e=&primary[1024+i*128..1024+(i+1)*128];
        let chars:Vec<u16>=e[56..128].chunks_exact(2).map(|b|u16::from_le_bytes([b[0],b[1]])).take_while(|n|*n!=0).collect();
        if String::from_utf16(&chars).ok().as_deref()!=Some(label) || e[..16].iter().all(|b|*b==0) || e[16..32].iter().all(|b|*b==0) { return Err(fail("IMAGE_PLATFORM", "Not the expected HAOS partition layout")); }
    }
    // EFI System Partition type GUID on disk, not a file-name/platform guess.
    if primary[1024..1040] != [0x28,0x73,0x2a,0xc1,0x1f,0xf8,0xd2,0x11,0xba,0x4b,0,0xa0,0xc9,0x3e,0xc9,0x3b] { return Err(fail("IMAGE_PLATFORM", "Missing EFI system partition")); }
    let start=n64(&primary,1024+32)*512;let length=(n64(&primary,1024+40)+1)*512-start;
    let view=Partition{file,start,length,position:0,budget:100_000,cancel};
    let fs=fatfs::FileSystem::new(view,fatfs::FsOptions::new()).map_err(|e|fail("IMAGE_BOOT",e))?;
    let mut boot=fs.root_dir().open_file("EFI/BOOT/BOOTAA64.EFI").map_err(|e|fail("IMAGE_PLATFORM",format!("ARM64 EFI boot file missing: {e}")))?;
    let mut dos=[0u8;64];boot.read_exact(&mut dos).map_err(|e|fail("IMAGE_BOOT",e))?;
    let pe=u32::from_le_bytes(dos[60..64].try_into().unwrap());
    if &dos[..2]!=b"MZ" || pe>1024*1024 { return Err(fail("IMAGE_BOOT","Invalid EFI executable")); }
    boot.seek(SeekFrom::Start(pe as u64)).map_err(|e|fail("IMAGE_BOOT",e))?;let mut header=[0u8;6];boot.read_exact(&mut header).map_err(|e|fail("IMAGE_BOOT",e))?;
    if &header[..4]!=b"PE\0\0" || u16::from_le_bytes([header[4],header[5]])!=0xaa64 { return Err(fail("IMAGE_PLATFORM","EFI executable is not ARM64")); }
    if cancel.load(Ordering::Relaxed) { return Err(fail("IMAGE_CANCELLED","Image operation cancelled")); } Ok(())
}

#[cfg(test)] pub(crate) mod tests;
