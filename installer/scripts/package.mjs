import {spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync,copyFileSync,existsSync} from 'node:fs';
import {createHash} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');process.chdir(root);
const folder='K11C-Installer-'+JSON.parse(readFileSync('package.json','utf8')).version;const release=path.join(root,'release');const out=path.join(release,folder);
for(const f of ['README.md','VERIFICATION.md','THIRD-PARTY.md','api-contract.md','Cargo.lock','package-lock.json'])copyFileSync(f,path.join(out,f));
const uiCheck=spawnSync(process.execPath,['scripts/test-ui.mjs'],{stdio:'inherit'});if(uiCheck.status!==0)throw Error('UI verification failed');
const check=spawnSync(process.execPath,['scripts/verify.mjs'],{stdio:'inherit'});if(check.status!==0)throw Error('Packaged verification failed');
const names=['k11c-installer.exe','k11c-usb.exe','resources','notices','README.md','VERIFICATION.md','THIRD-PARTY.md','api-contract.md','Cargo.lock','package-lock.json','BUILD.json'];
const archive=path.join(release,folder+'-windows-x64.zip');
// Explicit payload list excludes local data, WebView profile, and test history.
const tar=path.join(process.env.SystemRoot,'System32/tar.exe');if(!existsSync(tar))throw Error('Windows tar.exe is required on the build PC');
const result=spawnSync(tar,['-a','-cf',archive,'-C',release,...names.map(n=>folder+'/'+n)],{stdio:'inherit'});
if(result.status!==0)throw Error('ZIP creation failed');
const sha=createHash('sha256').update(readFileSync(archive)).digest('hex');
writeFileSync(archive+'.sha256',`${sha}  ${path.basename(archive)}\n`);console.log(`ZIP=${archive}\nSHA256=${sha}`);
