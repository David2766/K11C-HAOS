import {spawnSync} from 'node:child_process';
import {readFileSync,mkdirSync,writeFileSync,cpSync,mkdtempSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');process.chdir(root);
const release=path.join(root,'release/K11C-Installer-'+JSON.parse(readFileSync('package.json','utf8')).version);
const exe=path.join(release,'k11c-usb.exe');
const hash=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
function run(args,exePath=exe,cwd=root){const r=spawnSync(exePath,args,{encoding:'utf8',cwd,timeout:35000});assert.ifError(r.error);return {exit:r.status,value:JSON.parse(r.stdout)};}
const preflight=run(['preflight']);assert.equal(preflight.exit,0);assert.equal(preflight.value.ok,true);
assert.equal(preflight.value.data.driver.installed,true,'This host verification requires its existing driver; never installs a missing driver.');
const installedInf=path.join(process.env.SystemRoot,'INF',preflight.value.data.driver.published_inf);
const before=hash(installedInf);
for(const args of [['write','0','image'],['erase'],['preflight','extra'],['driver-install-only','untrusted.inf'],['storage-execute','a'.repeat(64)],['storage-execute','a'.repeat(64),'--yes'],['storage-plan','id','port','erase','']]){const r=run(args);assert.equal(r.exit,1);assert.equal(r.value.error.code,'COMMAND_NOT_ALLOWED');}
const absent=run(['inspect','USB\\VID_2207&PID_350A\\NONEXISTENT-K11C-TEST']);assert.equal(absent.value.error.code,'DEVICE_GONE');
const build=JSON.parse(readFileSync(path.join(release,'BUILD.json')));assert.equal(hash(exe),build.helper_sha256);
const expected={ 'rockusb.inf':'4fc1ea1a32e235f03805b0955dc06203be15738d0672d9679a9539ccc23e8887','rockusb.cat':'f9098e5a0067b5d01cb6f5be87f42a196503aee7e2d1fc41d573c80fd5c9f10f','rockusb.sys':'dbe50ec840008e3db5f4acd6063f622438f3bbb049e4db84e1721ea0425fcac1'};
for(const [name,digest] of Object.entries(expected))assert.equal(hash(path.join(release,'resources/rockusb',name)),digest);
// Same packaged CLI, moved under a Unicode path and invoked from a different cwd.
mkdirSync('test-results',{recursive:true});
const relocated=mkdtempSync(path.join(root,'test-results/포터블 이동-'));cpSync(release,relocated,{recursive:true,filter:p=>!p.startsWith(path.join(release,'data'))});
const moved=run(['preflight'],path.join(relocated,'k11c-usb.exe'),process.env.SystemRoot);assert.deepEqual(moved.value,preflight.value);
assert.equal(hash(installedInf),before);
const source=readFileSync('engine/src/usb.rs','utf8');assert.doesNotMatch(source,/\.(erase_lba|erase_force|write_sector|reset_device|switch_storage)\(/);
assert.equal((source.match(/\.write_lba\(/g)??[]).length,1,'One bounded transport boundary, no general raw-write dispatcher');
assert.match(source,/impl crate::flash::FlashIo for WindowsIo/);
assert.equal(hash(path.join(release,'resources/loader/k11c-usb-loader-v1.23.114.bin')),build.loader_sha256);
for(const args of [['backup','USB\\VID_2207&PID_350A\\NONEXISTENT-K11C-TEST'],['prepare','USB\\VID_2207&PID_350A\\NONEXISTENT-K11C-TEST','port','--confirmed-k11c'],['gpt-check','USB\\VID_2207&PID_350A\\NONEXISTENT-K11C-TEST'],['storage-plan','USB\\VID_2207&PID_350A\\NONEXISTENT-K11C-TEST','port','uboot','']])assert.equal(run(args).value.error.code,'DEVICE_GONE');
assert.equal(hash(path.join(release,'resources/firmware/u-boot-k11c-dfi-r24.bin')),build.firmware_sha256);
assert.equal(run(['storage-execute','0'.repeat(64),'--confirmed-k11c-write']).value.ok,false,'Missing plan cannot write');
assert.equal(run(['prepare','device','port']).value.error.code,'COMMAND_NOT_ALLOWED');
const ui=['src/App.vue','src/InstallWizard.vue'].map(p=>readFileSync(p,'utf8')).join('\n');assert.doesNotMatch(ui,/NO FLASH|PREVIEW ·|다음 구현 단계|이번 버전은|읽기 전용 빌드|fake-input/);
const result={passed:true,actual_packaged_cli:true,preflight:preflight.value.data,negative_dispatch:true,missing_selection:true,explicit_loader_confirmation:true,explicit_storage_confirmation:true,unicode_relocation:true,driver_payload_digests:true,loader_payload_digest:true,firmware_payload_digest:true,customer_copy:true,installed_inf_unchanged:true,physical_usb_writes_in_this_test:0,hardware_read_verified:false,hardware_loader_verified:false,hardware_write_verified:false};
writeFileSync('test-results/verification.json',JSON.stringify(result,null,2));console.log(JSON.stringify(result,null,2));
