import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createHandler } from '../worker/handler.mjs';

const file='clipcraft-0.1.0-alpha.1-windows-x64.zip',bytes=new TextEncoder().encode('synthetic immutable ZIP bytes');
const entry={file,bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')};
function fixture() {
  const records=new Map(); let writes=0;
  const bucket={
    async head(key) { const item=records.get(key); return item ? {size:item.bytes.length,customMetadata:item.metadata} : null; },
    async put(key,body,options) {
      assert.equal(options.onlyIf.get('If-None-Match'),'*');
      assert.equal(options.sha256,entry.sha256);
      if (records.has(key)) return null;
      writes++; records.set(key,{bytes:body.slice(),metadata:options.customMetadata}); return this.head(key);
    },
    async get(key,options) {
      const item=records.get(key); if(!item)return null;
      const body=options?.range ? item.bytes.slice(options.range.offset,options.range.offset+options.range.length) : item.bytes;
      return {body};
    }
  };
  const handler=createHandler({'/index.html':{type:'text/html',base64:Buffer.from('<h1>ClipCraft</h1>').toString('base64')}},[entry]);
  const request=(method='GET',body,headers={},path='/api/release-files/'+file)=>handler.fetch(new Request('https://test.invalid'+path,{method,body,headers}),{BUCKET:bucket});
  const upload=(body=bytes,headers={})=>request('PUT',body,{'Content-Type':'application/zip','Content-Length':String(body.length),...headers});
  return {request,upload,records,get writes(){return writes;},handler,bucket};
}
test('immutable upload roundtrip, HEAD, ranges, cache condition and idempotence',async()=>{
  const f=fixture(); assert.equal((await f.upload()).status,201);
  assert.equal((await f.upload()).status,200); assert.equal(f.writes,1);
  const get=await f.request('GET',undefined,{},'/downloads/'+file);
  assert.equal(get.headers.get('X-ClipCraft-Storage'),'r2'); assert.equal(get.headers.get('X-ClipCraft-SHA256'),entry.sha256);
  assert.deepEqual(new Uint8Array(await get.arrayBuffer()),bytes);
  const head=await f.request('HEAD'); assert.equal(head.headers.get('Content-Length'),String(bytes.length)); assert.equal(await head.text(),'');
  for(const [range,start,end] of [['bytes=2-5',2,6],['bytes=-4',bytes.length-4,bytes.length],['bytes=4-',4,bytes.length]]) {
    const r=await f.request('GET',undefined,{'Range':range}); assert.equal(r.status,206);assert.deepEqual(new Uint8Array(await r.arrayBuffer()),bytes.slice(start,end));
  }
  assert.equal((await f.request('GET',undefined,{'Range':'bytes=999-1000'})).status,416);
  assert.equal((await f.request('GET',undefined,{'Range':'bytes=0-1,3-4'})).status,416);
  assert.equal((await f.request('GET',undefined,{'If-None-Match':`"${entry.sha256}"`})).status,304);
  assert.equal((await f.request('GET',undefined,{'Range':'bytes=2-5','If-Range':'"different"'})).status,200);
});
test('reject corrupt, short, cross-origin, unknown and destructive requests without changing objects',async()=>{
  const f=fixture();const corrupt=bytes.slice();corrupt[0]^=1;
  assert.equal((await f.upload(corrupt)).status,422);
  assert.equal((await f.upload(bytes.slice(1))).status,400);
  assert.equal((await f.upload(bytes,{'Origin':'https://other.invalid'})).status,403);
  assert.equal((await f.request('PUT',bytes,{},'/api/release-files/unknown.zip')).status,404);
  assert.equal((await f.request('DELETE')).status,405);assert.equal(f.writes,0);
  await f.upload();f.records.get('releases/'+file).metadata.sha256='0'.repeat(64);
  assert.equal((await f.upload()).status,409);assert.equal((await f.request()).status,409);assert.equal(f.writes,1);
});
test('migration fallback and embedded website preserve existing routes',async()=>{
  const f=fixture();
  const fallback=await f.handler.fetch(new Request('https://test.invalid/downloads/'+file),{BUCKET:f.bucket,ASSETS:{fetch:()=>new Response('old immutable ZIP')}});
  assert.equal(await fallback.text(),'old immutable ZIP');
  assert.equal(await (await f.request('GET',undefined,{},'/')).text(),'<h1>ClipCraft</h1>');
  assert.equal((await f.request('POST',undefined,{},'/')).status,405);
});
