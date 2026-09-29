import {mkdirSync,writeFileSync} from 'node:fs';
// Original code-drawn app icon. No external image or font dependency.
const n=64,bmp=Buffer.alloc(40+n*n*4+n*n/8);
bmp.writeUInt32LE(40,0);bmp.writeInt32LE(n,4);bmp.writeInt32LE(n*2,8);bmp.writeUInt16LE(1,12);bmp.writeUInt16LE(32,14);
for(let y=0;y<n;y++)for(let x=0;x<n;x++){
 let rgb=[23,48,43,255];
 const edgeX=Math.max(0,12-x,x-51),edgeY=Math.max(0,12-y,y-51);
 if(edgeX*edgeX+edgeY*edgeY>144)rgb=[0,0,0,0];
 const frame=(x>=18&&x<=45&&y>=18&&y<=45)&&(x<21||x>42||y<21||y>42);
 const core=(x>=26&&x<=37&&y>=26&&y<=37)&&(x<29||x>34||y<29||y>34);
 const pin=([23,31,39].some(i=>Math.abs(x-i)<2)&&(y>=12&&y<=19||y>=44&&y<=51))||([23,31,39].some(i=>Math.abs(y-i)<2)&&(x>=12&&x<=19||x>=44&&x<=51));
 if(frame||core||pin)rgb=[112,224,195,255];
 const off=40+((n-1-y)*n+x)*4;bmp[off]=rgb[2];bmp[off+1]=rgb[1];bmp[off+2]=rgb[0];bmp[off+3]=rgb[3];
}
const ico=Buffer.alloc(22);ico.writeUInt16LE(1,2);ico.writeUInt16LE(1,4);ico[6]=n;ico[7]=n;ico.writeUInt16LE(1,10);ico.writeUInt16LE(32,12);ico.writeUInt32LE(bmp.length,14);ico.writeUInt32LE(22,18);
mkdirSync('src-tauri/icons',{recursive:true});writeFileSync('src-tauri/icons/icon.ico',Buffer.concat([ico,bmp]));
