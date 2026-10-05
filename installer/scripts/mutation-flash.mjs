import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..');process.chdir(root);
import {toolchain} from './toolchain.mjs';
const {cargo,env}=toolchain();
env.RUST_TEST_THREADS='2';
const profile=process.argv.includes('--debug')?'dev':'release';
// Crypto/full-media regressions are CPU-bound without any optimization. Keep
// quick mutation builds while exercising the same production code and guards.
if(profile==='dev')env.CARGO_PROFILE_DEV_OPT_LEVEL='1';
const files=['flash.rs','gpt.rs'];const originals=Object.fromEntries(files.map(f=>[f,readFileSync('engine/src/'+f,'utf8')]));
const cases=[
 ['restore renewal consumed guard omitted','flash.rs','if dir.join(format!("{id}.used.json")).exists(){','if false{','repeated_restore_renews'],
 ['restore renewal stays blocked','flash.rs','p.attempt=Some(p.attempt.unwrap_or(0).checked_add(1).ok_or_else(||fail("PLAN_CHANGED","Transaction attempt counter exhausted"))?);','return Err(fail("PLAN_CHANGED","This transaction was already attempted"));','repeated_restore_renews'],
 ['restore renewal attempt reversed','flash.rs','p.attempt.unwrap_or(0).checked_add(1)','p.attempt.unwrap_or(0).checked_sub(1)','repeated_restore_after_failed'],
 ['restore renewal pending comparison omitted','flash.rs','if read_bounded(&pending,128*1024)?!=bytes','if false','repeated_restore_renews'],
 ['restore renewal pending comparison reversed','flash.rs','if read_bounded(&pending,128*1024)?!=bytes','if read_bounded(&pending,128*1024)?==bytes','repeated_restore_renews'],
 ['restore renewal one-use claim omitted','flash.rs','fs::rename(dir.join(format!("{id}.json")),dir.join(format!("{id}.used.json"))).map_err(disk_io)?;','','repeated_restore_renews'],
 ['restore renewal target hash omitted','flash.rs','if observed.digest()!=*expected','if false','repeated_restore_renewed'],
 ['restore renewal target hash reversed','flash.rs','if observed.digest()!=*expected','if observed.digest()==*expected','repeated_restore_renewed'],
 ['restore renewal confirmation omitted','flash.rs','if !confirmed{','if false{','repeated_restore_renewed'],
 ['restore renewal physical selection omitted','flash.rs','if p.device.instance_id!=device.instance_id||p.device.location!=device.location','if false','repeated_restore_renewed'],
 ['restore renewal metadata hash omitted','flash.rs','if hash_bytes(&b)!=id','if false','repeated_restore_renewed'],
 ['restore renewal full readback omitted','flash.rs','if actual.hash!=r.sha256','if false','repeated_restore_renewed'],
 ['restore renewal full readback reversed','flash.rs','if actual.hash!=r.sha256','if actual.hash==r.sha256','repeated_restore_renewed'],
 ['direct restore target hash omitted','flash.rs','if observed.digest()!=*expected','if false','direct_restore_guards'],
 ['direct restore target hash reversed','flash.rs','if observed.digest()!=*expected','if observed.digest()==*expected','direct_restore_raw'],
 ['direct restore creates redundant backup','flash.rs','Recovery::DirectRestore|Recovery::SkipBackup=>Value::Null,','Recovery::DirectRestore|Recovery::SkipBackup=>crate::archive::create(io,base,"FULL",progress).await?,','direct_restore_raw'],
 ['direct restore operation scope omitted','flash.rs','if matches!(recovery,Recovery::DirectRestore)&&!["restore","restore-archive","factory-raw"].contains(&operation)','if false','direct_restore_guards'],
 ['direct restore confirmation omitted','flash.rs','if !confirmed{','if false{','direct_restore_guards'],
 ['direct restore confirmation reversed','flash.rs','if !confirmed{','if confirmed{','direct_restore_raw'],
 ['direct restore readback omitted','flash.rs','if actual.hash!=r.sha256','if false','direct_restore_write_failure'],
 ['direct restore readback reversed','flash.rs','if actual.hash!=r.sha256','if actual.hash==r.sha256','pipeline_production_raw'],
 ['confirmation removed','flash.rs','if !confirmed{','if false{','missing_confirmation'],
 ['confirmation reversed','flash.rs','if !confirmed{','if confirmed{','uboot_update'],
 ['physical selection ignored','flash.rs','if p.device.instance_id!=device.instance_id||p.device.location!=device.location','if false','missing_confirmation'],
 ['stale device data ignored','flash.rs','if current.head()!=saved.head()||current.tail!=saved.tail','if false','missing_confirmation'],
 ['source hash ignored','flash.rs','if whole!=source','if false','source_plan_backup'],
 ['plan hash ignored','flash.rs','if hash_bytes(&b)!=id','if false','source_plan_backup'],
 ['backup schema ignored','flash.rs','if p.file!=name||p.lba!=lba||p.sectors!=sectors||p.bytes!=sectors as u64*512','if false','invalid_firmware'],
 ['firmware digest ignored','flash.rs','||hash_bytes(&b)!=FIRMWARE_SHA256','','invalid_firmware'],
 ['short write ignored','flash.rs','if transferred as usize!=n','if false','short_corrupt'],
 ['readback hash ignored','flash.rs','if actual.hash!=r.sha256','if false','short_corrupt'],
 ['identity final check ignored','flash.rs','if io.identity().await?!=identity{return Err(fail("DEVICE_CHANGED","Identity changed during storage transaction"));}','','short_corrupt'],
 ['restore layout ignored','flash.rs','if active[1024..]!=saved.primary[1024..]','if false','modified_partition'],
 ['write bounds ignored','flash.rs','if r.bytes==0||r.bytes%512!=0||end>sectors as u64','if false','bounds_ids'],
 ['overlap ignored','flash.rs','if sorted.windows(2).any(|r|r[0].1>r[1].0)','if false','bounds_ids'],
 ['ambiguous GPT accepted','gpt.rs','if av&&bv&&(primary[1024..]!=tail[..16384]||a[40..56]!=b[40..56])','if false','modified_partition'],
 ['relocation removed','gpt.rs','put64(e,o,u64at(e,o)+32768);','put64(e,o,u64at(e,o));','install_preserves'],
 ['physical backup GPT omitted','flash.rs','segments.push(bytes("Backup GPT",identity.sectors-33,tail.clone()));\n            segments.push(bytes("Primary GPT",0,primary.clone()));','segments.push(bytes("Primary GPT",0,primary.clone()));','install_preserves'],
];
function test(filter='flash::tests'){return spawnSync(cargo,['test',...(profile==='release'?['--release']:[]),'--locked','-p','k11c-usb','--lib',filter],{env,encoding:'utf8',timeout:300000,maxBuffer:8*1024*1024});}
const baselineFilter=process.argv.includes('--renewal-only')?'flash::tests::repeated_restore':'flash::tests';
mkdirSync('test-results',{recursive:true});assert.equal(test(baselineFilter).status,0,'baseline');const results=[];
try{
 for(const [name,file,before,after,filter] of cases.filter(c=>process.argv.includes('--renewal-only')?c[0].startsWith('restore renewal'):process.argv.includes('--pipeline-only')?['direct restore readback omitted','direct restore readback reversed','short write ignored'].includes(c[0]):!process.argv.includes('--direct-only')||c[0].startsWith('direct restore'))){
  const original=originals[file];assert.equal(original.split(before).length-1,1,`unique anchor ${name}`);
  writeFileSync('engine/src/'+file,original.replace(before,after));
  const r=test('flash::tests::'+filter);writeFileSync('engine/src/'+file,original);
  const killed=r.status!==0&&/test result: FAILED/.test(r.stdout)&&!/could not compile/.test(r.stderr);
  results.push({name,killed});console.log(`${killed?'KILLED':'SURVIVED'}: ${name}`);
  if(!killed)throw Error(r.stdout+r.stderr);
 }
}finally{for(const [file,source] of Object.entries(originals))writeFileSync('engine/src/'+file,source);}
assert.equal(test(baselineFilter).status,0,'restored baseline');
writeFileSync(process.argv.includes('--renewal-only')?'test-results/restore-renewal-mutations.json':process.argv.includes('--pipeline-only')?'test-results/pipeline-flash-mutations.json':process.argv.includes('--direct-only')?'test-results/direct-restore-mutations.json':'test-results/flash-mutations.json',JSON.stringify({results,restored_baseline:true,profile,transport:'fake USB, production transaction and real filesystem'},null,2));
