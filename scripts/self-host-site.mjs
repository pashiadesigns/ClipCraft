import {createServer} from 'node:http';
import {createReadStream,readFileSync,statSync} from 'node:fs';
import {resolve,dirname,extname,sep} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const types={'.html':'text/html; charset=utf-8','.css':'text/css; charset=utf-8','.js':'text/javascript; charset=utf-8','.json':'application/json','.txt':'text/plain; charset=utf-8','.png':'image/png','.jpg':'image/jpeg','.svg':'image/svg+xml','.zip':'application/zip'};
export function createSiteServer({stateRoot=resolve(root,'artifacts/selfhost'),releaseRoot=resolve(root,'sites/capture-workbench/release-assets')}={}) {
  return createServer((req,res)=>{
    const end=(status,text='',headers={})=>{res.writeHead(status,{'Cache-Control':'no-store','X-Content-Type-Options':'nosniff',...headers});res.end(req.method==='HEAD'?undefined:text);};
    if (!['GET','HEAD'].includes(req.method)) {end(405,'Method not allowed',{'Allow':'GET, HEAD'});return;}
    try {
      const current=JSON.parse(readFileSync(resolve(stateRoot,'current.json'),'utf8'));
      if(!/^[a-z0-9-]+$/.test(current.snapshot))throw new Error('Invalid snapshot');
      const path=decodeURIComponent(new URL(req.url,'http://localhost').pathname);
      if(path==='/__clipcraft/status') {end(200,JSON.stringify({service:'ClipCraft website',latest:current.latest,published_at:current.published_at,hosting:'this-pc'}),{'Content-Type':'application/json'});return;}
      const snapshot=resolve(stateRoot,'snapshots',current.snapshot);
      let file,asset;
      if(path.startsWith('/downloads/')&&path.endsWith('.zip')) {
        const name=path.slice('/downloads/'.length);
        asset=current.releases.find(a=>a.file===name);
        if(!asset||name.includes('/')||name.includes('\\')||name.includes('..')) {end(404,'Not found');return;}
        file=resolve(releaseRoot,name);
      } else {
        file=resolve(snapshot,'.'+(path==='/'?'/index.html':path));
        if(!file.startsWith(snapshot+sep)||path.includes('\\')||path.split('/').some(p=>p.startsWith('.'))) {end(404,'Not found');return;}
      }
      const stat=statSync(file);
      if(!stat.isFile()||(asset&&stat.size!==asset.bytes)) {end(404,'Not found');return;}
      const headers={'Content-Type':types[extname(file)]??'application/octet-stream','Content-Length':String(stat.size),'Cache-Control':'no-cache','X-Content-Type-Options':'nosniff','Accept-Ranges':'bytes'};
      if(asset) {headers.ETag=`"${asset.sha256}"`;headers['X-ClipCraft-SHA256']=asset.sha256;headers['Content-Disposition']=`attachment; filename="${asset.file}"`;}
      if(asset&&req.headers['if-none-match']===headers.ETag) {end(304,'',{'ETag':headers.ETag});return;}
      let start=0,endByte=stat.size-1,status=200;
      if(req.headers.range&&(!req.headers['if-range']||req.headers['if-range']===headers.ETag)) {
        const match=/^bytes=(\d*)-(\d*)$/.exec(req.headers.range);
        if(!match||(!match[1]&&!match[2])) {end(416,'',{'Content-Range':`bytes */${stat.size}`});return;}
        start=match[1]?Number(match[1]):Math.max(0,stat.size-Number(match[2]));
        endByte=match[1]&&match[2]?Math.min(stat.size-1,Number(match[2])):stat.size-1;
        if(!Number.isSafeInteger(start)||!Number.isSafeInteger(endByte)||start>endByte||start>=stat.size) {end(416,'',{'Content-Range':`bytes */${stat.size}`});return;}
        status=206;headers['Content-Range']=`bytes ${start}-${endByte}/${stat.size}`;headers['Content-Length']=String(endByte-start+1);
      }
      res.writeHead(status,headers);
      if(req.method==='HEAD'||stat.size===0) {res.end();return;}
      const stream=createReadStream(file,{start,end:endByte});
      stream.on('error',()=>res.destroy());res.on('close',()=>stream.destroy());stream.pipe(res);
    } catch {end(404,'Not found');}
  });
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  const host=process.argv[2]??'127.0.0.1',port=Number(process.argv[3]??4173);
  if(!['127.0.0.1','0.0.0.0'].includes(host)||!Number.isInteger(port)||port<1024||port>65535)throw new Error('Invalid host or port');
  createSiteServer().listen(port,host,()=>console.log(`ClipCraft self-host listening on ${host}:${port}`));
}
