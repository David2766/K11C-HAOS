import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {toolchain} from './toolchain.mjs';
const {cargo,env}=toolchain();
const cases=[
 ['exact-image','engine/src/components.rs','find(|s|s.official_raw_sha256==raw)','find(|_|true)'],
 ['reverse-image','engine/src/components.rs','find(|s|s.official_raw_sha256==raw)','find(|s|s.official_raw_sha256!=raw)'],
 ['boot-compatibility','engine/src/components.rs','||s.uboot_sha256!=self.boot.asset.sha256',''],
 ['boot-bytes','engine/src/components.rs','||crate::hash_bytes(&data)!=id',''],
 ['official-hash','engine/src/components.rs','if images::file_hash(&raw.path,job,"verify-official")?.1!=s.official_raw_sha256','if false'],
 ['download-hash','engine/src/components.rs','if h!=a.sha256{return Err(fail("COMPONENT_HASH","Component SHA-256 mismatch"));}',''],
 ['kernel-match','engine/src/maintenance.rs','s.haos==t.haos&&s.kernel==t.kernel','s.haos==t.haos'],
 ['confirmation','engine/src/maintenance.rs','if !confirmed{','if false{'],
 ['reverse-confirmation','engine/src/maintenance.rs','if !confirmed{','if confirmed{'],
 ['backup-contents','engine/src/maintenance.rs','if !list.iter().any(|a|a["slug"]==SLUG)','if false'],
 ['clean-removal','engine/src/maintenance.rs','json!({"remove_config":true})','json!({"remove_config":false})'],
 ['autostart','engine/src/maintenance.rs','"boot":"auto"','"boot":"manual"'],
 ['enabled','engine/src/maintenance.rs','defaults["enabled"]=json!(true);','defaults["enabled"]=json!(false);'],
 ['ready-verification','engine/src/maintenance.rs','if after["state"]!="started"||','if '],
 ['OS-preservation','engine/src/components.rs','out.write_all(&head).map_err(io)?;','out.write_all(&head).map_err(io)?;src.seek(SeekFrom::Start(0)).map_err(io)?;'],
];
// The last mutation must corrupt a payload, not be cancelled by a later seek.
cases[cases.length-1]=['OS-preservation','engine/src/components.rs','src.seek(SeekFrom::Start(head.len() as u64)).map_err(io)?;','src.seek(SeekFrom::Start(head.len() as u64+512)).map_err(io)?;'];
mkdirSync('test-results',{recursive:true});
const test=(scope)=>spawnSync(cargo,['test','--locked','-p','k11c-usb','--lib',scope,'--','--test-threads=2'],{env,encoding:'utf8',timeout:180000,maxBuffer:8*1024*1024});
for(const scope of ['components::','maintenance::']){const baseline=test(scope);if(baseline.status!==0)throw Error(baseline.stdout+baseline.stderr);}
const results=[];
for(const [name,file,before,after] of cases){
 const original=readFileSync(file,'utf8');if(!original.includes(before))throw Error('Missing target '+name);
 try{writeFileSync(file,original.replace(before,after),'utf8');const r=test(file.includes('maintenance')?'maintenance::':'components::');const killed=r.status!==0&&/test result: FAILED/.test(r.stdout)&&!/could not compile/.test(r.stderr);results.push({name,killed});writeFileSync('test-results/component-mutation-'+name+'.txt',r.stdout+r.stderr);console.log(name+': '+killed);if(!killed)throw Error('Mutation survived: '+name);}
 finally{writeFileSync(file,original,'utf8');}
}
for(const scope of ['components::','maintenance::']){if(test(scope).status!==0)throw Error('Restored baseline failed');}
writeFileSync('test-results/component-mutations.json',JSON.stringify({passed:true,results,restored:true},null,2));
