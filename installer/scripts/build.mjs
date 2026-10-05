import {spawnSync} from 'node:child_process';
import {mkdirSync,copyFileSync,readFileSync,writeFileSync,existsSync,cpSync,statSync,chmodSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {buildInputs,vendorInputs} from './build-inputs.mjs';
import {toolchain} from './toolchain.mjs';
import {buildNative,nativeInputs} from './build-native.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
process.chdir(root);
const version=JSON.parse(readFileSync('package.json','utf8')).version;
const inputs=buildInputs(root);
const vendor=vendorInputs(root);
nativeInputs(root);
if(process.argv.includes('--check-inputs')){
 console.log('INSTALLER_BUILD_INPUTS_PASS '+JSON.stringify([...inputs.files,...vendor.files].map(({name,sha256})=>({name,sha256}))));
 process.exit(0);
}
const {cargo,env}=toolchain();
env.RUST_TEST_THREADS='2';
buildNative(root,env);
// HAOS and Connectivity are acquired by the installation workflow, not by
// compilation. The source archive contains only the pinned USB/build inputs.
function run(exe,args){const r=spawnSync(exe,args,{env,stdio:'inherit',shell:false});if(r.error)throw r.error;if(r.status!==0)throw Error(`${exe} exit=${r.status}`);}
run(process.execPath,['scripts/test-build-inputs.mjs']);
// Rust fixture tests consume the actual pinned loader/firmware below the
// source root. Materialize only these verified inputs into ignored resources.
for(const item of [...inputs.files.filter(item=>!item.name.startsWith('rockusb/')),...vendor.files.map(item=>({...item,name:'rockchip/'+item.name}))]){
 const target=path.join(root,'resources',item.name);
 mkdirSync(path.dirname(target),{recursive:true});
 if(path.resolve(item.path)!==path.resolve(target))copyFileSync(item.path,target);
 if(createHash('sha256').update(readFileSync(target)).digest('hex')!==item.sha256)throw Error('Test resource mismatch: '+item.name);
}
run(process.execPath,['scripts/icon.mjs']);
run(process.execPath,['scripts/test-theme.mjs']);
run(process.execPath,['node_modules/vue-tsc/bin/vue-tsc.js','--noEmit']);
run(process.execPath,['scripts/test-i18n.mjs']);
run(process.execPath,['scripts/test-ui.mjs']);
run(process.execPath,['node_modules/vite/bin/vite.js','build']);
run(cargo,['test','--release','--locked','-p','k11c-usb']);
run(cargo,['build','--locked','--release','-p','k11c-usb']);
const metadata=spawnSync(cargo,['metadata','--locked','--format-version','1'],{env,encoding:'utf8',maxBuffer:32*1024*1024});
if(metadata.status!==0)throw Error(metadata.stderr);
const cargoMetadata=JSON.parse(metadata.stdout);
const binaryRoot=path.join(cargoMetadata.target_directory,'release');
const helper=path.join(binaryRoot,'k11c-usb.exe');
env.K11C_HELPER_SHA256=createHash('sha256').update(readFileSync(helper)).digest('hex');
run(cargo,['test','--release','--locked','-p','k11c-installer']);
run(cargo,['build','--locked','--release','-p','k11c-installer']);
const out=path.join(root,`release/K11C-Installer-${version}`);
for(const pkg of cargoMetadata.packages){
 const files=({'reqwest':['LICENSE-MIT','LICENSE-APACHE'],'xz2':['LICENSE-MIT','LICENSE-APACHE'],'fatfs':['LICENSE.txt'],'rfd':['LICENSE'],'lzma-sys':['LICENSE-MIT','LICENSE-APACHE','xz-5.2/COPYING'],'zstd':['LICENSE'],'zstd-safe':['LICENSE'],'zstd-sys':['LICENSE','zstd/LICENSE']})[pkg.name];
 for(const file of files??[])copyFileSync(path.join(path.dirname(pkg.manifest_path),file),path.join('notices',pkg.name+'-'+(pkg.name==='zstd-sys'?file.replaceAll('/','-'):path.basename(file))));
}
// Revalidate after compilation, then verify copied bytes before packaging.
const verified=buildInputs(root);
const verifiedVendor=vendorInputs(root);
mkdirSync(out,{recursive:true});
if(existsSync(path.join(out,'resources/components')))throw Error('Release directory contains obsolete bundled installation data; use a clean output directory.');
for(const name of ['k11c-usb.exe','k11c-installer.exe'])copyFileSync(path.join(binaryRoot,name),path.join(out,name));
for(const item of verified.files){
 const target=path.join(out,'resources',item.name);
 mkdirSync(path.dirname(target),{recursive:true});
 if(path.resolve(item.path)!==path.resolve(target))copyFileSync(item.path,target);
 if(createHash('sha256').update(readFileSync(target)).digest('hex')!==item.sha256)throw Error('Packaged resource mismatch: '+item.name);
}
for(const item of verifiedVendor.files){
 const target=path.join(out,'resources/rockchip',item.name);mkdirSync(path.dirname(target),{recursive:true});copyFileSync(item.path,target);
 if(createHash('sha256').update(readFileSync(target)).digest('hex')!==item.sha256)throw Error('Packaged vendor resource mismatch');
}
copyFileSync(verifiedVendor.revisionPath,path.join(out,'resources/rockchip/revision.txt'));
mkdirSync(path.join(out,'resources/native'),{recursive:true});
for(const item of nativeInputs(root).files)copyFileSync(item.path,path.join(out,'resources/native',item.name));
copyFileSync(path.join(root,'resources/native/tools.json'),path.join(out,'resources/native/tools.json'));
copyFileSync(path.join(root,'resources/native/k11c-seed.exe'),path.join(out,'resources/native/k11c-seed.exe'));
const manifestTarget=path.join(out,'resources/firmware/manifest.txt');
if(path.resolve(verified.manifestPath)!==path.resolve(manifestTarget))copyFileSync(verified.manifestPath,manifestTarget);
for(const name of ['README.md','api-contract.md','THIRD-PARTY.md'])copyFileSync(path.join(root,name),path.join(out,name));
mkdirSync(path.join(out,'docs'),{recursive:true});
copyFileSync(path.join(root,'docs/ram-loader-read-access.md'),path.join(out,'docs/ram-loader-read-access.md'));
if(existsSync(path.join(root,'notices')))cpSync(path.join(root,'notices'),path.join(out,'notices'),{recursive:true,filter:(_source,target)=>{
 if(existsSync(target)&&statSync(target).isFile())chmodSync(target,0o644);return true;
}});
writeFileSync(path.join(out,'BUILD.json'),JSON.stringify({version,built:new Date().toISOString(),helper_sha256:env.K11C_HELPER_SHA256,native_sha256:env.K11C_NATIVE_SHA256,native_tools_sha256:nativeInputs(root).manifestHash,loader_sha256:verified.loaderHash,firmware_sha256:verified.firmwareHash,install_components:'download-on-install',preparation:'windows-native-containerd-ext4',manufacturer_tool:{version:'2.46',files:verifiedVendor.files.map(({name,sha256})=>({name,sha256}))},rockchiprs:'31d8bb996d841e270d4dd55887f60cf9ffe27103',scope:'Official HAOS with factory Connectivity, release-resolved U-Boot, app maintenance, manufacturer RKFW/RAW installation, compressed full/partial backup/restore and GPT repair',hardware_verified:false},null,2)+'\n');
console.log(`PORTABLE_DIRECTORY=${out}`);
