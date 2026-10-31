import test from 'node:test';
import assert from 'node:assert/strict';
import {dependencyInventory,sha256} from './release-metadata.mjs';
test('SPDX includes only reachable Windows dependencies and records lockfile hashes',()=>{
  const meta={workspace_members:['root'],packages:[{id:'root',name:'app',version:'1',license:'Apache-2.0'},{id:'dep',name:'dep',version:'2',license:'MIT',source:'registry+test'},{id:'other',name:'unused',version:'3'}],resolve:{nodes:[{id:'root',deps:[{pkg:'dep'}]},{id:'dep',deps:[]}]}};
  const hash='a'.repeat(64),result=dependencyInventory(meta,'[[package]]\nname = "dep"\nversion = "2"\nchecksum = "'+hash+'"\n',{version:'0.1.0-preview.1',commit:'b'.repeat(40),created:'2026-10-31T00:00:00Z'});
  assert.equal(result.packages.length,2);
  assert.equal(result.spdxVersion,'SPDX-2.3');
  assert.equal(typeof result.comment,'string');
  assert.ok(!('documentComment' in result)); // SPDX 2.3 rejects this misspelled field.
  assert.equal(result.packages[1].checksums[0].checksumValue,hash);
  const ids=new Set(['SPDXRef-DOCUMENT',...result.packages.map(p=>p.SPDXID)]);
  for(const r of result.relationships){assert.ok(ids.has(r.spdxElementId));assert.ok(ids.has(r.relatedSpdxElement));}
  assert.equal(result.relationships.filter(r=>r.relationshipType==='DEPENDS_ON').length,1);
  assert.equal(result.packages[1].externalRefs[0].referenceLocator,'pkg:cargo/dep@2');
  assert.equal(sha256(Buffer.from('abc')),'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
});
