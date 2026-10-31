import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, sep } from 'node:path';
import { createHash } from 'node:crypto';
import { validateReleases, renderRelease } from './release.mjs';

test('a release requires the exact downloadable artifact; corrupt and missing files fail', () => {
  const root = mkdtempSync(join(tmpdir(), 'capture-release-'));
  try {
    mkdirSync(join(root, 'dist/downloads'), {recursive:true});
    const bytes = Buffer.from('synthetic test artifact');
    const release = {version:'1.0.0-alpha.1',date:'2026-10-29',channel:'alpha',title:'Test',summary:'Verified artifact',changes:{Added:['Example']},limitations:[],asset:{file:'clipcraft-1.0.0-alpha.1-windows-x64.zip',bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')}};
    const path = join(root, 'dist/downloads', release.asset.file);
    writeFileSync(path, bytes);
    assert.equal(validateReleases({releases:[release]}, root).length, 1);
    writeFileSync(path, Buffer.from('Synthetic test artifact'));
    assert.throws(() => validateReleases({releases:[release]}, root), /checksum/);
    rmSync(path);
    assert.throws(() => validateReleases({releases:[release]}, root), /ENOENT/);
  } finally {
    if (!resolve(root).startsWith(resolve(tmpdir()) + sep) || !root.includes('capture-release-')) throw new Error('Unsafe test cleanup path.');
    rmSync(root, {recursive:true, force:true});
  }
});

test('release notes render as text, not executable markup', () => {
  const html = renderRelease({version:'1.0.0',date:'2026-10-29',title:'<script>alert(1)</script>',summary:'A & B',changes:{Added:['<img src=x onerror=alert(1)>']},limitations:[],asset:{sha256:'a'.repeat(64),file:'clipcraft-1.0.0-windows-x64.zip'}}, 0);
  assert.ok(!html.includes('<script>'));
  assert.ok(!html.includes('<img'));
  assert.ok(html.includes('&lt;script&gt;'));
  assert.ok(html.includes('A &amp; B'));
});
