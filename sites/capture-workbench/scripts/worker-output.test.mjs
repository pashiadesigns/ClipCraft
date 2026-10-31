import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,readdirSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import worker from '../dist/server/index.js';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
test('built Worker returns every generated page and original image byte-for-byte',async()=>{
  for (const dir of ['','assets','downloads']) for(const file of readdirSync(resolve(root,'preview',dir),{withFileTypes:true})) {
    if (!file.isFile()) continue;
    const path=[dir,file.name].filter(Boolean).join('/');
    const response=await worker.fetch(new Request('https://test.invalid/'+path),{});
    assert.equal(response.status,200,path);
    assert.deepEqual(Buffer.from(await response.arrayBuffer()),readFileSync(resolve(root,'preview',path)),path);
  }
});
