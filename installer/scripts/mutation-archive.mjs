import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import assert from 'node:assert/strict';
import path from 'node:path';
import {toolchain} from './toolchain.mjs';
process.chdir(path.resolve(import.meta.dirname,'..'));
const {cargo,env}=toolchain();env.RUST_TEST_THREADS='2';
const originals=Object.fromEntries(['archive.rs','flash.rs'].map(f=>[f,readFileSync('engine/src/'+f,'utf8')]));
const cases=[
 ['RAW imported catalog omitted','archive.rs','if imports.exists(){for entry','if false{for entry','raw_catalog_persists'],
 ['RAW changed import hash ignored','archive.rs','if worker.finish()?.hash!=r.sha256','if false','raw_catalog_persists'],
 ['RAW changed import hash reversed','archive.rs','if worker.finish()?.hash!=r.sha256','if worker.finish()?.hash==r.sha256','pipeline_production_raw'],
 ['archive metadata hash removed','archive.rs','if Sha256::digest(&bytes)[..]!=footer[..32]','if false','archive::tests'],
 ['archive payload hash removed','archive.rs','if worker.finish()?.hash!=r.sha256{return Err(err("Backup payload checksum mismatch"));}','if false{return Err(err("Backup payload checksum mismatch"));}','archive::tests'],
 ['archive range guard removed','archive.rs','a.lba!=b.lba||a.bytes!=b.bytes||','', 'archive::tests'],
 ['archive saved headers ignored','archive.rs','if self.meta.kind!="HAOS"&&(primary!=self.meta.primary||tail!=self.meta.tail)','if false','archive::tests'],
 ['archive codec signature guard removed','archive.rs','if meta.format!=format','if false','archive_zstd_framing'],
 ['archive Zstd tail guard removed','archive.rs','if input.get_ref().limit()!=0||!input.buffer().is_empty()','if false','archive_zstd_framing'],
 // Removing this setting retains Zstd's identical built-in default (27).
 // Raise it instead so the mutation actually weakens the production boundary.
 ['archive Zstd memory cap weakened','archive.rs','decoder.window_log_max(27)','decoder.window_log_max(28)','archive_zstd_framing'],
 ['archive legacy XZ dispatch removed','archive.rs','else if self.meta.format==3','else if false','archive_legacy_xz'],
 ['archive capacity guard removed','flash.rs','if a.meta.identity.sectors!=identity.sectors{return Err(fail("RESTORE_CAPACITY"','if false{return Err(fail("RESTORE_CAPACITY"','archive_restore_rejects'],
 ['archive capacity guard reversed','flash.rs','if a.meta.identity.sectors!=identity.sectors{return Err(fail("RESTORE_CAPACITY"','if a.meta.identity.sectors==identity.sectors{return Err(fail("RESTORE_CAPACITY"','archive_full_unknown'],
 ['archive partial layout guard removed','flash.rs','if current.primary[1024..]!=a.meta.primary[1024..]','if false','archive_restore_rejects'],
 ['current full data comparison removed','archive.rs','if actual.hash!=r.sha256{return Err(fail("DEVICE_CHANGED"','if false{return Err(fail("DEVICE_CHANGED"','archive_wizard_reuses'],
 ['pending plan hash comparison removed','flash.rs','if read_bounded(&pending,128*1024)?!=bytes','if false','archive_wizard_reuses'],
 ['managed archive reuse removed','archive.rs','if valid_id(&name){return report(&managed.join(name.as_ref()),&a.meta);}','if false{return report(&managed.join(name.as_ref()),&a.meta);}','archive_full_unknown'],
];
function test(filter){return spawnSync(cargo,['test','--release','--locked','-p','k11c-usb','--lib',filter],{env,encoding:'utf8',timeout:300000,maxBuffer:8*1024*1024});}
const baseline=test('archive');assert.equal(baseline.status,0,baseline.stdout+baseline.stderr);const results=[];
try{
 for(const[name,file,before,after,filter] of (process.argv.includes('--pipeline-only')?cases.filter(c=>['RAW changed import hash ignored','RAW changed import hash reversed','archive payload hash removed','current full data comparison removed'].includes(c[0])):process.argv.includes('--raw-only')?cases.filter(c=>c[0].startsWith('RAW ')):process.argv.includes('--reuse-only')?cases.slice(-2):cases)){
  const original=originals[file];assert.equal(original.split(before).length-1,1,name);
  writeFileSync('engine/src/'+file,original.replace(before,after));const r=test(filter);writeFileSync('engine/src/'+file,original);
  const killed=r.status!==0&&/test result: FAILED/.test(r.stdout)&&!/could not compile/.test(r.stderr);
  results.push({name,killed});console.log(`${killed?'KILLED':'SURVIVED'}: ${name}`);assert.ok(killed,r.stdout+r.stderr);
 }
}finally{for(const[f,source]of Object.entries(originals))writeFileSync('engine/src/'+f,source);}
const restored=test('archive');assert.equal(restored.status,0,restored.stdout+restored.stderr);
mkdirSync('test-results',{recursive:true});writeFileSync('test-results/'+(process.argv.includes('--pipeline-only')?'pipeline-archive-mutations.json':process.argv.includes('--raw-only')?'raw-catalog-mutations.json':process.argv.includes('--reuse-only')?'archive-reuse-mutations.json':'archive-mutations.json'),JSON.stringify({passed:true,results,restored_baseline:true,transport:'virtual USB; production archive/plan/restore and real files'},null,2));
