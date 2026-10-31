// Credentials arrive only on hidden stdin and are never persisted or logged.
import {readFileSync,writeFileSync,existsSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const site=resolve(root,'sites/capture-workbench');
const origin='https://clipcraft.chirpy-wolf-3948.chatgpt.site';
if (!process.stdin.isTTY) throw new Error('Use a hidden interactive stdin session');
process.stdin.setRawMode(true);process.stdin.resume();process.stdin.setEncoding('utf8');
console.log('Ready for Site service JSON on stdin (input is hidden).');
const input=await new Promise((resolve,reject)=>{
  let text='';const listener=chunk=>{
    if(chunk.includes('\x03')) {reject(new Error('Cancelled'));return;}
    text+=chunk;
    if(text.includes('\n')||text.includes('\r')) {
      process.stdin.off('data',listener);process.stdin.setRawMode(false);process.stdin.pause();
      try{resolve(JSON.parse(text.trim()));}catch{reject(new Error('Invalid service JSON'));}
    }
  };process.stdin.on('data',listener);
});
if(typeof input.token!=='string'||!input.token)throw new Error('Missing Site service credential');
const request=(path,options={})=>fetch(origin+path,{...options,redirect:'error',headers:{...options.headers,'OAI-Sites-Authorization':'Bearer '+input.token}});
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const feed=JSON.parse(readFileSync(resolve(site,'release-data.json'),'utf8').replace(/^\uFEFF/,''));
const staged=JSON.parse(readFileSync(resolve(site,'upload-catalog.json'),'utf8'));
const catalog=[...new Map([...feed.releases.map(r=>r.asset),...staged].map(a=>[a.file,a])).values()];
const health=await request('/api/release-storage');
if(!health.ok||!health.headers.get('content-type')?.includes('application/json'))throw new Error(`Storage service unavailable (${health.status})`);
const state=await health.json();console.log(JSON.stringify(state));
if(!state.bucket)throw new Error('Missing bucket binding');
if(input.mode==='probe') {
  for(const asset of [feed.releases[0].asset,feed.releases.at(-1).asset]) {
    const r=await request('/downloads/'+asset.file);
    console.log(JSON.stringify({file:asset.file,status:r.status,type:r.headers.get('content-type'),length:r.headers.get('content-length')}));
    if(r.status!==200)throw new Error(`Historical download failed (${asset.file}, ${r.status})`);
    const downloaded=new Uint8Array(await r.arrayBuffer());
    if(downloaded.length!==asset.bytes||hash(downloaded)!==asset.sha256)throw new Error('Historical byte mismatch: '+asset.file);
    console.log(`Historical route checksum verified: ${asset.file}`);
  }
} else if(input.mode==='upload') {
  const verified=[];
  for(const asset of catalog) {
    if(!/^clipcraft-\d+\.\d+\.\d+(?:-[a-z0-9.]+)?-windows-x64\.zip$/.test(asset.file)||asset.file.includes('..'))throw new Error('Invalid filename');
    const path=resolve(site,'release-assets',asset.file);
    const version=asset.file.slice('clipcraft-'.length,-'-windows-x64.zip'.length);
    const local=existsSync(path)?path:resolve(root,'artifacts/releases',version,asset.file);
    const bytes=readFileSync(local);
    if(bytes.length!==asset.bytes||hash(bytes)!==asset.sha256)throw new Error('Local artifact mismatch: '+asset.file);
    const route='/api/release-files/'+asset.file;
    const head=await request(route,{method:'HEAD'});
    if(head.status!==200) {
      if(head.status!==404)throw new Error(`Unexpected storage state ${head.status}`);
      const put=await request(route,{method:'PUT',body:bytes,headers:{'Content-Type':'application/zip','Content-Length':String(bytes.length)}});
      if(![200,201].includes(put.status))throw new Error(`Upload failed (${asset.file}, ${put.status})`);
      await put.body?.cancel();
    }
    const get=await request(route);
    if(get.status!==200||get.headers.get('X-ClipCraft-Storage')!=='r2')throw new Error(`Durable read failed (${asset.file}, ${get.status})`);
    const downloaded=new Uint8Array(await get.arrayBuffer());
    if(downloaded.length!==asset.bytes||hash(downloaded)!==asset.sha256)throw new Error('Remote checksum mismatch: '+asset.file);
    verified.push({...asset,verified_at:new Date().toISOString()});
    writeFileSync(resolve(root,'artifacts/site/durable-download-checks.json'),JSON.stringify({origin,verified},null,2)+'\n');
    if(verified.length%8===0||verified.length===catalog.length)console.log(`Verified ${verified.length}/${catalog.length} immutable downloads`);
  }
  const storage=JSON.parse(readFileSync(resolve(site,'release-storage.json'),'utf8'));
  writeFileSync(resolve(site,'release-storage.json'),JSON.stringify({...storage,verified},null,2)+'\n');
} else if(input.mode==='verify') {
  for(const asset of feed.releases.map(r=>r.asset)) {
    const get=await request('/downloads/'+asset.file);
    if(get.status!==200||get.headers.get('X-ClipCraft-Storage')!=='r2')throw new Error(`Published route is not durable (${asset.file}, ${get.status})`);
    const downloaded=new Uint8Array(await get.arrayBuffer());
    if(downloaded.length!==asset.bytes||hash(downloaded)!==asset.sha256)throw new Error('Published route checksum mismatch: '+asset.file);
  }
  const result={verified_at:new Date().toISOString(),origin,published_downloads:feed.releases.length,passed:true};
  writeFileSync(resolve(root,'artifacts/site/published-durable-downloads.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
} else throw new Error('Unknown service operation');
