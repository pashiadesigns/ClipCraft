import test from 'node:test';
import assert from 'node:assert/strict';
import {auditFile} from './source-audit.mjs';
test('private captures, credentials and executable output cannot enter the source import',()=>{
  for(const path of ['artifacts/capture.png','.codex/settings.json','.openai/hosting.json','x/.env','target/app.exe','key.pfx','sites/a/release-assets/a.zip'])
    assert.ok(auditFile(path,Buffer.from('sample')).length,path);
});
test('recognized secrets are rejected without printing their values',()=>{
  const token='gh'+'p_'+'x'.repeat(36);
  assert.deepEqual(auditFile('config.txt',Buffer.from(token)),['credential pattern']);
  const url=['https:','//','name',':','secret','@','example.invalid'].join('');
  assert.deepEqual(auditFile('config.txt',Buffer.from(url)),['credential pattern']);
});
test('synthetic fixtures and redistributable input assets are allowed',()=>{
  for(const path of ['apps/desktop/src/main.rs','crates/core/tests/fixtures/demo.capture.json','apps/desktop/assets/brand/clipcraft.res','licenses/designcraft/LICENSE-MIT'])
    assert.deepEqual(auditFile(path,Buffer.from('original synthetic content')),[]);
});
