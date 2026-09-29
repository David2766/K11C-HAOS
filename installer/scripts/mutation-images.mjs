import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');process.chdir(root);
import {toolchain} from './toolchain.mjs';
const {cargo,env}=toolchain();
const file='engine/src/images.rs', original=readFileSync(file,'utf8');
const cases=[
 ['digest mismatch ignored','if release.sha256.as_ref().is_some_and(|s| *s != hash)','if false','official_digest_is_mandatory'],
 ['digest mismatch reversed','if release.sha256.as_ref().is_some_and(|s| *s != hash)','if release.sha256.as_ref().is_some_and(|s| *s == hash)','metadata_is_dynamic'],
 ['source verification skipped','if expected.is_some_and(|s| s != source_sha256)','if false','saved_bytes_and_verified_source'],
 ['cache reused without hash','length == release.size && Some(hash) == release.sha256','true','metadata_is_dynamic'],
 ['saved hash skipped','if file_hash(&temp.path,job,"verify-image")? != (length,hash.clone())','if false','saved_bytes_and_verified_source'],
 ['GPT check skipped','if gpt["healthy"] != true || gpt["partitions"] != 8','if false','rejects_wrong_layout'],
 ['inspection skipped','inspect(&temp.path,job.cancel)?;','/* inspection removed */','rejects_wrong_layout'],
 ['ARM64 check removed',' || u16::from_le_bytes([header[4],header[5]])!=0xaa64','','rejects_wrong_layout'],
 ['partition names ignored','String::from_utf16(&chars).ok().as_deref()!=Some(label)','false','rejects_wrong_layout'],
 ['transfer bounds removed','if total > limit || expected.is_some_and(|n| total > n)','if false','bounded_copy'],
 ['short transfer accepted','if total == 0 || expected.is_some_and(|n| total != n)','if false','bounded_copy'],
 ['cancellation ignored','pub fn check(&self) -> Result<()> { if self.cancel.load(Ordering::Relaxed)','pub fn check(&self) -> Result<()> { if false','bounded_copy'],
 ['release URL ignored','if asset["browser_download_url"].as_str() != Some(official_url(&release).as_str())','if false','metadata_url'],
 ['TLS scheme not enforced','url.scheme() == "https"','true','metadata_url'],
];
mkdirSync('test-results',{recursive:true});const results=[];
function test(filter='images::tests') { return spawnSync(cargo,['test','--locked','-p','k11c-usb','--lib',filter],{env,encoding:'utf8',timeout:120000,maxBuffer:8*1024*1024}); }
assert.equal(test().status,0,'baseline');
try {
 for(const [name,before,after,filter] of cases) {
  assert.equal(original.split(before).length-1,1,`unique anchor: ${name}`);
  writeFileSync(file,original.replace(before,after));
  const result=test('images::tests::'+filter);
  const killed=result.status!==0&&/test result: FAILED/.test(result.stdout)&&!/could not compile/.test(result.stderr);
  results.push({name,killed});console.log(`${killed?'KILLED':'FAILED'}: ${name}`);
  if(!killed)throw Error(result.stdout+result.stderr);
 }
}finally {writeFileSync(file,original);}
assert.equal(test().status,0,'restored baseline');assert.equal(readFileSync(file,'utf8'),original);
writeFileSync('test-results/image-mutations.json',JSON.stringify({results,restored_baseline:true},null,2));
