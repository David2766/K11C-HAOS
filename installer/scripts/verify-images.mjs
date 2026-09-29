import {spawn} from 'node:child_process';
import {readFileSync,writeFileSync,createReadStream,mkdirSync} from 'node:fs';
import {createHash} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');process.chdir(root);
const release=path.join(root,'release/K11C-Installer-'+JSON.parse(readFileSync('package.json','utf8')).version);
const exe=path.join(release,'k11c-usb.exe');
async function run(args) {
 return await new Promise((resolve,reject)=>{
  const p=spawn(exe,args,{windowsHide:true,timeout:600000});let out='',err='',phase='';
  p.stdout.on('data',b=>out+=b);p.stderr.on('data',b=>{
   err+=b;const lines=err.split('\n');err=lines.pop();for(const line of lines){try{const v=JSON.parse(line);if(v.phase!==phase){phase=v.phase;console.log(`${args[0]}: ${phase}`);}}catch{}}
  });
  p.on('error',reject);p.on('close',code=>{try{const v=JSON.parse(out);assert.equal(code,0,JSON.stringify(v));assert.equal(v.ok,true);resolve(v.data);}catch(e){reject(e);}});
 });
}
async function digest(p){const h=createHash('sha256');for await(const b of createReadStream(p))h.update(b);return h.digest('hex');}
const rows=await run(['image-releases']);const latest=rows.find(r=>r.sha256);assert.ok(latest);
console.log(`OFFICIAL_RELEASE=${latest.version}; digest read from GitHub metadata`);
const downloaded=await run(['image-download',latest.version]);assert.equal(downloaded.source_sha256,latest.sha256);assert.equal(downloaded.verification,'github_sha256');
assert.equal(await digest(downloaded.path),downloaded.sha256);
const reused=await run(['image-download',latest.version]);assert.equal(reused.reused_download,true);assert.equal(reused.reused_image,true);assert.equal(reused.sha256,downloaded.sha256);
const imported=await run(['image-import',downloaded.path]);assert.equal(imported.verification,'local_structure');assert.equal(imported.sha256,downloaded.sha256);assert.equal(imported.reused_image,true);
mkdirSync('test-results',{recursive:true});writeFileSync('test-results/image-live-verification.json',JSON.stringify({passed:true,packaged_cli:true,latest_from_repository:latest,downloaded,reused,imported},null,2));
console.log('IMAGE_LIVE_PASS: official metadata, download digest, decompression, GPT/ARM64, reuse, local import');
