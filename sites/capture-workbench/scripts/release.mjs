import { createHash } from 'node:crypto';
import { readFileSync, statSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

export const escapeHtml = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));

export function validateReleases(feed, root) {
  if (!Array.isArray(feed.releases) || !feed.releases.length) throw new Error('A verified release is required.');
  const versions = new Set();
  for (const release of feed.releases) {
    if (!/^\d+\.\d+\.\d+(?:-[a-z0-9.]+)?$/.test(release.version) || versions.has(release.version)) throw new Error('Invalid or duplicate version.');
    versions.add(release.version);
    if (!/^\d{4}-\d{2}-\d{2}$/.test(release.date) || Number.isNaN(Date.parse(release.date))) throw new Error('Invalid release date.');
    for (const field of ['title','summary','channel']) if (typeof release[field] !== 'string' || !release[field].trim()) throw new Error(`Missing ${field}.`);
    if (!release.changes || !Object.keys(release.changes).length || Object.values(release.changes).some(items => !Array.isArray(items) || !items.length || items.some(item => typeof item !== 'string'))) throw new Error('Invalid release notes.');
    if (!Array.isArray(release.limitations) || release.limitations.some(item => typeof item !== 'string')) throw new Error('Invalid limitations.');
    const file = release.asset?.file;
    if (!file || !/^clipcraft-[a-z0-9.-]+-windows-x64\.zip$/.test(file) || file.includes('..')) throw new Error('Invalid download filename.');
    if (file !== `clipcraft-${release.version}-windows-x64.zip`) throw new Error('Version and download differ.');
    const archived = resolve(root, 'release-assets', file);
    const path = existsSync(archived) ? archived : resolve(root, 'dist', 'downloads', file);
    const size = statSync(path).size;
    if (size !== release.asset.bytes || size === 0) throw new Error('Download size does not match release.');
    const hash = createHash('sha256').update(readFileSync(path)).digest('hex');
    if (hash !== release.asset.sha256) throw new Error('Download checksum does not match release.');
  }
  return feed.releases;
}

export function renderRelease(release, index) {
  const e = escapeHtml;
  return `<article class="release" id="v${e(release.version)}"><div class="release-meta"><time datetime="${e(release.date)}">${new Date(`${release.date}T12:00:00Z`).toLocaleDateString('en-US', {month:'short', day:'numeric', year:'numeric', timeZone:'UTC'})}</time><span class="version">v${e(release.version)}</span>${index === 0 ? '<span class="tag">Latest release</span>' : ''}</div><div class="release-body"><h3>${e(release.title)}</h3><p>${e(release.summary)}</p>${Object.entries(release.changes).map(([label, items]) => `<div class="change-group"><h4>${e(label)}</h4><ul>${items.map(item => `<li>${e(item)}</li>`).join('')}</ul></div>`).join('')}<details><summary>Known limitations &amp; release details</summary><ul>${release.limitations.map(item => `<li>${e(item)}</li>`).join('')}</ul><p class="checksum">SHA-256<br><code>${e(release.asset.sha256)}</code></p></details><a class="text-link" href="downloads/${e(release.asset.file)}" download>Download this version <span aria-hidden="true">↗</span></a></div></article>`;
}
