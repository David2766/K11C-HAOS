use super::*;
use std::{cell::RefCell, collections::HashMap, io::Cursor, sync::OnceLock};

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let p=std::env::temp_dir().join(format!("k11c-image-test-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir(&p).unwrap(); Self(p)
    }
    fn file(&self,name:&str,bytes:&[u8])->PathBuf { let p=self.0.join(name);fs::write(&p,bytes).unwrap();p }
}
impl Drop for Sandbox { fn drop(&mut self) { fs::remove_dir_all(&self.0).unwrap(); } }
fn crc(b:&[u8])->u32 { let mut c=!0u32; for x in b {c^=*x as u32;for _ in 0..8 { c=(c>>1)^if c&1!=0 {0xedb88320} else {0};}} !c }
fn put64(b:&mut[u8],o:usize,n:u64){b[o..o+8].copy_from_slice(&n.to_le_bytes());}
fn put32(b:&mut[u8],o:usize,n:u32){b[o..o+4].copy_from_slice(&n.to_le_bytes());}
fn seal(image:&mut[u8]) {
    let len=image.len(); let entries=image[1024..34*512].to_vec();
    image[len-33*512..len-512].copy_from_slice(&entries);
    let ec=crc(&entries);
    for off in [512,len-512] { let h=&mut image[off..off+512];put32(h,88,ec);put32(h,16,0);let hc=crc(&h[..92]);put32(h,16,hc); }
}
pub(crate) fn fixture()-> &'static Vec<u8> {
    static IMAGE:OnceLock<Vec<u8>>=OnceLock::new();
    IMAGE.get_or_init(|| {
        let sectors=36864u64;let mut image=vec![0;sectors as usize*512];let len=image.len();
        image[510..512].copy_from_slice(&[0x55,0xaa]);image[450]=0xee;
        put32(&mut image,454,1);put32(&mut image,458,sectors as u32-1);
        for (i,label) in LABELS.iter().enumerate() {
            let e=&mut image[1024+i*128..1024+(i+1)*128]; e[0]=1;e[16]=i as u8+1;
            let start=2048+i as u64*4096;put64(e,32,start);put64(e,40,start+4095);
            for (j,ch) in label.encode_utf16().enumerate() { e[56+j*2..58+j*2].copy_from_slice(&ch.to_le_bytes()); }
        }
        image[1024..1040].copy_from_slice(&[0x28,0x73,0x2a,0xc1,0x1f,0xf8,0xd2,0x11,0xba,0x4b,0,0xa0,0xc9,0x3e,0xc9,0x3b]);
        for (off,current,other,entries) in [(512,1,sectors-1,2),(len-512,sectors-1,1,sectors-33)] {
            let h=&mut image[off..off+512];h[..8].copy_from_slice(b"EFI PART");put32(h,8,0x10000);put32(h,12,92);
            put64(h,24,current);put64(h,32,other);put64(h,40,34);put64(h,48,sectors-34);h[56]=77;
            put64(h,72,entries);put32(h,80,128);put32(h,84,128);
        }
        let mut fat=Cursor::new(vec![0u8;4096*512]);fatfs::format_volume(&mut fat,fatfs::FormatVolumeOptions::new()).unwrap();
        {
            let disk=fatfs::FileSystem::new(&mut fat,fatfs::FsOptions::new()).unwrap();
            let efi=disk.root_dir().create_dir("EFI").unwrap();let boot=efi.create_dir("BOOT").unwrap();
            let mut exe=vec![0;256];exe[..2].copy_from_slice(b"MZ");put32(&mut exe,60,128);exe[128..134].copy_from_slice(&[b'P',b'E',0,0,0x64,0xaa]);
            boot.create_file("BOOTAA64.EFI").unwrap().write_all(&exe).unwrap();
        }
        image[2048*512..6144*512].copy_from_slice(fat.get_ref());seal(&mut image);image
    })
}
fn compressed()-> &'static Vec<u8> {
    static XZ:OnceLock<Vec<u8>>=OnceLock::new();
    XZ.get_or_init(|| {let mut e=xz2::write::XzEncoder::new(Vec::new(),1);e.write_all(fixture()).unwrap();e.finish().unwrap()})
}
fn run<T>(f:impl FnOnce(&mut Job<'_>)->T)->T {let c=AtomicBool::new(false);let mut n=|_|{};f(&mut Job::new(&c,&mut n))}
fn error<T:std::fmt::Debug>(r:Result<T>,code:&str){assert_eq!(r.unwrap_err().code,code);}
fn partials(dir:&Path)->usize {
    if !dir.exists(){return 0;}
    fs::read_dir(dir).unwrap().map(|p|p.unwrap().path()).map(|p|if p.is_dir(){partials(&p)}else{usize::from(p.to_string_lossy().contains(".partial."))}).sum()
}
fn metadata(version:&str,digest:Option<&str>,size:usize)->serde_json::Value {
    serde_json::json!({"tag_name":version,"draft":false,"prerelease":false,"published_at":"2026-01-01T00:00:00Z","assets":[{"name":format!("haos_generic-aarch64-{version}.img.xz"),"size":size,"digest":digest.map(|d|format!("sha256:{d}")),"browser_download_url":format!("{DOWNLOAD}/{version}/haos_generic-aarch64-{version}.img.xz")}]})
}
struct Fake { responses:HashMap<String,Vec<u8>>, calls:RefCell<Vec<String>> }
impl Fake {
    fn new(version:&str)->Self {
        let bytes=compressed().clone();let m=metadata(version,Some(&crate::hash_bytes(&bytes)),bytes.len());
        Self{responses:HashMap::from([(format!("{API}/tags/{version}"),serde_json::to_vec(&m).unwrap()),(format!("{API}?per_page=100"),serde_json::to_vec(&vec![m]).unwrap()),(format!("{DOWNLOAD}/{version}/haos_generic-aarch64-{version}.img.xz"),bytes)]),calls:RefCell::new(vec![])}
    }
    fn set_meta(&mut self,v:serde_json::Value){self.responses.insert(format!("{API}/tags/99.7"),serde_json::to_vec(&v).unwrap());}
}
impl Transport for Fake {
    fn get(&self,url:&str)->Result<Response<'_>> { self.calls.borrow_mut().push(url.into());let b=self.responses.get(url).ok_or_else(||fail("TEST_URL",url))?;Ok(Response{length:Some(b.len() as u64),body:Box::new(Cursor::new(b))}) }
}

#[test] fn raw_and_xz_share_saved_image_validation() {
    let dir=Sandbox::new();let raw=dir.file("한글 HAOS.img",fixture());let xz=dir.file("haos.img.xz",compressed());
    let a=run(|j|import(&raw,&dir.0,j)).unwrap();let b=run(|j|import(&xz,&dir.0,j)).unwrap();
    assert_eq!(a.sha256,crate::hash_bytes(fixture()));assert_eq!(a.path,b.path);assert!(b.reused_image);assert_eq!(a.verification,"local_structure");
    assert_eq!(fs::read(&raw).unwrap(),*fixture());assert_eq!(fs::read(&xz).unwrap(),*compressed());assert_eq!(partials(&dir.0),0);
}
#[test] fn metadata_is_dynamic_and_cache_is_rehashed() {
    let dir=Sandbox::new();let net=Fake::new("99.7");
    let a=run(|j|download(&net,"99.7",&dir.0,j)).unwrap();assert_eq!(a.version.as_deref(),Some("99.7"));assert_eq!(a.verification,"github_sha256");assert!(!a.reused_download);
    let b=run(|j|download(&net,"99.7",&dir.0,j)).unwrap();assert!(b.reused_download);assert!(b.reused_image);
    let asset_calls=||net.calls.borrow().iter().filter(|s|s.starts_with(DOWNLOAD)).count();assert_eq!(asset_calls(),1);
    let cached=dir.0.join("data/images/downloads/haos_generic-aarch64-99.7.img.xz");fs::write(cached,b"corrupted").unwrap();
    assert!(!run(|j|download(&net,"99.7",&dir.0,j)).unwrap().reused_download);assert_eq!(asset_calls(),2);
}
#[test] fn official_digest_is_mandatory_and_reloaded_per_version() {
    let dir=Sandbox::new();let mut net=Fake::new("99.7");
    net.set_meta(metadata("99.7",None,compressed().len()));error(run(|j|download(&net,"99.7",&dir.0,j)),"IMAGE_NO_DIGEST");
    net.set_meta(metadata("99.7",Some(&"0".repeat(64)),compressed().len()));error(run(|j|download(&net,"99.7",&dir.0,j)),"IMAGE_HASH");
    assert!(!dir.0.join("data/images/downloads/haos_generic-aarch64-99.7.img.xz").exists());assert_eq!(partials(&dir.0),0);
}
#[test] fn rejects_wrong_layout_crc_and_architecture() {
    let dir=Sandbox::new();let c=AtomicBool::new(false);
    let mut b=fixture().clone();b[512+16]^=1;error(inspect(&dir.file("crc.img",&b),&c),"IMAGE_GPT");
    let mut b=fixture().clone();b[1024+56]=b'x';seal(&mut b);error(inspect(&dir.file("layout.img",&b),&c),"IMAGE_PLATFORM");
    let mut b=fixture().clone();let pe=b.windows(6).position(|b|b==[b'P',b'E',0,0,0x64,0xaa]).unwrap();b[pe+4..pe+6].copy_from_slice(&[0x64,0x86]);
    error(inspect(&dir.file("amd64.img",&b),&c),"IMAGE_PLATFORM");
    let b=vec![0;fixture().len()];error(run(|j|import(&dir.file("bad.img",&b),&dir.0,j)),"IMAGE_GPT");assert_eq!(partials(&dir.0),0);
}
#[test] fn rejects_incomplete_compression_and_cancellation() {
    let dir=Sandbox::new();let bad=dir.file("truncated.img.xz",&compressed()[..compressed().len()-10]);assert!(run(|j|import(&bad,&dir.0,j)).is_err());
    let raw=dir.file("valid.img",fixture());let cancel=AtomicBool::new(false);let mut notify=|p:Progress|if p.phase=="copy"{cancel.store(true,Ordering::Relaxed)};
    error(import(&raw,&dir.0,&mut Job::new(&cancel,&mut notify)),"IMAGE_CANCELLED");assert_eq!(partials(&dir.0),0);
}
#[test] fn bounded_copy_rejects_short_and_excess_transfers() {
    let dir=Sandbox::new();let mut out=File::create(dir.0.join("output")).unwrap();
    for (limit,expected) in [(2,None),(100,Some(4)),(100,Some(2))] {error(run(|j|stream_copy(&mut Cursor::new(b"abc"),&mut out,limit,expected,j,"download")),"IMAGE_SIZE");}
    let c=AtomicBool::new(true);let mut notify=|_|{};
    error(stream_copy(&mut Cursor::new(b"abc"),&mut out,100,None,&mut Job::new(&c,&mut notify),"download"),"IMAGE_CANCELLED");
}
#[test] fn saved_bytes_and_verified_source_must_match() {
    let dir=Sandbox::new();let raw=dir.file("valid.img",fixture());let c=AtomicBool::new(false);let mut changed=false;
    let mut notify=|p:Progress|if p.phase=="verify-image"&&!changed {
        changed=true;let path=fs::read_dir(dir.0.join("data/images/prepared")).unwrap().next().unwrap().unwrap().path();
        let mut f=OpenOptions::new().write(true).open(path).unwrap();f.seek(SeekFrom::Start(50000)).unwrap();f.write_all(b"changed").unwrap();
    };
    error(import(&raw,&dir.0,&mut Job::new(&c,&mut notify)),"IMAGE_HASH");assert!(changed);
    error(run(|j|prepare(&raw,&dir.0,j,Some(&"0".repeat(64)))),"IMAGE_HASH");assert_eq!(partials(&dir.0),0);
}
#[test] fn metadata_url_and_release_filters() {
    let mut net=Fake::new("99.7");let base=metadata("99.7",Some(&"1".repeat(64)),42);
    let mut rc=base.clone();rc["prerelease"]=true.into();let mut draft=base.clone();draft["draft"]=true.into();
    net.responses.insert(format!("{API}?per_page=100"),serde_json::to_vec(&vec![rc,draft,base.clone()]).unwrap());assert_eq!(releases(&net).unwrap().len(),1);
    let mut bad=base.clone();bad["assets"][0]["browser_download_url"]=serde_json::json!("https://evil.invalid/image.xz");error(parse_release(&bad),"IMAGE_URL");
    let mut bad=base;bad["assets"][0]["digest"]=serde_json::json!("sha256:no");error(parse_release(&bad),"IMAGE_RELEASE");
    for url in ["http://github.com/x","http://github.com:443/x","https://localhost/x","https://github.com.evil.invalid/x","https://github.com:444/x","https://user@github.com/x"] {assert!(!allowed_url(&reqwest::Url::parse(url).unwrap()));}
    assert!(allowed_url(&reqwest::Url::parse("https://release-assets.githubusercontent.com/x").unwrap()));
    for v in ["../18.3","18.3/other","18.3-rc1",""] {assert!(!valid_version(v));}
}
