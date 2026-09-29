import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');process.chdir(root);
import {toolchain} from './toolchain.mjs';
const {cargo,env}=toolchain();
const cases=[
 ['reuse-driver','engine/src/driver.rs','!installed && !attempted','!attempted'],
 ['reverse-reuse','engine/src/driver.rs','!installed && !attempted','installed && !attempted'],
 ['skip-bundle-check','engine/src/driver.rs','host.verify()?;','// verification removed by test'],
 ['accept-false-success','engine/src/driver.rs','if !after.installed','if false'],
 ['skip-file-digest','engine/src/lib.rs','if hash_bytes(&bytes) != expected','if false'],
 ['reverse-file-digest','engine/src/lib.rs','if hash_bytes(&bytes) != expected','if hash_bytes(&bytes) == expected'],
 ['allow-maskrom-read','engine/src/usb.rs','if device.mode != "Loader"','if false'],
 ['allow-unbound-read','engine/src/usb.rs','if !device.binding','if false'],
 ['allow-wrong-storage','engine/src/usb.rs','if storage != "emmc"','if false'],
 ['reverse-storage','engine/src/usb.rs','if storage != "emmc"','if storage == "emmc"'],
 ['skip-loader-digest','engine/src/loader.rs','if hash_bytes(&bytes) != SHA256','if false'],
 ['reverse-loader-digest','engine/src/loader.rs','if hash_bytes(&bytes) != SHA256','if hash_bytes(&bytes) == SHA256'],
 ['skip-board-confirmation','engine/src/usb.rs','if !confirmed','if false'],
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
function test(){return spawnSync(cargo,['test','--locked','-p','k11c-usb','--lib'],{env,encoding:'utf8',timeout:180000,maxBuffer:8*1024*1024});}
let baseline=test();if(baseline.status!==0)throw Error('Baseline failed: '+baseline.stdout+baseline.stderr);
for(const [name,file,before,after] of cases){
 const original=readFileSync(file,'utf8');if(!original.includes(before))throw Error(`Missing mutation target ${name}`);
 try{
   writeFileSync(file,original.replace(before,after),'utf8');
   const r=test();const killed=r.status!==0&&/test result: FAILED/.test(r.stdout)&&!/could not compile/.test(r.stderr);
   results.push({name,killed,exit:r.status});writeFileSync(`test-results/mutation-${name}.txt`,r.stdout+r.stderr);
   console.log(`${name}: ${killed?'KILLED':'FAILED VERIFICATION'}`);if(!killed)throw Error(`Mutation survived or did not compile: ${name}`);
 }finally{writeFileSync(file,original,'utf8');}
}
baseline=test();if(baseline.status!==0)throw Error('Restored baseline failed');
writeFileSync('test-results/mutations.json',JSON.stringify({results,restored_baseline:true},null,2));
console.log(`MUTATION_PASS=${results.length} RESTORED_BASELINE=PASS`);
