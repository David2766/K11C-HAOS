import {existsSync} from 'node:fs';
import path from 'node:path';

export function toolchain(input=process.env){
 const env={...input}, tools=env.K11C_BUILD_TOOLS;
 if(!tools)return {cargo:'cargo',env};
 const cargo=path.join(tools,'cargo/bin/cargo.exe');
 if(!existsSync(cargo))throw Error('K11C_BUILD_TOOLS has no cargo/bin/cargo.exe');
 env.CARGO_HOME=path.join(tools,'cargo');env.RUSTUP_HOME=path.join(tools,'rustup');
 env.Path=path.join(tools,'cargo/bin')+';'+(env.Path??env.PATH??'');
 return {cargo,env};
}
