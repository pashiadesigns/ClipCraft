import { escapeHtml as e } from './release.mjs';

export const statuses = {
  planned: 'Planned',
  in_progress: 'In progress',
  needs_verification: 'Needs verification',
  shipped: 'Shipped',
  deferred: 'Deferred'
};
const priorities = {next:'Next',soon:'After the essentials',later:'Later',done:'Delivered'};

export function validateRoadmap(roadmap, releaseVersions) {
  if (roadmap.schema_version !== 1 || !Array.isArray(roadmap.features) || !roadmap.features.length) throw new Error('Invalid roadmap schema.');
  const validDate = value => typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value) && new Date(`${value}T12:00:00Z`).toISOString().slice(0,10) === value;
  if (!validDate(roadmap.updated) || typeof roadmap.scope !== 'string') throw new Error('Invalid roadmap metadata.');
  const ids = new Set();
  for (const feature of roadmap.features) {
    if (!/^[A-Z]+-\d{3}$/.test(feature.id) || ids.has(feature.id)) throw new Error('Invalid or duplicate feature ID.');
    ids.add(feature.id);
    if (!Object.hasOwn(statuses, feature.status) || !Object.hasOwn(priorities, feature.priority)) throw new Error('Unknown roadmap status or priority.');
    for (const field of ['title','category','description','completion']) if (typeof feature[field] !== 'string' || !feature[field].trim()) throw new Error(`Missing feature ${field}.`);
    if (!validDate(feature.updated) || feature.updated > roadmap.updated) throw new Error('Feature date exceeds roadmap update.');
    if (!Array.isArray(feature.evidence) || feature.evidence.some(item => typeof item !== 'string' || !item.trim())) throw new Error('Invalid evidence.');
    if (feature.status === 'shipped' && (!feature.evidence.length || !releaseVersions.includes(feature.released_in))) throw new Error('Shipped features require evidence and a known release.');
    if (feature.status === 'shipped' && feature.target_version) throw new Error('Shipped features must record their release rather than a provisional target.');
    const version = value => typeof value === 'string' && /^\d+\.\d+\.\d+(?:-(?:alpha|beta|preview|rc)\.[1-9]\d*)?$/.test(value);
    if (feature.target_version && (!version(feature.target_version) || feature.target_provisional !== true)) throw new Error('Invalid provisional target version.');
    if (feature.status !== 'shipped' && !feature.target_version && feature.schedule_status !== 'unscheduled') throw new Error('Unfinished features require a target or explicit unscheduled status.');
    if (feature.original_target_version && (!version(feature.original_target_version) || !feature.target_history?.includes(feature.original_target_version))) throw new Error('Original target requires retained planning history.');
    if (feature.target_history && (!Array.isArray(feature.target_history) || feature.target_history.some(v => !version(v)))) throw new Error('Invalid target history.');
    if (feature.missed_target_version && (feature.missed_target_version !== feature.original_target_version || feature.status === 'shipped')) throw new Error('Missed target must preserve the original unfinished commitment.');
    if (feature.status === 'deferred' && (typeof feature.blocker !== 'string' || !feature.blocker.trim())) throw new Error('Deferred features require a reason.');
    if (feature.depends_on && (!Array.isArray(feature.depends_on) || feature.depends_on.some(id => typeof id !== 'string'))) throw new Error('Invalid dependencies.');
  }
  for (const feature of roadmap.features) for (const id of feature.depends_on ?? []) if (!ids.has(id) || id === feature.id) throw new Error('Unknown or self-referencing dependency.');
  return roadmap;
}

export function renderTarget(feature) {
  const parts = [];
  if (feature.missed_target_version) parts.push(`<p class="roadmap-target">Overdue · original target v${e(feature.missed_target_version)} <span>· scope still unfinished</span></p>`);
  else if (feature.original_target_version) parts.push(`<p class="roadmap-target">Original target v${e(feature.original_target_version)}</p>`);
  if (feature.status !== 'shipped') parts.push(feature.target_version
    ? `<p class="roadmap-target">Target v${e(feature.target_version)} <span>· provisional</span></p>`
    : '<p class="roadmap-target">Release not scheduled · priority shown above</p>');
  return parts.join('');
}

