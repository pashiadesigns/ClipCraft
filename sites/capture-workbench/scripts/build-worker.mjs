import { readFileSync, writeFileSync, readdirSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { resolve, dirname, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
const root = resolve(dirname(fileURLToPath(import.meta.url)),'..');
const types = {'.html':'text/html; charset=utf-8','.css':'text/css; charset=utf-8','.js':'text/javascript; charset=utf-8','.json':'application/json','.png':'image/png','.jpg':'image/jpeg','.svg':'image/svg+xml','.txt':'text/plain; charset=utf-8'};
const assets = {};
for (const dir of ['', 'assets', 'downloads']) {
  for (const file of readdirSync(resolve(root,'preview',dir),{withFileTypes:true})) {
    if (!file.isFile() || file.name.endsWith('.zip')) continue;
    const path = [dir,file.name].filter(Boolean).join('/');
    const raw=readFileSync(resolve(root,'preview',path)), compressed=gzipSync(raw);
    assets['/'+path] = {type:types[extname(file.name)] ?? 'application/octet-stream',base64:(compressed.length < raw.length ? compressed : raw).toString('base64'),encoding:compressed.length < raw.length ? 'gzip' : 'identity'};
  }
}
const feed = JSON.parse(readFileSync(resolve(root,'release-data.json'),'utf8').replace(/^\uFEFF/,''));
const staged = JSON.parse(readFileSync(resolve(root,'upload-catalog.json'),'utf8'));
const catalog = [...feed.releases.map(r=>r.asset),...staged];
const unique = new Map();
for (const asset of catalog) {
  if (!/^clipcraft-\d+\.\d+\.\d+(?:-[a-z0-9.]+)?-windows-x64\.zip$/.test(asset.file) || asset.file.includes('..') || !/^[a-f0-9]{64}$/.test(asset.sha256) || !Number.isSafeInteger(asset.bytes) || asset.bytes<=0 || asset.bytes>24*1024*1024) throw new Error('Invalid upload catalog');
  if (unique.has(asset.file) && JSON.stringify(unique.get(asset.file))!==JSON.stringify(asset)) throw new Error('Conflicting immutable catalog entry');
  unique.set(asset.file,asset);
}
mkdirSync(resolve(root,'dist/server'),{recursive:true});
mkdirSync(resolve(root,'dist/.openai'),{recursive:true});
copyFileSync(resolve(root,'.openai/hosting.json'),resolve(root,'dist/.openai/hosting.json'));
writeFileSync(resolve(root,'dist/server/index.js'),readFileSync(resolve(root,'worker/handler.mjs'),'utf8')+`\nconst assets=${JSON.stringify(assets)};\nconst catalog=${JSON.stringify([...unique.values()])};\nexport default createHandler(assets,catalog);\n`);
// Phase one retains every existing ZIP as a static fallback. Phase two is allowed
// only after every published release has a verified remote-download receipt.
const storage = JSON.parse(readFileSync(resolve(root,'release-storage.json'),'utf8'));
if (storage.phase === 'migrate') {
  mkdirSync(resolve(root,'dist/client/downloads'),{recursive:true});
  for (const release of feed.releases) copyFileSync(resolve(root,'release-assets',release.asset.file),resolve(root,'dist/client/downloads',release.asset.file));
} else if (storage.phase === 'durable') {
  for (const release of feed.releases) {
    const receipt = storage.verified.find(x=>x.file===release.asset.file);
    if (!receipt || receipt.sha256!==release.asset.sha256 || receipt.bytes!==release.asset.bytes) throw new Error('Missing verified durable release receipt');
    if (existsSync(resolve(root,'dist/client/downloads',release.asset.file))) throw new Error('Remove verified fallback from generated output before durable packaging');
  }
} else throw new Error('Unknown storage phase');
console.log(`Worker built: ${Object.keys(assets).length} embedded web assets, ${unique.size} allowed immutable uploads; ${storage.phase} phase.`);
