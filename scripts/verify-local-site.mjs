import {readFileSync,writeFileSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const tunnel=JSON.parse(readFileSync(resolve(root,'artifacts/selfhost/tunnel.json'),'utf8').replace(/^\uFEFF/,''));
const origin=tunnel.url;
if(!/^https:\/\/[a-z0-9-]+\.trycloudflare\.com$/.test(origin))throw new Error('Unknown tunnel origin');
const current=JSON.parse(readFileSync(resolve(root,'artifacts/selfhost/current.json'),'utf8'));
const get=path=>fetch(origin+path,{redirect:'error',signal:AbortSignal.timeout(30000)});
const health=await get('/__clipcraft/status');if(!health.ok||(await health.json()).latest!==current.latest)throw new Error('Public version mismatch');
const html=await get('/');if(!html.ok||!(await html.text()).includes(current.latest))throw new Error('Public page mismatch');
const roadmap=await get('/roadmap.json');if(!roadmap.ok||!(await roadmap.json()).features.length)throw new Error('Public roadmap missing');
let total=0;const verified=[];
for(const asset of current.releases){
  const response=await get('/downloads/'+asset.file);if(!response.ok)throw new Error('Download failed: '+asset.file);
  let bytes=0;const hash=createHash('sha256');
  for await(const chunk of response.body){bytes+=chunk.length;hash.update(chunk);}
  if(bytes!==asset.bytes||hash.digest('hex')!==asset.sha256)throw new Error('Download mismatch: '+asset.file);
  total+=bytes;verified.push(asset.file);
}
const range=await fetch(origin+'/downloads/'+current.releases[0].file,{headers:{Range:'bytes=0-15'},redirect:'error',signal:AbortSignal.timeout(30000)});
if(range.status!==206||(await range.arrayBuffer()).byteLength!==16)throw new Error('Public range request failed');
const result={passed:true,url:origin,latest:current.latest,verified_at:new Date().toISOString(),snapshot:current.snapshot,downloads:verified.length,download_bytes:total,all_checksums_match:true,range:true};
writeFileSync(resolve(root,'artifacts/selfhost/public-checks.json'),JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify(result));
