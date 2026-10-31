import {readFileSync,writeFileSync,mkdirSync,cpSync,renameSync,existsSync,copyFileSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {validateReleases} from '../sites/capture-workbench/scripts/release.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..'),site=resolve(root,'sites/capture-workbench');
const version=process.argv[2];
if(version) {
  if(!/^\d+\.\d+\.\d+(?:-[a-z0-9.]+)?$/.test(version))throw new Error('Invalid version');
  const release=JSON.parse(readFileSync(resolve(root,'artifacts/releases',version,'site-entry.json'),'utf8').replace(/^\uFEFF/,''));
  if(release.version!==version)throw new Error('Version mismatch');
  const destination=resolve(site,'release-assets',release.asset.file);
  if(!existsSync(destination))copyFileSync(resolve(root,'artifacts/releases',version,release.asset.file),destination);
  const feed=JSON.parse(readFileSync(resolve(site,'release-data.json'),'utf8').replace(/^\uFEFF/,''));
  if(!feed.releases.some(r=>r.version===version)) {
    const candidate={releases:[release,...feed.releases]};validateReleases(candidate,site);
    writeFileSync(resolve(site,'release-data.json'),JSON.stringify(candidate,null,2)+'\n');
  }
}
execFileSync(process.execPath,['scripts/build.mjs'],{cwd:site,stdio:'inherit',windowsHide:true});
const feed=JSON.parse(readFileSync(resolve(site,'release-data.json'),'utf8').replace(/^\uFEFF/,''));validateReleases(feed,site);
const stateRoot=resolve(root,'artifacts/selfhost');mkdirSync(resolve(stateRoot,'snapshots'),{recursive:true});
const digest=createHash('sha256');
for(const name of ['release-data.json','roadmap.json'])digest.update(readFileSync(resolve(site,name)));
// Include all generated web assets so visual edits also create a new snapshot.
const {readdirSync}=await import('node:fs');
function addTree(dir){for(const f of readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name))){const p=resolve(dir,f.name);digest.update(f.name);if(f.isDirectory())addTree(p);else digest.update(readFileSync(p));}}
addTree(resolve(site,'preview'));
const fingerprint=digest.digest('hex'),snapshot='site-'+fingerprint.slice(0,20),currentPath=resolve(stateRoot,'current.json');
const previous=existsSync(currentPath)?JSON.parse(readFileSync(currentPath,'utf8')):null;
if(previous?.fingerprint===fingerprint) {console.log('Local website unchanged; no republish.');process.exit(0);}
const destination=resolve(stateRoot,'snapshots',snapshot);
if(!existsSync(destination))cpSync(resolve(site,'preview'),destination,{recursive:true});
const current={snapshot,fingerprint,latest:feed.releases[0].version,published_at:new Date().toISOString(),releases:feed.releases.map(r=>r.asset),hosting:'this-pc'};
writeFileSync(resolve(stateRoot,'current.next.json'),JSON.stringify(current,null,2)+'\n');renameSync(resolve(stateRoot,'current.next.json'),currentPath);
console.log(JSON.stringify({prepared:true,snapshot,latest:current.latest,downloads:current.releases.length,hosting:'this-pc'}));
