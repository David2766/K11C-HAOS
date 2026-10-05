import {spawnSync} from 'node:child_process';
import {readFileSync,existsSync,mkdirSync,copyFileSync,writeFileSync,readdirSync,statSync,chmodSync} from 'node:fs';
import {createHash} from 'node:crypto';
import path from 'node:path';

export function nativeInputs(root,env=process.env){
 const base=path.resolve(env.K11C_RELEASE_INPUTS??path.join(root,'resources'));
 const manifestPath=path.join(base,'native/tools.json');
 const bytes=readFileSync(manifestPath),manifestHash=createHash('sha256').update(bytes).digest('hex');
 const approved=readFileSync(path.join(root,'native/tools.sha256'),'utf8').trim();
 if(approved!==manifestHash)throw Error('Native manifest changed');
 const files=Object.entries(JSON.parse(bytes)).map(([name,expected])=>{
  if(path.basename(name)!==name||!/^[a-f0-9]{64}$/.test(expected))throw Error('Invalid native input');
  const file=path.join(base,'native',name),hash=createHash('sha256').update(readFileSync(file)).digest('hex');
  if(hash!==expected)throw Error('Native input changed: '+name);
  return {name,path:file,sha256:hash};
 });
 for(const name of ['debugfs.exe','e2fsck.exe','resize2fs.exe','cygwin1.dll'])if(!files.some(f=>f.name===name))throw Error('Missing native input: '+name);
 return {files,manifestPath,manifestHash};
}
export function buildNative(root,env){
 const inputs=nativeInputs(root,env);
 const go=env.K11C_GO??(env.K11C_BUILD_TOOLS?path.join(env.K11C_BUILD_TOOLS,'go-native/go/bin/go.exe'):'go');
 const dir=path.join(root,'resources/native');mkdirSync(dir,{recursive:true});
 for(const f of inputs.files)if(path.resolve(f.path)!==path.join(dir,f.name))copyFileSync(f.path,path.join(dir,f.name));
 if(path.resolve(inputs.manifestPath)!==path.join(dir,'tools.json'))copyFileSync(inputs.manifestPath,path.join(dir,'tools.json'));
 const exec=(args)=>{const r=spawnSync(go,args,{cwd:path.join(root,'native'),env:{...env,CGO_ENABLED:'0',GOOS:'windows',GOARCH:'amd64'},stdio:'inherit'});if(r.error)throw r.error;if(r.status!==0)throw Error('Native Go build failed');};
 exec(['test','-mod=readonly','./...']);
 exec(['build','-mod=readonly','-trimpath','-ldflags','-s -w -X main.toolsManifestHash='+inputs.manifestHash,'-o',path.join(dir,'k11c-seed.exe'),'.']);
 const modules=spawnSync(go,['list','-mod=readonly','-m','-f','{{.Dir}}|{{.Path}}|{{.Version}}','all'],{cwd:path.join(root,'native'),env,encoding:'utf8',maxBuffer:4*1024*1024});
 if(modules.status!==0)throw Error('Cannot collect native dependency notices');
 const notices=path.join(root,'notices/native/go');mkdirSync(notices,{recursive:true});
 for(const line of modules.stdout.trim().split(/\r?\n/)){
  const [source,name,version]=line.split('|');if(!source||!version)continue;
  for(const file of (awaitFiles(source)))if(/^(LICENSE|COPYING|NOTICE)([.-]|$)/i.test(file)){
   const target=path.join(notices,name.replaceAll('/','_')+'-'+version+'-'+file);
   if(existsSync(target))chmodSync(target,0o644);
   copyFileSync(path.join(source,file),target);chmodSync(target,0o644);
  }
 }
 writeFileSync(path.join(notices,'MODULES.txt'),modules.stdout.trim().split(/\r?\n/).map(line=>{
  const [,name,version]=line.split('|');return `${name} ${version||'(main module)'}`;
 }).join('\n')+'\n');
 env.K11C_NATIVE_SHA256=createHash('sha256').update(readFileSync(path.join(dir,'k11c-seed.exe'))).digest('hex');
 return inputs;
}
function awaitFiles(dir){return existsSync(dir)?readdirSync(dir).filter(name=>statSync(path.join(dir,name)).isFile()):[];}
