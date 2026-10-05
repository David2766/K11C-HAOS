// Explicit hardware smoke test: no erase/write/reset, driver installation,
// firmware resources or RAM Loader payload in the temporary helper directory.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,copyFileSync,mkdtempSync,existsSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const version=JSON.parse(readFileSync(path.join(root,'package.json'))).version;
const release=path.join(root,`release/K11C-Installer-${version}`);
const binary=path.join(release,'k11c-usb.exe');
const hash=createHash('sha256').update(readFileSync(binary)).digest('hex');
assert.equal(hash,JSON.parse(readFileSync(path.join(release,'BUILD.json'))).helper_sha256);
mkdirSync(path.join(root,'test-results'),{recursive:true});
const work=mkdtempSync(path.join(root,'test-results/connected-readonly-'));
const helper=path.join(work,'k11c-usb.exe');copyFileSync(binary,helper);
assert.equal(existsSync(path.join(work,'resources')),false,'RAM upload cannot load a payload');
const report={helper_sha256:hash,started:new Date().toISOString(),steps:[]};
function run(args,timeout=30000){
 const started=Date.now();const r=spawnSync(helper,args,{encoding:'utf8',timeout});
 assert.ifError(r.error);const value=JSON.parse(r.stdout);
 report.steps.push({command:args[0],elapsed_ms:Date.now()-started,result:value,progress:r.stderr});
 writeFileSync(path.join(work,'report.json'),JSON.stringify(report,null,2));
 assert.equal(r.status,0,JSON.stringify(value));assert.equal(value.ok,true);console.log(`PASS ${args[0]}`);return value.data;
}
const devices=run(['devices']);assert.equal(devices.length,1,'Connect only the intended K11C');
const d=devices[0];assert.equal(d.binding,true);assert.equal(d.vid,0x2207);assert.ok([0x350a,0x300a].includes(d.pid));
const before=run(['inspect',d.instance_id]);assert.equal(before.ready,true);
assert.equal(before.location,d.location);assert.equal(before.storage,'emmc');
assert.match(before.read_capability_hex,/^[a-f0-9]{16}$/);
assert.ok(parseInt(before.read_capability_hex.slice(0,2),16)&8,'Full read capability is required');
const gpt=run(['gpt-check',d.instance_id]);
assert.equal(gpt.gpt.healthy,true,'This acceptance test requires the existing healthy K11C disk; do not repair based on failed USB reads');
assert.equal(gpt.repairable,false);
const prepared=run(['prepare',d.instance_id,d.location,'--confirmed-k11c']);
assert.equal(prepared.uploaded,false,'Already-ready device must not upload');
assert.equal(prepared.observation.ready,true);assert.equal(prepared.device.mode,d.mode);
const backup=run(['backup',d.instance_id],300000);
const manifest=JSON.parse(readFileSync(path.join(backup.path,'manifest.json')));
assert.equal(manifest.complete,true);assert.equal(manifest.user_data_included,false);
assert.equal(manifest.format,2);assert.equal(manifest.read_capability_hex,before.read_capability_hex);
assert.equal(manifest.gpt.healthy,true);assert.deepEqual(manifest.gpt,gpt.gpt);
for(const part of manifest.files){
 const bytes=readFileSync(path.join(backup.path,part.file));
 assert.equal(bytes.length,part.bytes);
 assert.equal(createHash('sha256').update(bytes).digest('hex'),part.sha256);
 if(part.file.startsWith('gpt-'))assert.ok(!bytes.every(b=>b===0xcc),'Sentinel data is not a backup');
}
const after=run(['inspect',d.instance_id]);assert.deepEqual(after,before,'Read-only operations preserve observed identity/headers');
report.complete=true;report.backup_path=backup.path;report.uploaded=false;report.persistent_writes=0;
writeFileSync(path.join(work,'report.json'),JSON.stringify(report,null,2));
console.log(`CONNECTED_READONLY_PASS descriptor=${d.mode} ready=true uploaded=false backup=${backup.path}`);
console.log(`REPORT=${path.join(work,'report.json')}`);
