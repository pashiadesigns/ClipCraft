import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = p => JSON.parse(fs.readFileSync(path.join(root, p), 'utf8').replace(/^\uFEFF/, ''));
const queue = read('docs/manual-verification.json');
const candidate = read(queue.current_build.manifest);
if (candidate.executable_sha256 !== queue.current_build.executable_sha256) {
  throw new Error('Verification queue and current launcher refer to different binaries.');
}
const latestPublic = read('sites/capture-workbench/release-data.json').releases[0].version;
const features = new Set(read('sites/capture-workbench/roadmap.json').features.map(f => f.id));
const ids = new Set();
const statuses = new Set(['ready', 'passed', 'failed', 'skipped', 'waiting_build']);
for (const batch of queue.batches) {
  for (const check of batch.checks) {
    if (ids.has(check.id) || !statuses.has(check.status) || !check.action || !check.expected) {
      throw new Error(`Invalid or duplicate manual check: ${check.id}`);
    }
    ids.add(check.id);
    for (const id of check.features) {
      if (!features.has(id)) throw new Error(`Unknown feature ${id} in ${check.id}`);
    }
  }
}
const ready = priority => queue.batches.filter(b => b.priority === priority)
  .flatMap(b => b.checks).filter(c => c.status === 'ready').length;
const currentBatches = queue.batches.filter(b => b.priority === 'current' && b.checks.some(c => c.status === 'ready')).length;
const pendingBatches = queue.batches.map(b => ({...b, checks:b.checks.filter(c => c.status !== 'passed')}))
  .filter(b => b.checks.length).sort((a,b) => Number(a.priority !== 'current') - Number(b.priority !== 'current'));
const passed = queue.batches.flatMap(b => b.checks.filter(c => c.status === 'passed').map(c => ({...c,batch:b.title})));
const lines = [
  '# ClipCraft manual verification batches', '',
  `Updated ${queue.updated}. **${ready('current')} ready current checks in ${currentBatches} batches, plus ${ready('optional')} optional checks.** Passed checks are collected at the bottom.`, '',
  `Current build: **${queue.current_build.label}**. This is an unpublished test candidate; ${latestPublic} remains the public download.`, '',
  'Stable shortcut target:', '', `\`${path.join(root, queue.current_build.launcher)}\``, '',
  ...queue.instructions.map((s, i) => `${i + 1}. ${s}`), '',
  '## Still to verify', '',
  '| Batch | Remaining checks | Approximate time | Priority |',
  '| --- | --- | --- | --- |',
  ...pendingBatches.map(b => `| ${b.id} — ${b.title} | ${b.checks.map(c=>c.id).join(', ')} | ${b.estimate} | ${b.priority} |`), '',
];
function detail(check) {
  lines.push(`### ${check.id} ${check.title}`, '', `Status: **${check.status}**.`, '',check.action, '', `Expected: ${check.expected}`, '');
  if(check.verification){const v=check.verification;lines.push(`Recorded evidence: ${v.source}; ${v.build}. ${v.note ?? ''}`, '');}
}
for (const batch of pendingBatches) {
  lines.push(`## Batch ${batch.id} ${batch.title}`, '', `${batch.estimate}; ${batch.priority}.`, '');
  for (const check of batch.checks) detail(check);
}
lines.push('## Not waiting on you', '',
  ...queue.not_assigned_to_user.map(c => `- ${c.reason}`), '',
  '## Reporting results', '',
  'Send the build label and a compact report, for example:', '',
  `> ${queue.current_build.label} — I1 and I2 pass; I3 not tried.`, '',
  'Results retain the tested build and source of evidence. Inferred builds are identified as such. Skipped or untried checks are not passes. Passed results are preserved below and only requeued for a relevant regression.', '',
  `Build SHA256: \`${queue.current_build.executable_sha256}\`.`, '',
  'Maintained data: `docs/manual-verification.json`. Regenerate this view with `node scripts/build-manual-verification.mjs`.', '',
  '## Passed checks', '',
  `${passed.length} batch checks and ${queue.completed_user_checks.length} earlier modifier checks passed. No repeat is needed unless a targeted regression is added.`, '',
  ...queue.completed_user_checks.map(c => `- ${c.title} — ${c.build}, **pass**. Includes release commit and one-step Undo.`), '',
);
for(const check of passed) detail(check);
fs.writeFileSync(path.join(root, 'docs/manual-verification.md'), lines.join('\n'));
console.log(JSON.stringify({ current: ready('current'), optional: ready('optional'), passed:passed.length+queue.completed_user_checks.length, generated: 'docs/manual-verification.md' }));
