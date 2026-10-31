import { readFileSync, writeFileSync, mkdirSync, copyFileSync, cpSync, existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { validateReleases, renderRelease, escapeHtml } from './release.mjs';
import { validateRoadmap, renderRoadmap, renderRoadmapMarkdown } from './roadmap.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = resolve(root, 'preview');
mkdirSync(output, {recursive:true});
cpSync(resolve(root,'public/assets'),resolve(output,'assets'),{recursive:true});
const feed = JSON.parse(readFileSync(resolve(root, 'release-data.json'), 'utf8').replace(/^\uFEFF/, ''));
const releases = validateReleases(feed, root);
const latest = releases[0];
const roadmap = validateRoadmap(JSON.parse(readFileSync(resolve(root, 'roadmap.json'), 'utf8').replace(/^\uFEFF/, '')), releases.map(release => release.version));
const manifest = JSON.parse(readFileSync(resolve(root, '.openai/hosting.json'), 'utf8'));
if (!manifest.project_id || manifest.r2 !== 'BUCKET' || manifest.static) throw new Error('Missing Site identity or release storage binding.');
const tokens = {
  RELEASES: releases.map(renderRelease).join('\n'),
  VERSION: escapeHtml(latest.version),
  DOWNLOAD: `downloads/${escapeHtml(latest.asset.file)}`,
  SIZE: `${(latest.asset.bytes / 1024 / 1024).toFixed(1)} MB`,
  HASH: latest.asset.sha256,
  DATE: escapeHtml(latest.date),
  ...renderRoadmap(roadmap)
};
let html = readFileSync(resolve(root, 'src/index.html'), 'utf8').replace(/\{\{([A-Z]+)\}\}/g, (_, key) => {
  if (!(key in tokens)) throw new Error(`Unknown template token ${key}`);
  return tokens[key];
});
if (/\{\{[A-Z]+\}\}/.test(html)) throw new Error('Unresolved template token.');
for (const [, asset] of html.matchAll(/<img\b[^>]*\bsrc="(assets\/[^"]+)"/g)) {
  if (!existsSync(resolve(output, asset))) throw new Error(`Missing preview image: ${asset}`);
}
writeFileSync(resolve(output, 'index.html'), html);
const roadmapHtml = readFileSync(resolve(root, 'src/roadmap.html'), 'utf8').replace(/\{\{([A-Z]+)\}\}/g, (_, key) => {
  if (!(key in tokens)) throw new Error(`Unknown roadmap token ${key}`);
  return tokens[key];
});
writeFileSync(resolve(output, 'roadmap.html'), roadmapHtml);
writeFileSync(resolve(output, 'roadmap.json'), `${JSON.stringify(roadmap, null, 2)}\n`);
writeFileSync(resolve(root, 'ROADMAP.md'), renderRoadmapMarkdown(roadmap));
for (const name of ['style.css','app.js','roadmap.js']) copyFileSync(resolve(root, 'src', name), resolve(output, name));
writeFileSync(resolve(output, 'releases.json'), `${JSON.stringify(feed, null, 2)}\n`);
mkdirSync(resolve(output, 'downloads'), {recursive:true});
writeFileSync(resolve(output, 'downloads/SHA256SUMS.txt'), releases.map(r => `${r.asset.sha256}  ${r.asset.file}`).join('\n') + '\n');
console.log(`Site built: ${releases.length} release(s); latest ${latest.version}; ${roadmap.features.length} roadmap items; downloads and SHA-256 verified.`);
