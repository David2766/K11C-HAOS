import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');process.chdir(root);
import {toolchain} from './toolchain.mjs';
const {cargo,env}=toolchain();
env.RUST_TEST_THREADS='2';
const cases=[
 ['read-access-skip-patch','engine/src/loader.rs','enable_full_read(&mut loader)?;','// mutation: skipped RAM patch','loader::tests::'],
 ['read-access-old-limit','engine/src/loader.rs','0x52a00028u32,0x12800008u32','0x52a00028u32,0x52a00028u32','loader::tests::'],
 ['read-access-old-capability','engine/src/loader.rs','0x528086e8u32,0x528087e8u32','0x528086e8u32,0x528086e8u32','loader::tests::'],
 ['read-access-skip-capability','engine/src/workflow.rs','if caps[0]&0x08==0','if false','workflow::tests::restricted_or_identical_cc_reads'],
 ['read-access-reverse-capability','engine/src/workflow.rs','if caps[0]&0x08==0','if caps[0]&0x08!=0','workflow::tests::backup_roundtrip'],
 ['read-access-skip-cc','engine/src/workflow.rs','if !bytes.is_empty() && bytes.iter().all(|b|*b==0xcc)','if false','workflow::tests::restricted_or_identical_cc_reads'],
 ['read-access-reverse-cc','engine/src/workflow.rs','bytes.iter().all(|b|*b==0xcc)','!bytes.iter().all(|b|*b==0xcc)','workflow::tests::restricted_or_identical_cc_reads'],
 ['read-access-backup-entry','engine/src/workflow.rs','let capabilities=require_full_read(io).await?;','let capabilities="3f07000000000000";','workflow::tests::restricted_or_identical_cc_reads'],
 ['read-access-backup-final','engine/src/workflow.rs','require_full_read(io).await?;\n        let gpt=','// mutation: skipped final capability\n        let gpt=','workflow::tests::capability_loss'],
 ['read-access-execute-final','engine/src/flash.rs','workflow::require_full_read(io).await?;\n    // Claim','// mutation: skipped pre-write capability\n    // Claim','flash::tests::capability_loss'],
 ['read-access-capture-tail','engine/src/flash.rs','workflow::require_gpt_bytes(&tail)?;','// mutation: skipped tail check','flash::tests::false_success_cc'],
 ['read-access-saved-tail','engine/src/flash.rs','workflow::require_gpt_bytes(&payloads[3])?;','// mutation: skipped saved tail check','flash::tests::legacy_cc_backup'],
 ['read-access-backup-evidence','engine/src/flash.rs','if m["format"]==2 {','if false {','flash::tests::capability_evidence'],
 ['read-access-transport-guard','engine/src/usb.rs','if !full_read {','if false {','usb::tests::storage_transport'],
 ['read-access-transport-reverse','engine/src/usb.rs','if !full_read {','if full_read {','usb::tests::storage_transport'],
 ['reuse-driver','engine/src/driver.rs','!installed && !attempted','!attempted'],
 ['reverse-reuse','engine/src/driver.rs','!installed && !attempted','installed && !attempted'],
 ['skip-bundle-check','engine/src/driver.rs','host.verify()?;','// verification removed by test'],
 ['accept-false-success','engine/src/driver.rs','if !after.installed','if false'],
 ['skip-file-digest','engine/src/lib.rs','if hash_bytes(&bytes) != expected','if false'],
 ['reverse-file-digest','engine/src/lib.rs','if hash_bytes(&bytes) != expected','if hash_bytes(&bytes) == expected'],
 ['allow-unknown-mode','engine/src/usb.rs','if !["Maskrom","Loader"].contains(&device.mode.as_str())','if false'],
 ['reject-ready-maskrom','engine/src/usb.rs','State::Ready=>return prepared(d,host.observe(d).await?,false),','State::Ready if d.mode=="Loader"=>return prepared(d,host.observe(d).await?,false),'],
 ['skip-post-upload-readiness','engine/src/usb.rs','match host.probe(&next) {','match Ok::<_,crate::Failure>(State::Ready) {'],
 ['reverse-post-upload-readiness','engine/src/usb.rs','Ok(State::Ready)=>return prepared','Ok(State::Unavailable)=>return prepared'],
 ['skip-ready-observation','engine/src/usb.rs','observation["ready"]!=true ||',''],
 ['skip-readiness-tag','engine/src/readiness.rs',' || &bytes[4..8]!=tag',''],
 ['reverse-readiness-success','engine/src/readiness.rs','0=>Ok(State::Ready)','0=>Ok(State::Busy)'],
 ['skip-transport-readiness','engine/src/readiness.rs','if state!=State::Ready','if false'],
 ['reverse-transport-readiness','engine/src/readiness.rs','if state!=State::Ready','if state==State::Ready'],
 ['allow-unbound-read','engine/src/usb.rs','if !device.binding','if false'],
 ['allow-wrong-storage','engine/src/usb.rs','if storage != "emmc"','if false'],
 ['reverse-storage','engine/src/usb.rs','if storage != "emmc"','if storage == "emmc"'],
 ['skip-loader-digest','engine/src/loader.rs','if hash_bytes(&bytes) != SHA256','if false'],
 ['reverse-loader-digest','engine/src/loader.rs','if hash_bytes(&bytes) != SHA256','if hash_bytes(&bytes) == SHA256'],
 ['skip-board-confirmation','engine/src/usb.rs','if !confirmed {return Err(fail("CONFIRM_BOARD"','if false {return Err(fail("CONFIRM_BOARD"'],
 ['skip-port-check','engine/src/usb.rs','if d.location != location','if false'],
 ['wrong-reconnect-port','engine/src/usb.rs','filter(|d|d.location==location)','filter(|_|true)'],
 ['accept-short-read','engine/src/workflow.rs','if n as usize != buf.len()','if false'],
 ['skip-backup-digest','engine/src/workflow.rs','if len != part.bytes || hash != part.sha256','if len != part.bytes'],
 ['skip-usb-reread','engine/src/workflow.rs','if format!("{:x}",hash.finalize()) != part.sha256','if false'],
 ['skip-final-identity','engine/src/workflow.rs','if after != identity','if false'],
 ['skip-saved-verification','engine/src/workflow.rs','verify_saved(&dir,&parts)?;','// removed saved-file check'],
 ['skip-loader-upload','engine/src/loader.rs','io.upload(e.area,&loader.bytes[e.offset..e.offset+e.size]).await?;','// removed RAM upload'],
 ['skip-gpt-copy-equality','engine/src/gpt.rs','if x!=y||a[56..72]!=b[56..72]||a[40..56]!=b[40..56]','if false'],
];
mkdirSync('test-results',{recursive:true});
const results=[];
function test(filter){return spawnSync(cargo,['test','--locked','-p','k11c-usb','--lib',...(filter?[filter]:[])],{env,encoding:'utf8',timeout:180000,maxBuffer:8*1024*1024});}
let baseline=test();writeFileSync('test-results/mutation-baseline.txt',baseline.stdout+baseline.stderr);if(baseline.status!==0)throw Error('Baseline failed: '+baseline.stdout+baseline.stderr);
for(const [name,file,before,after,target] of cases){
 if(process.argv.includes('--read-access-only')&&!name.startsWith('read-access-'))continue;
 if(process.argv.includes('--usb-only')&&!['engine/src/usb.rs','engine/src/readiness.rs'].includes(file))continue;
 const original=readFileSync(file,'utf8');if(!original.includes(before))throw Error(`Missing mutation target ${name}`);
 try{
   writeFileSync(file,original.replace(before,after),'utf8');
   const filter=target??(file==='engine/src/lib.rs'?'tests::digest_guard_uses_file_bytes':path.basename(file,'.rs')+'::tests::');
   const r=test(filter);const killed=r.status!==0&&/test result: FAILED/.test(r.stdout)&&!/could not compile/.test(r.stderr);
   results.push({name,killed,exit:r.status});writeFileSync(`test-results/mutation-${name}.txt`,r.stdout+r.stderr);
   console.log(`${name}: ${killed?'KILLED':'FAILED VERIFICATION'}`);if(!killed)throw Error(`Mutation survived or did not compile: ${name}`);
 }finally{writeFileSync(file,original,'utf8');}
}
baseline=test();writeFileSync('test-results/mutation-restored-baseline.txt',baseline.stdout+baseline.stderr);if(baseline.status!==0)throw Error('Restored baseline failed: '+baseline.stdout+baseline.stderr);
writeFileSync(process.argv.includes('--read-access-only')?'test-results/read-access-mutations.json':'test-results/mutations.json',JSON.stringify({results,restored_baseline:true},null,2));
console.log(`MUTATION_PASS=${results.length} RESTORED_BASELINE=PASS`);
