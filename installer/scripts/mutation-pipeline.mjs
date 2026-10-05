import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import assert from 'node:assert/strict';
import {toolchain} from './toolchain.mjs';
process.chdir(path.resolve(import.meta.dirname,'..'));
const {cargo,env}=toolchain();env.RUST_TEST_THREADS='2';
const file='engine/src/pipeline.rs',original=readFileSync(file,'utf8');
const cases=[
 ['block bounds/order removed','if block.offset != offset || block.len == 0 || block.len != expected || block.data.len() != CHUNK','if false','hash_missing'],
 ['block bounds/order reversed','if block.offset != offset || block.len == 0 || block.len != expected || block.data.len() != CHUNK','if !(block.offset != offset || block.len == 0 || block.len != expected || block.data.len() != CHUNK)','hash_exact'],
 ['source hashing removed','hash.update(&data[..len]);','/* omitted */','source_exact'],
 ['readback hashing removed','hash.update(&block.data[..block.len]);','/* omitted */','hash_exact'],
 ['source order changed','Ok(Block { offset, len, data })','Ok(Block { offset: offset + 512, len, data })','source_exact'],
 ['pool bound weakened','const BUFFERS: usize = 3','const BUFFERS: usize = 4','source_prepares'],
];
function test(filter='pipeline'){return spawnSync(cargo,['test','--release','--locked','-p','k11c-usb','--lib',filter],{env,encoding:'utf8',timeout:180000,maxBuffer:8*1024*1024});}
let r=test();assert.equal(r.status,0,r.stdout+r.stderr);const results=[];
try{
 for(const[name,before,after,filter] of cases){
  assert.equal(original.split(before).length-1,1,name);writeFileSync(file,original.replace(before,after));r=test('pipeline::tests::'+filter);writeFileSync(file,original);
  const killed=r.status!==0&&/test result: FAILED/.test(r.stdout)&&!/could not compile/.test(r.stderr);results.push({name,killed});console.log(`${killed?'KILLED':'SURVIVED'}: ${name}`);assert.ok(killed,r.stdout+r.stderr);
 }
}finally{writeFileSync(file,original);}
r=test();assert.equal(r.status,0,r.stdout+r.stderr);mkdirSync('test-results',{recursive:true});writeFileSync('test-results/pipeline-mutations.json',JSON.stringify({results,restored_baseline:true},null,2));
