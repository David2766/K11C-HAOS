import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
const go=process.env.K11C_GO??path.join(process.env.K11C_BUILD_TOOLS,'go-native/go/bin/go.exe');
const cases=[
 ['verified profile','main.go','!p.Verified ||',''],
 ['reverse verified','main.go','!p.Verified ||','p.Verified ||'],
 ['state digest','main.go','if hashBytes(canonical)!=p.StateHash','if hashBytes(canonical)==""'],
 ['enabled flag','main.go','||options["enabled"]!=true',''],
 ['image ID','image.go','||string(m.Config.Digest)!=p.ImageID',''],
 ['platform','image.go','c.Architecture!="arm64"||',''],
 ['GC children','image.go','images.SetChildrenLabels(cs,images.HandlerFunc(fetch))','images.HandlerFunc(fetch)'],
 ['tool hash','main.go','if actual!=h','if actual==h&&false'],
 ['repository config','repository.go','if hashBytes(config)!=p.ConfigHash','if hashBytes(config)==""'],
 ['repository kernel','repository.go','||release.HAOS[p.HAOS].Kernel!=p.Kernel',''],
 ['repository tracking','repository.go','||branch.Remote!="origin"',''],
];
const test=()=>spawnSync(go,['test','-mod=readonly','./...'],{cwd:'native',encoding:'utf8',timeout:120000,maxBuffer:4*1024*1024});
const baseline=test();if(baseline.status!==0)throw Error('Native baseline failed\n'+baseline.stdout+baseline.stderr);mkdirSync('test-results',{recursive:true});
const results=[];
for(const [name,file,from,to] of cases){const p='native/'+file,source=readFileSync(p,'utf8');if(!source.includes(from))throw Error('Missing mutation '+name);
 try{writeFileSync(p,source.replace(from,to),'utf8');const r=test();const killed=r.status!==0&&/--- FAIL: Test/.test(r.stdout+r.stderr)&&!/build failed/.test(r.stdout+r.stderr);results.push({name,killed});if(!killed)throw Error(name+' survived\n'+r.stdout+r.stderr);console.log('NATIVE_MUTATION_REJECTED='+name)}finally{writeFileSync(p,source,'utf8')}}
if(test().status!==0)throw Error('Native restored baseline failed');writeFileSync('test-results/native-mutations.json',JSON.stringify({passed:true,results,restored:true},null,2));
