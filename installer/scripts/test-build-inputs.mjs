import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,copyFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {buildInputs,vendorInputs} from './build-inputs.mjs';
import {nativeInputs} from './build-native.mjs';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const verified=buildInputs(root);
const vendor=vendorInputs(root);
const native=nativeInputs(root);
const temp=mkdtempSync(path.join(tmpdir(),'k11c-build-inputs-'));
try{
 const base=path.join(temp,'resources');
 for(const item of verified.files){const p=path.join(base,item.name);mkdirSync(path.dirname(p),{recursive:true});copyFileSync(item.path,p);}
 copyFileSync(verified.manifestPath,path.join(base,'firmware/manifest.txt'));
 for(const item of vendor.files){const p=path.join(base,'rockchip',item.name);mkdirSync(path.dirname(p),{recursive:true});copyFileSync(item.path,p);}
 copyFileSync(vendor.revisionPath,path.join(base,'rockchip/revision.txt'));
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
 function vendorContract(resolve){
  assert.equal(resolve(root,env).files.length,2);
  assert.equal(resolve(root,env).revisionPath,path.join(base,'rockchip/revision.txt'));
  for(const item of vendor.files){
   const p=path.join(base,'rockchip',item.name), saved=readFileSync(p), bad=Buffer.from(saved);bad[0]^=1;
   writeFileSync(p,bad);
   try{assert.throws(()=>resolve(root,env),/checksum mismatch/);}finally{writeFileSync(p,saved);}
   rmSync(p);
   try{assert.throws(()=>resolve(root,env),/ENOENT/);}finally{writeFileSync(p,saved);}
  }
 }
 vendorContract(vendorInputs);
 // A source ZIP has no HAOS image, seed or portable release. Its own resources
 // must suffice with an empty environment, even when the cwd is elsewhere.
 const standalone=path.join(temp,'standalone');
 mkdirSync(path.join(standalone,'engine/src'),{recursive:true});
 for(const name of ['loader.rs','flash.rs','driver.rs','factory.rs'])copyFileSync(path.join(root,'engine/src',name),path.join(standalone,'engine/src',name));
 for(const item of verified.files){const p=path.join(standalone,'resources',item.name);mkdirSync(path.dirname(p),{recursive:true});copyFileSync(item.path,p);}
 mkdirSync(path.join(standalone,'resources/rockchip'),{recursive:true});
 for(const item of vendor.files)copyFileSync(item.path,path.join(standalone,'resources/rockchip',item.name));
 copyFileSync(vendor.revisionPath,path.join(standalone,'resources/rockchip/revision.txt'));
 copyFileSync(verified.manifestPath,path.join(standalone,'resources/firmware/manifest.txt'));
 assert.equal(buildInputs(standalone,{}).files.length,5);
 assert.equal(vendorInputs(standalone,{}).files.length,2);
 mkdirSync(path.join(standalone,'resources/native'),{recursive:true});mkdirSync(path.join(standalone,'native'),{recursive:true});
 copyFileSync(path.join(root,'native/tools.sha256'),path.join(standalone,'native/tools.sha256'));
 copyFileSync(native.manifestPath,path.join(standalone,'resources/native/tools.json'));
 for(const item of native.files)copyFileSync(item.path,path.join(standalone,'resources/native',item.name));
 assert.equal(nativeInputs(standalone,{}).files.length,native.files.length);
 const tool=path.join(standalone,'resources/native/debugfs.exe'),originalTool=readFileSync(tool);writeFileSync(tool,Buffer.from('changed'));
 try{assert.throws(()=>nativeInputs(standalone,{}),/Native input changed/);}finally{writeFileSync(tool,originalTool);}
 const build=readFileSync(path.join(root,'scripts/build.mjs'),'utf8');
 assert.doesNotMatch(build,/componentInputs|componentCatalog|componentRoot|resources\/components[^\n]*cpSync/);
 assert.match(build,/install_components:'download-on-install'/);
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
 for(const [name,old,replacement] of [
  ['remove vendor checksum','if(!expected||actual!==expected)','if(false)'],
  ['reverse vendor checksum','if(!expected||actual!==expected)','if(!expected||actual===expected)'],
  ['ignore resource base for vendor',"path.join(base,'rockchip',name)","path.join(root,'resources/rockchip',name)"],
 ]){
  assert.ok(code.includes(old));
  const mod=await import('data:text/javascript;base64,'+Buffer.from(code.replace(old,replacement)).toString('base64'));
  assert.throws(()=>vendorContract(mod.vendorInputs),'Mutation survived: '+name);
  console.log('MUTATION_DETECTED '+name);
 }
 console.log('BUILD_INPUT_CONTRACT_PASS files=7 standalone_source=true mutations='+(mutations.length+3));
}finally{
 // Only the directory returned by mkdtempSync, never a caller-provided path.
 assert.ok(path.dirname(temp)===tmpdir()&&path.basename(temp).startsWith('k11c-build-inputs-'));
 rmSync(temp,{recursive:true,force:true});
}
