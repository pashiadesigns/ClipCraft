import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve,sep} from 'node:path';
import {createSiteServer} from './self-host-site.mjs';
test('local website serves only snapshots and pinned releases; download ranges and atomic updates work',async()=>{
  const dir=mkdtempSync(resolve(tmpdir(),'clipcraft-local-test-')),stateRoot=resolve(dir,'state'),releaseRoot=resolve(dir,'releases');
  mkdirSync(resolve(stateRoot,'snapshots','a'),{recursive:true});mkdirSync(releaseRoot);
  writeFileSync(resolve(stateRoot,'snapshots','a','index.html'),'ClipCraft test');
  const file='clipcraft-0.1.0-alpha.1-windows-x64.zip';writeFileSync(resolve(releaseRoot,file),'0123456789');writeFileSync(resolve(dir,'secret.txt'),'not public');
  const current={snapshot:'a',latest:'0.1.0-alpha.1',releases:[{file,bytes:10,sha256:'a'.repeat(64)}]};
  writeFileSync(resolve(stateRoot,'current.json'),JSON.stringify(current));
  const server=createSiteServer({stateRoot,releaseRoot});await new Promise(r=>server.listen(0,'127.0.0.1',r));
  const origin='http://127.0.0.1:'+server.address().port;
  try {
    assert.equal(await (await fetch(origin)).text(),'ClipCraft test');
    const get=await fetch(origin+'/downloads/'+file);assert.equal(get.status,200);assert.equal(await get.text(),'0123456789');
    const head=await fetch(origin+'/downloads/'+file,{method:'HEAD'});assert.equal(head.headers.get('content-length'),'10');assert.equal(await head.text(),'');
    const range=await fetch(origin+'/downloads/'+file,{headers:{Range:'bytes=2-5'}});assert.equal(range.status,206);assert.equal(await range.text(),'2345');
    assert.equal((await fetch(origin+'/downloads/'+file,{headers:{Range:'bytes=99-'}})).status,416);
    for(const path of ['/../secret.txt','/%2e%2e%2fsecret.txt','/.openai/hosting.json','/downloads/unknown.zip','/downloads/%2e%2e%5csecret.txt'])assert.equal((await fetch(origin+path)).status,404,path);
    assert.equal((await fetch(origin,{method:'PUT',body:'change'})).status,405);
    mkdirSync(resolve(stateRoot,'snapshots','b'));writeFileSync(resolve(stateRoot,'snapshots','b','index.html'),'Next release');
    writeFileSync(resolve(stateRoot,'current.json'),JSON.stringify({...current,snapshot:'b'}));assert.equal(await (await fetch(origin)).text(),'Next release');
  } finally {
    server.closeAllConnections();await new Promise(r=>server.close(r));
    if(!dir.startsWith(resolve(tmpdir())+sep)||!dir.includes('clipcraft-local-test-'))throw new Error('Unsafe cleanup');rmSync(dir,{recursive:true,force:true});
  }
});
