import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,copyFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {buildInputs} from './build-inputs.mjs';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const verified=buildInputs(root);
const temp=mkdtempSync(path.join(tmpdir(),'k11c-build-inputs-'));
try{
 const base=path.join(temp,'resources');
 for(const item of verified.files){const p=path.join(base,item.name);mkdirSync(path.dirname(p),{recursive:true});copyFileSync(item.path,p);}
 copyFileSync(verified.manifestPath,path.join(base,'firmware/manifest.txt'));
 const env={K11C_RELEASE_INPUTS:base};
 function contract(resolve){
  assert.equal(resolve(root,env).files.length,5);
  for(const item of verified.files){
   const p=path.join(base,item.name), data=readFileSync(p), bad=Buffer.from(data);bad[0]^=1;
   writeFileSync(p,bad);
   try{assert.throws(()=>resolve(root,env),/checksum mismatch/);}finally{writeFileSync(p,data);}
  }
  const p=path.join(base,'firmware/manifest.txt'), data=readFileSync(p);
  writeFileSync(p,'uboot.sha256='+'0'.repeat(64)+'\n');
  try{assert.throws(()=>resolve(root,env),/manifest mismatch/);}finally{writeFileSync(p,data);}
  const driver=path.join(base,'rockusb/rockusb.sys'), saved=readFileSync(driver);
  rmSync(driver);
  try{assert.throws(()=>resolve(root,env),/ENOENT/);}finally{writeFileSync(driver,saved);}
 }
 contract(buildInputs);
 const code=readFileSync(path.join(root,'scripts/build-inputs.mjs'),'utf8');
 const mutations=[
  ['remove input checksum','if(actual!==expected)','if(false)'],
  ['reverse input checksum','if(actual!==expected)','if(actual===expected)'],
  ['remove manifest verification',"if(!manifest.split(/\\r?\\n/).includes('uboot.sha256='+literal(flashSource,'FIRMWARE_SHA256')))",'if(false)'],
  ['omit driver input',"...driverSource.matchAll(/\\(\"(rockusb\\.(?:inf|cat|sys))\",\"([a-f0-9]{64})\"\\)/g)","...driverSource.matchAll(/\\(\"(rockusb\\.(?:inf|cat))\",\"([a-f0-9]{64})\"\\)/g)"]
 ];
 for(const [name,old,replacement] of mutations){
  assert.ok(code.includes(old),'Mutation anchor missing: '+name);
  const mod=await import('data:text/javascript;base64,'+Buffer.from(code.replace(old,replacement)).toString('base64'));
  let caught=false;try{contract(mod.buildInputs);}catch{caught=true;}
  assert.ok(caught,'Mutation survived: '+name);console.log('MUTATION_DETECTED '+name);
 }
 console.log('BUILD_INPUT_CONTRACT_PASS files=5 mutations='+mutations.length);
}finally{
 // Only the directory returned by mkdtempSync, never a caller-provided path.
 assert.ok(path.dirname(temp)===tmpdir()&&path.basename(temp).startsWith('k11c-build-inputs-'));
 rmSync(temp,{recursive:true,force:true});
}
