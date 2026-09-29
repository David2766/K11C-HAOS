import {spawnSync} from 'node:child_process';
import {mkdirSync,copyFileSync,readFileSync,writeFileSync,existsSync,cpSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {buildInputs} from './build-inputs.mjs';
import {toolchain} from './toolchain.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
process.chdir(root);
const version=JSON.parse(readFileSync('package.json','utf8')).version;
const inputs=buildInputs(root);
if(process.argv.includes('--check-inputs')){
 console.log('INSTALLER_BUILD_INPUTS_PASS '+JSON.stringify(inputs.files.map(({name,sha256})=>({name,sha256}))));
 process.exit(0);
}
const {cargo,env}=toolchain();
function run(exe,args){const r=spawnSync(exe,args,{env,stdio:'inherit',shell:false});if(r.error)throw r.error;if(r.status!==0)throw Error(`${exe} exit=${r.status}`);}
run(process.execPath,['scripts/test-build-inputs.mjs']);
// Rust fixture tests consume the actual pinned loader/firmware below the
// source root. Materialize only these verified inputs into ignored resources.
for(const item of inputs.files.filter(item=>!item.name.startsWith('rockusb/'))){
 const target=path.join(root,'resources',item.name);
 mkdirSync(path.dirname(target),{recursive:true});
 if(path.resolve(item.path)!==path.resolve(target))copyFileSync(item.path,target);
 if(createHash('sha256').update(readFileSync(target)).digest('hex')!==item.sha256)throw Error('Test resource mismatch: '+item.name);
}
run(process.execPath,['scripts/icon.mjs']);
run(process.execPath,['node_modules/vue-tsc/bin/vue-tsc.js','--noEmit']);
run(process.execPath,['scripts/test-ui.mjs']);
run(process.execPath,['node_modules/vite/bin/vite.js','build']);
run(cargo,['test','--locked','-p','k11c-usb']);
run(cargo,['build','--locked','--release','-p','k11c-usb']);
const helper=path.join(root,'target/release/k11c-usb.exe');
env.K11C_HELPER_SHA256=createHash('sha256').update(readFileSync(helper)).digest('hex');
run(cargo,['test','--locked','-p','k11c-installer']);
run(cargo,['build','--locked','--release','-p','k11c-installer']);
const out=path.join(root,`release/K11C-Installer-${version}`);
const metadata=spawnSync(cargo,['metadata','--locked','--format-version','1'],{env,encoding:'utf8',maxBuffer:32*1024*1024});
if(metadata.status!==0)throw Error(metadata.stderr);
for(const pkg of JSON.parse(metadata.stdout).packages){
 const files=({'reqwest':['LICENSE-MIT','LICENSE-APACHE'],'xz2':['LICENSE-MIT','LICENSE-APACHE'],'fatfs':['LICENSE.txt'],'rfd':['LICENSE'],'lzma-sys':['LICENSE-MIT','LICENSE-APACHE','xz-5.2/COPYING']})[pkg.name];
 for(const file of files??[])copyFileSync(path.join(path.dirname(pkg.manifest_path),file),path.join('notices',pkg.name+'-'+path.basename(file)));
}
// Revalidate after compilation, then verify copied bytes before packaging.
const verified=buildInputs(root);
mkdirSync(out,{recursive:true});
for(const name of ['k11c-usb.exe','k11c-installer.exe'])copyFileSync(path.join(root,'target/release',name),path.join(out,name));
for(const item of verified.files){
 const target=path.join(out,'resources',item.name);
 mkdirSync(path.dirname(target),{recursive:true});
 if(path.resolve(item.path)!==path.resolve(target))copyFileSync(item.path,target);
 if(createHash('sha256').update(readFileSync(target)).digest('hex')!==item.sha256)throw Error('Packaged resource mismatch: '+item.name);
}
const manifestTarget=path.join(out,'resources/firmware/manifest.txt');
if(path.resolve(verified.manifestPath)!==path.resolve(manifestTarget))copyFileSync(verified.manifestPath,manifestTarget);
for(const name of ['README.md','api-contract.md','THIRD-PARTY.md'])copyFileSync(path.join(root,name),path.join(out,name));
if(existsSync(path.join(root,'notices')))cpSync(path.join(root,'notices'),path.join(out,'notices'),{recursive:true});
writeFileSync(path.join(out,'BUILD.json'),JSON.stringify({version,built:new Date().toISOString(),helper_sha256:env.K11C_HELPER_SHA256,loader_sha256:verified.loaderHash,firmware_sha256:verified.firmwareHash,rockchiprs:'31d8bb996d841e270d4dd55887f60cf9ffe27103',scope:'HAOS install, U-Boot update, boot backup/restore and GPT repair with readback verification',hardware_verified:false},null,2)+'\n');
console.log(`PORTABLE_DIRECTORY=${out}`);
