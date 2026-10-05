import { invoke, isTauri } from '@tauri-apps/api/core';
export type Reply = {ok:boolean;data?:any;error?:{code:string;detail:string};history_warning?:unknown};
export type ImageRelease = {version:string;filename:string;size:number;sha256:string|null;published:string};
export type PreparedImage = {path:string;filename:string;version:string|null;platform:string;bytes:number;sha256:string;source_sha256:string;verification:string;partitions:number;reused_download:boolean;reused_image:boolean;connectivity?:string;uboot?:string;catalog_source?:string;kernel?:string};
export type ImageProgress = {phase:string;completed:number;total:number|null};
export const native = isTauri();
export async function call(command:string,args:Record<string,unknown>={}):Promise<Reply> {
  // Browser preview is explicitly disconnected. Never invent successful devices.
  if (!native) return {ok:false,error:{code:'PREVIEW_ONLY',detail:'Browser preview has no PC or USB access.'}};
  try { return await invoke<Reply>(command,args); }
  catch(e) { return {ok:false,error:{code:'IPC_ERROR',detail:String(e)}}; }
}
