import {readFileSync,writeFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
process.chdir(path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'));
import {toolchain} from './toolchain.mjs';
const {cargo,env}=toolchain();
const file='src-tauri/src/main.rs',source=readFileSync(file,'utf8');
function test(){return spawnSync(cargo,['test','--locked','-p','k11c-installer','image_action_waits_for_poll_then_owns_lock'],{env,encoding:'utf8',timeout:240000,maxBuffer:8*1024*1024});}
let r=test();assert.equal(r.status,0,r.stdout+r.stderr);
try{
 assert.equal(source.split('Ok(self.busy.lock().await)').length,2);
 writeFileSync(file,source.replace('Ok(self.busy.lock().await)','self.busy.try_lock().map_err(|_|fail("DEVICE_BUSY","poll"))'));
 r=test();assert.ok(r.status!==0&&/test result: FAILED/.test(r.stdout)&&!/could not compile/.test(r.stderr),r.stdout+r.stderr);
}finally{writeFileSync(file,source);}
r=test();assert.equal(r.status,0,r.stdout+r.stderr);
writeFileSync('test-results/gui-lock-mutation.json',JSON.stringify({passed:true,mutation:'reject instead of waiting for background poll',killed:true,restored_baseline:true},null,2));
console.log('GUI_LOCK_MUTATION_PASS');
