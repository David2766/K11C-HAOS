import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..');process.chdir(root);
import {toolchain} from './toolchain.mjs';
const {cargo,env}=toolchain();
const files=['flash.rs','gpt.rs'];const originals=Object.fromEntries(files.map(f=>[f,readFileSync('engine/src/'+f,'utf8')]));
const cases=[
 ['confirmation removed','flash.rs','if !confirmed{','if false{','missing_confirmation'],
 ['confirmation reversed','flash.rs','if !confirmed{','if confirmed{','uboot_update'],
 ['physical selection ignored','flash.rs','if p.device.instance_id!=device.instance_id||p.device.location!=device.location','if false','missing_confirmation'],
 ['stale device data ignored','flash.rs','if current.head()!=saved.head()||current.tail!=saved.tail','if false','missing_confirmation'],
 ['source hash ignored','flash.rs','if hash_range(&mut f,0,len)?!=source','if false','source_plan_backup'],
 ['plan hash ignored','flash.rs','if hash_bytes(&b)!=id','if false','source_plan_backup'],
 ['backup schema ignored','flash.rs','if p.file!=name||p.lba!=lba||p.sectors!=sectors||p.bytes!=sectors as u64*512','if false','invalid_firmware'],
 ['firmware digest ignored','flash.rs','||hash_bytes(&b)!=FIRMWARE_SHA256','','invalid_firmware'],
 ['short write ignored','flash.rs','if transferred as usize!=n','if false','short_corrupt'],
 ['readback hash ignored','flash.rs','if format!("{:x}",actual.finalize())!=r.sha256','if false','short_corrupt'],
 ['identity final check ignored','flash.rs','if io.identity().await?!=identity{return Err(fail("DEVICE_CHANGED","Identity changed during storage transaction"));}','','short_corrupt'],
 ['restore layout ignored','flash.rs','if active[1024..]!=saved.primary[1024..]','if false','modified_partition'],
 ['write bounds ignored','flash.rs','if r.bytes==0||r.bytes%512!=0||end>sectors as u64','if false','bounds_ids'],
 ['overlap ignored','flash.rs','if sorted.windows(2).any(|r|r[0].1>r[1].0)','if false','bounds_ids'],
 ['ambiguous GPT accepted','gpt.rs','if av&&bv&&(primary[1024..]!=tail[..16384]||a[40..56]!=b[40..56])','if false','modified_partition'],
 ['relocation removed','gpt.rs','put64(e,o,u64at(e,o)+32768);','put64(e,o,u64at(e,o));','install_preserves'],
 ['physical backup GPT omitted','flash.rs','segments.push(bytes("Backup GPT",identity.sectors-33,tail.clone()));','/* removed */','install_preserves'],
];
function test(filter='flash::tests'){return spawnSync(cargo,['test','--locked','-p','k11c-usb','--lib',filter],{env,encoding:'utf8',timeout:120000,maxBuffer:8*1024*1024});}
mkdirSync('test-results',{recursive:true});assert.equal(test().status,0,'baseline');const results=[];
try{
 for(const [name,file,before,after,filter] of cases){
  const original=originals[file];assert.equal(original.split(before).length-1,1,`unique anchor ${name}`);
  writeFileSync('engine/src/'+file,original.replace(before,after));
  const r=test('flash::tests::'+filter);writeFileSync('engine/src/'+file,original);
  const killed=r.status!==0&&/test result: FAILED/.test(r.stdout)&&!/could not compile/.test(r.stderr);
  results.push({name,killed});console.log(`${killed?'KILLED':'SURVIVED'}: ${name}`);
  if(!killed)throw Error(r.stdout+r.stderr);
 }
}finally{for(const [file,source] of Object.entries(originals))writeFileSync('engine/src/'+file,source);}
assert.equal(test().status,0,'restored baseline');
writeFileSync('test-results/flash-mutations.json',JSON.stringify({results,restored_baseline:true,transport:'fake USB, production transaction and real filesystem'},null,2));
