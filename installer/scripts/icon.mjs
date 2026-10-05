import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {Resvg} from '@resvg/resvg-js';
// Shared vector source for the UI, favicon and multi-resolution Windows icon.
const svg=readFileSync(new URL('../public/app-icon.svg',import.meta.url));
const sizes=[16,20,24,32,40,48,64,128,256];
const images=sizes.map(size=>new Resvg(svg,{fitTo:{mode:'width',value:size},font:{loadSystemFonts:false}}).render().asPng());
const header=Buffer.alloc(6+16*sizes.length);
header.writeUInt16LE(1,2);header.writeUInt16LE(sizes.length,4);
let offset=header.length;
for(let i=0;i<sizes.length;i++){
 const entry=6+16*i;header[entry]=header[entry+1]=sizes[i]===256?0:sizes[i];
 header.writeUInt16LE(1,entry+4);header.writeUInt16LE(32,entry+6);
 header.writeUInt32LE(images[i].length,entry+8);header.writeUInt32LE(offset,entry+12);offset+=images[i].length;
}
mkdirSync(new URL('../src-tauri/icons/',import.meta.url),{recursive:true});
writeFileSync(new URL('../src-tauri/icons/icon.ico',import.meta.url),Buffer.concat([header,...images]));
console.log('ICON_READY sizes='+sizes.join(','));
