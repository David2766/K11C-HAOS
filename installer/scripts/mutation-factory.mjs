import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import assert from 'node:assert/strict';
import {toolchain} from './toolchain.mjs';
const root=path.resolve(import.meta.dirname,'..');process.chdir(root);
const {cargo,env}=toolchain();env.RUST_TEST_THREADS='2';
const profile=process.argv.includes('--debug')?'dev':'release';
const originals=Object.fromEntries(['factory.rs','usb.rs'].map(file=>[file,readFileSync('engine/src/'+file,'utf8')]));
const cases=[
 ['factory confirmation removed','factory.rs','if !confirmed{return Err(fail("CONFIRM_WRITE","Explicit confirmation required"));}','if false{return Err(fail("CONFIRM_WRITE","Explicit confirmation required"));}','confirmation_and_dispatch'],
 ['factory confirmation reversed','factory.rs','if !confirmed{return Err(fail("CONFIRM_WRITE","Explicit confirmation required"));}','if confirmed{return Err(fail("CONFIRM_WRITE","Explicit confirmation required"));}','confirmation_and_dispatch'],
 ['factory source binding removed','factory.rs','if image.sha256!=id||actual!=image','if false','source_and_receipt'],
 ['factory source binding reversed','factory.rs','if image.sha256!=id||actual!=image','if image.sha256==id||actual==image','source_and_receipt'],
 ['factory sparse capacity removed','factory.rs','total.checked_mul(block).is_none_or(|n|n>capacity)','false','sparse_expansion'],
 ['factory sparse capacity reversed','factory.rs','total.checked_mul(block).is_none_or(|n|n>capacity)','total.checked_mul(block).is_none_or(|n|n<=capacity)','sparse_expansion'],
 ['factory payload comparison removed','factory.rs','if expected[..len]!=actual[..len]','if false','every_vendor_payload'],
 ['factory payload comparison reversed','factory.rs','if expected[..len]!=actual[..len]','if expected[..len]==actual[..len]','every_vendor_payload'],
 ['factory layout production call removed','factory.rs','verify_layout(&a,&b,identity.sectors,image,file)','Ok(())','every_vendor_payload'],
 ['factory GPT validation removed','factory.rs','if crate::gpt::check(primary,tail,sectors)["healthy"]!=true','if false','every_vendor_payload'],
 ['factory GPT validation reversed','factory.rs','if crate::gpt::check(primary,tail,sectors)["healthy"]!=true','if crate::gpt::check(primary,tail,sectors)["healthy"]==true','every_vendor_payload'],
 ['factory partition count removed','factory.rs','if rows.len()!=image.partitions.len()','if false','grow_layout_still'],
 ['factory partition count reversed','factory.rs','if rows.len()!=image.partitions.len()','if rows.len()==image.partitions.len()','grow_layout_still'],
 ['factory partition name removed','factory.rs','if name!=p.name||start!=p.lba as u64||!end_matches','if start!=p.lba as u64||!end_matches','grow_layout_still'],
 ['factory partition name reversed','factory.rs','if name!=p.name||start!=p.lba as u64||!end_matches','if name==p.name||start!=p.lba as u64||!end_matches','grow_layout_still'],
 ['factory partition start removed','factory.rs','if name!=p.name||start!=p.lba as u64||!end_matches','if name!=p.name||!end_matches','grow_layout_still'],
 ['factory partition start reversed','factory.rs','if name!=p.name||start!=p.lba as u64||!end_matches','if name!=p.name||start==p.lba as u64||!end_matches','grow_layout_still'],
 ['factory fixed partition end removed','factory.rs','Some(count)=>end==p.lba as u64+count as u64-1','Some(_count)=>true','grow_layout_still'],
 ['factory fixed partition end reversed','factory.rs','Some(count)=>end==p.lba as u64+count as u64-1','Some(count)=>end!=p.lba as u64+count as u64-1','grow_layout_still'],
 ['factory grow alignment removed','factory.rs','end==full_end||end+1==((full_end+1)&!63)','end==full_end','grow_full_and_vendor_aligned'],
 ['factory grow alignment reversed','factory.rs','end==full_end||end+1==((full_end+1)&!63)','end==full_end||end+1!=((full_end+1)&!63)','grow_full_and_vendor_aligned'],
 ['factory grow arbitrary shortening allowed','factory.rs','end==full_end||end+1==((full_end+1)&!63)','end<=full_end','grow_layout_still'],
 ['factory grow extent validation removed','factory.rs','None=>i+1==image.partitions.len()&&(end==full_end||end+1==((full_end+1)&!63))','None=>true','grow_layout_still'],
 ['factory grow uses shortened header limit','factory.rs','let full_end=sectors as u64-34','let full_end=crate::gpt::u64at(primary,512+48)','grow_end_uses_physical'],
 ['factory actual payload capacity removed','factory.rs','verification(&actual,file,sectors).map_err','verification(image,file,sectors).map_err','grow_actual_capacity'],
 ['factory parameter CRC removed','factory.rs','if crc!=crate::gpt::u32at(bytes,8+len)','if false','rockchip_parameter_wrapper'],
 ['factory parameter CRC reversed','factory.rs','if crc!=crate::gpt::u32at(bytes,8+len)','if crc==crate::gpt::u32at(bytes,8+len)','rockchip_parameter_wrapper'],
 ['factory unique USB removed','usb.rs','if all.len()!=1','if false','vendor_target'],
 ['factory unique USB reversed','usb.rs','if all.len()!=1','if all.len()==1','vendor_target'],
];
function test(filter='factory::tests'){return spawnSync(cargo,['test','--profile',profile,'--locked','-p','k11c-usb','--lib',filter],{env,encoding:'utf8',timeout:300000,maxBuffer:8*1024*1024});}
mkdirSync('test-results',{recursive:true});let r=test();assert.equal(r.status,0,r.stdout+r.stderr);const results=[];
try{
 for(const[name,file,before,after,filter]of cases){
  const source=originals[file];assert.equal(source.split(before).length-1,1,name);
  writeFileSync('engine/src/'+file,source.replace(before,after));r=test('factory::tests::'+filter);writeFileSync('engine/src/'+file,source);
  const killed=r.status!==0&&/test result: FAILED/.test(r.stdout)&&!/could not compile/.test(r.stderr);
  results.push({name,killed});console.log(`${killed?'KILLED':'SURVIVED'}: ${name}`);if(!killed)throw Error(r.stdout+r.stderr);
 }
}finally{for(const[file,source]of Object.entries(originals))writeFileSync('engine/src/'+file,source);}
r=test();assert.equal(r.status,0,r.stdout+r.stderr);
writeFileSync('test-results/factory-mutations.json',JSON.stringify({results,restored_baseline:true,production_module:true,profile,usb_writes:false},null,2));
