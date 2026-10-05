import {readFileSync,realpathSync} from 'node:fs';
import {createHash} from 'node:crypto';
import path from 'node:path';

export function vendorInputs(root,env=process.env){
 const base=path.resolve(env.K11C_RELEASE_INPUTS??path.join(root,'resources'));
 const engine=readFileSync(path.join(root,'engine/src/factory.rs'),'utf8');
 const files=[['upgrade_tool.exe','TOOL_SHA256'],['config.ini','CONFIG_SHA256']].map(([name,key])=>{
  const expected=engine.match(new RegExp(`const ${key}:&str="([a-f0-9]{64})"`))?.[1];
  const file=path.join(base,'rockchip',name), actual=createHash('sha256').update(readFileSync(file)).digest('hex');
  if(!expected||actual!==expected)throw Error('Vendor tool checksum mismatch: '+name);
  return {name,path:realpathSync(file),sha256:actual};
 });
 const revisionPath=realpathSync(path.join(base,'rockchip/revision.txt'));
 return {files,revisionPath};
}

// Build-only resource resolution. Runtime trust still comes from the engine's
// compiled hashes. Accept no different firmware/driver through a build override.
export function buildInputs(root,env=process.env){
 const base=path.resolve(env.K11C_RELEASE_INPUTS??path.join(root,'resources'));
 const loaderSource=readFileSync(path.join(root,'engine/src/loader.rs'),'utf8');
 const flashSource=readFileSync(path.join(root,'engine/src/flash.rs'),'utf8');
 const driverSource=readFileSync(path.join(root,'engine/src/driver.rs'),'utf8');
 const literal=(text,key)=>{const m=text.match(new RegExp(`(?:pub )?const ${key}\\s*:\\s*&str\\s*=\\s*"([^"]+)"`));if(!m)throw Error('Missing engine constant '+key);return m[1];};
 const firmwareName=literal(flashSource,'FIRMWARE');
 const loaderName=literal(loaderSource,'NAME');
 const list=[
  ['loader/'+loaderName, literal(loaderSource,'SHA256')],
  ['firmware/'+firmwareName, literal(flashSource,'FIRMWARE_SHA256')],
  ...[...driverSource.matchAll(/\("(rockusb\.(?:inf|cat|sys))","([a-f0-9]{64})"\)/g)].map(m=>['rockusb/'+m[1],m[2]])
 ];
 if(list.length!==5)throw Error('Expected loader, firmware and three Rockusb inputs');
 const files=list.map(([name,expected])=>{
  const override=name.startsWith('loader/')?env.K11C_LOADER_SOURCE:name.startsWith('firmware/')?env.K11C_FIRMWARE_SOURCE:env.K11C_DRIVER_SOURCE?path.join(env.K11C_DRIVER_SOURCE,path.basename(name)):null;
  const file=override?path.resolve(override):path.join(base,name), actual=createHash('sha256').update(readFileSync(file)).digest('hex');
  if(actual!==expected)throw Error('Build input checksum mismatch: '+name);
  return {name,path:realpathSync(file),sha256:actual};
 });
 const manifestPath=env.K11C_FIRMWARE_SOURCE?path.resolve(env.K11C_FIRMWARE_MANIFEST??(env.K11C_FIRMWARE_SOURCE+'.manifest.txt')):path.join(base,'firmware/manifest.txt');
 const manifest=readFileSync(manifestPath,'utf8');
 if(!manifest.split(/\r?\n/).includes('uboot.sha256='+literal(flashSource,'FIRMWARE_SHA256')))throw Error('Firmware manifest mismatch');
 return {files,manifestPath,firmwareName,firmwareHash:literal(flashSource,'FIRMWARE_SHA256'),loaderName,loaderHash:literal(loaderSource,'SHA256')};
}