export function renderRoadmap(roadmap) {
  const counts = Object.fromEntries(Object.keys(statuses).map(status => [status, roadmap.features.filter(feature => feature.status === status).length]));
  const categories = [...new Set(roadmap.features.map(feature => feature.category))].sort();
  const filters = `<button type="button" data-status-filter="all" aria-pressed="true">All <span>${roadmap.features.length}</span></button>` + Object.entries(statuses).map(([status, label]) => `<button type="button" data-status-filter="${status}" aria-pressed="false">${label} <span>${counts[status]}</span></button>`).join('');
  const rows = roadmap.features.map(feature => `<article class="roadmap-feature" id="${e(feature.id)}" data-feature-status="${feature.status}" data-feature-category="${e(feature.category)}"><div class="feature-state"><span class="roadmap-status status-${feature.status}">${statuses[feature.status]}</span><span class="roadmap-priority">${priorities[feature.priority]}</span></div><div class="feature-description"><p class="roadmap-category">${e(feature.category)}</p><h2>${e(feature.title)}</h2><p>${e(feature.description)}</p>${renderTarget(feature)}<details><summary>${feature.status === 'shipped' ? 'Delivery evidence' : 'What completion looks like'}</summary><p>${e(feature.completion)}</p>${feature.evidence.length ? `<ul>${feature.evidence.map(item => `<li>${e(item)}</li>`).join('')}</ul>` : ''}${feature.depends_on?.length ? `<p class="dependency-list">Depends on: ${feature.depends_on.map(id => `<a href="#${e(id)}" data-feature-link="${e(id)}">${e(roadmap.features.find(f => f.id === id).title)}</a>`).join(', ')}</p>` : ''}${feature.blocker ? `<p class="roadmap-blocker">${e(feature.blocker)}</p>` : ''}${feature.released_in ? `<a class="text-link" href="index.html#v${e(feature.released_in)}">${feature.status === 'shipped' ? 'Released' : 'Partially delivered'} in v${e(feature.released_in)} ↗</a>` : ''}</details></div></article>`).join('\n');
  return {
    ROADMAPDATE: e(roadmap.updated),
    ROADMAPSCOPE: e(roadmap.scope),
    ROADMAPCOUNT: String(roadmap.features.length),
    ROADMAPFILTERS: filters,
    ROADMAPCATEGORIES: categories.map(category => `<option value="${e(category)}">${e(category)}</option>`).join(''),
    ROADMAPROWS: rows,
    ROADMAPSTATS: Object.entries(statuses).map(([status,label]) => `<div><b>${counts[status]}</b><span>${label}</span></div>`).join('')
  };
}

export function renderRoadmapMarkdown(roadmap) {
  const lines = ['# ClipCraft feature backlog', '', `Updated: ${roadmap.updated}`, '', roadmap.scope, '', 'Generated from roadmap.json. Edit that source and run node scripts/build.mjs; do not edit this generated view.', '', '| Feature | Area | Status | Priority |', '| --- | --- | --- | --- |'];
  const md = value => String(value).replaceAll('|', '\\|').replaceAll('\n', ' ');
  for (const feature of roadmap.features) lines.push(`| ${feature.id}: ${md(feature.title)} | ${md(feature.category)} | ${statuses[feature.status]} | ${priorities[feature.priority]} |`);
  lines.push('', '## Completion checks and evidence', '');
  for (const feature of roadmap.features) {
    lines.push(`### ${feature.id}: ${feature.title}`, '', `**${statuses[feature.status]}** · ${priorities[feature.priority]} · Updated ${feature.updated}`, '', feature.description, '', `Completion: ${feature.completion}`, '');
    for (const evidence of feature.evidence) lines.push(`- Evidence: ${evidence}`);
    if (feature.target_version) lines.push(`- Provisional target: ${feature.target_version}`);
    if (feature.original_target_version) lines.push(`- Original target: ${feature.original_target_version}${feature.missed_target_version ? ' — OVERDUE; scope still unfinished' : ''}`);
    if (feature.target_history?.length) lines.push(`- Recorded target history: ${feature.target_history.join(' → ')}`);
    if (feature.schedule_status === 'unscheduled') lines.push('- Release not scheduled; priorities are not version promises.');
    if (feature.released_in) lines.push(`- Release: ${feature.released_in}`);
    if (feature.depends_on?.length) lines.push(`- Depends on: ${feature.depends_on.join(', ')}`);
    if (feature.blocker) lines.push(`- Deferred: ${feature.blocker}`);
    lines.push('');
  }
  return lines.join('\n');
}
