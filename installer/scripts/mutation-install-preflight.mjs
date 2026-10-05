import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import assert from 'node:assert/strict';
import {toolchain} from './toolchain.mjs';
process.chdir(path.resolve(import.meta.dirname,'..'));
const {cargo,env}=toolchain();env.RUST_TEST_THREADS='2';env.CARGO_PROFILE_DEV_OPT_LEVEL='1';
const cases=[
 ['composition digest ignored','engine/src/components.rs','if copied!=s.data_sha256','if false','k11c-usb','components::tests::composition_verifies_data'],
 ['composition digest reversed','engine/src/components.rs','if copied!=s.data_sha256','if copied==s.data_sha256','k11c-usb','components::tests::compose_preserves_os'],
 ['cancel gate removed','engine/src/flash.rs','gate()?;','','k11c-usb','flash::tests::optional_backup_production'],
 ['target fingerprint ignored','engine/src/flash.rs','if observed.digest()!=*expected','if false','k11c-usb','flash::tests::optional_backup_production'],
 ['target fingerprint reversed','engine/src/flash.rs','if observed.digest()!=*expected','if observed.digest()==*expected','k11c-usb','flash::tests::optional_backup_production'],
 ['source digest ignored','engine/src/flash.rs','if whole!=source','if false','k11c-usb','flash::tests::optional_backup_production'],
 ['source digest reversed','engine/src/flash.rs','if whole!=source','if whole==source','k11c-usb','flash::tests::optional_backup_production'],
 ['skip silently creates backup','engine/src/flash.rs','Recovery::DirectRestore|Recovery::SkipBackup=>Value::Null,','Recovery::DirectRestore|Recovery::SkipBackup=>crate::archive::create(io,base,"FULL",progress).await?,','k11c-usb','flash::tests::optional_backup_production'],
 ['full readback digest ignored','engine/src/flash.rs','if actual.hash!=r.sha256','if false','k11c-usb','flash::tests::pipeline_production_readback_faults'],
 ['GUI cancellation permission ignored','src-tauri/src/main.rs','if *stage!=WriteStage::Preparing','if false','k11c-installer','tests::storage_cancel_gate'],
 ['GUI permission guard reversed','src-tauri/src/main.rs','if *stage!=WriteStage::Preparing','if *stage==WriteStage::Preparing','k11c-installer','tests::storage_cancel_gate'],
 ['GUI writing can be cancelled','src-tauri/src/main.rs','if *stage==WriteStage::Writing','if false','k11c-installer','tests::storage_cancel_gate'],
 ['GUI close guard released too late','src-tauri/src/main.rs','self.writing.store(true,std::sync::atomic::Ordering::SeqCst);*stage=WriteStage::Writing;','*stage=WriteStage::Writing;','k11c-installer','tests::storage_cancel_gate'],
 ['restore payload validation omitted','engine/src/flash.rs','let work=build(base,&p.operation,&p.source,&identity,device,&saved,true)?;','let work=build(base,&p.operation,&p.source,&identity,device,&saved,false)?;','k11c-usb','flash::tests::restore_metadata_preview'],
 ['restore preview repeats payload validation','engine/src/flash.rs','let work=build(base,operation,source,&identity,device,&current,false)?;','let work=build(base,operation,source,&identity,device,&current,true)?;','k11c-usb','flash::tests::restore_metadata_preview'],
 ['GUI close marker omitted','src-tauri/src/main.rs','*stage=WriteStage::Closing;false','false','k11c-installer','tests::storage_cancel_gate_serializes'],
 ['GUI close restart guard omitted','src-tauri/src/main.rs','if *stage==WriteStage::Closing{return Err(fail("STORAGE_CANCELLED","Window is closing"));}','if false{return Err(fail("STORAGE_CANCELLED","Window is closing"));}','k11c-installer','tests::storage_cancel_gate_serializes'],
 ['GUI close writing guard reversed','src-tauri/src/main.rs','if self.writing.load(std::sync::atomic::Ordering::SeqCst){return true;}','if !self.writing.load(std::sync::atomic::Ordering::SeqCst){return true;}','k11c-installer','tests::storage_cancel_gate_serializes'],
];
const restoreOnly=process.argv.includes('--restore-only');
const closeOnly=process.argv.includes('--close-only');
const compositionOnly=process.argv.includes('--composition-only');
const compositionBaselines=[['k11c-usb','components::tests::compose_preserves_os'],['k11c-usb','components::tests::composition_verifies_data']];
const baselines=compositionOnly?compositionBaselines:closeOnly?[['k11c-installer','tests::storage_cancel_gate']]:restoreOnly?[['k11c-usb','flash::tests::restore_metadata_preview']]:[['k11c-usb','flash::tests::optional_backup'],['k11c-installer','tests::storage_cancel_gate'],['k11c-usb','flash::tests::restore_metadata_preview'],...compositionBaselines];
mkdirSync('test-results',{recursive:true});
const test=(pkg,filter)=>spawnSync(cargo,['test','--locked','-p',pkg,...(pkg==='k11c-usb'?['--lib']:[]),filter],{env,encoding:'utf8',timeout:300000,maxBuffer:8*1024*1024});
for(const [pkg,filter] of baselines){
 const r=test(pkg,filter);assert.equal(r.status,0,r.stdout+r.stderr);
}
const results=[];
for(const [name,file,before,after,pkg,filter] of cases.filter(c=>compositionOnly?c[0].startsWith('composition '):closeOnly?c[0].startsWith('GUI close'):!restoreOnly||c[0].startsWith('restore '))){
 const source=readFileSync(file,'utf8');assert.equal(source.split(before).length-1,1,name);
 try{
  writeFileSync(file,source.replace(before,after),'utf8');const r=test(pkg,filter);
  const killed=r.status!==0&&/test result: FAILED/.test(r.stdout)&&!/could not compile/.test(r.stderr);
  writeFileSync('test-results/preflight-mutation-'+name.replaceAll(' ','-')+'.txt',r.stdout+r.stderr);
  results.push({name,killed});console.log(`${killed?'KILLED':'SURVIVED'}: ${name}`);assert.ok(killed,r.stdout+r.stderr);
 }finally{writeFileSync(file,source,'utf8');}
}
for(const [pkg,filter] of baselines){const r=test(pkg,filter);assert.equal(r.status,0,r.stdout+r.stderr);}
writeFileSync(compositionOnly?'test-results/composition-preflight-mutations.json':closeOnly?'test-results/close-preflight-mutations.json':restoreOnly?'test-results/restore-preflight-mutations.json':'test-results/preflight-mutations.json',JSON.stringify({passed:true,results,production_paths:true,restored:true},null,2));
