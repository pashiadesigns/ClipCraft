import {readFileSync,lstatSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

export function auditFile(name, bytes) {
  const errors = [], path = name.replaceAll('\\','/');
  if (/(^|\/)(?:artifacts|target|downloads|node_modules|\.codex|\.openai|\.git)(\/|$)/i.test(path)
      || /(^|\/)(?:\.env(?:\..*)?|STATUS\.md|AGENTS\.md|Launch ClipDesigner Test\.cmd)$/i.test(path)
      || /\.(?:exe|dll|zip|pfx|p12|key|pem|pdb)$/i.test(path)
      || /(^|\/)release-assets\//i.test(path)
      || /(^|\/)docs\/(?:manual-verification|.*-work\.|.*-plan\.)/i.test(path)
      || /(^|\/)sites\//i.test(path)) errors.push('private/generated path');
  if (bytes.length > 20 * 1024 * 1024) errors.push('unexpected source file size');
  if (!/\.(?:png|jpg|jpeg|ico|res|ttf|otf|woff2?)$/i.test(path)) {
    const text = bytes.toString('utf8');
    if (/\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|AKIA[A-Z0-9]{16})\b/.test(text)
        || /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/.test(text)
        || /https?:\/\/[^\s/@:]+:[^\s/@]+@/.test(text)) errors.push('credential pattern');
    if (/C:[/\\]+Users[/\\]+(?:thesa|CodexSandbox)/i.test(text)) errors.push('local user path');
  }
  return errors;
}
export function sourceFiles(root = process.cwd()) {
  return execFileSync('git',['-c',`safe.directory=${resolve(root).replaceAll('\\','/')}`,'ls-files','-z'],{cwd:root})
    .toString('utf8').split('\0').filter(Boolean).sort();
}
export function audit(root = process.cwd()) {
  const files=sourceFiles(root);
  if (!files.length) throw new Error('No tracked source to audit');
  for (const required of ['Cargo.toml','Cargo.lock','rust-toolchain.toml','LICENSE','apps/desktop/build.rs','apps/desktop/assets/brand/clipdesigner.res','crates/core/src/lib.rs']) {
    if (!files.includes(required)) throw new Error(`Required public build input is missing: ${required}`);
  }
  const findings=[];
  for (const name of files) {
    const path=resolve(root,name), stat=lstatSync(path);
    if (!stat.isFile()) {findings.push({file:name,errors:['non-file/submodule/symlink']});continue;}
    const errors=auditFile(name,readFileSync(path));
    if(errors.length) findings.push({file:name,errors});
  }
  if (findings.length) {console.error(JSON.stringify(findings,null,2));throw new Error('Source publication audit failed');}
  console.log(`Source audit passed: ${files.length} tracked files; no forbidden paths or recognized credential patterns. This is not a complete security audit.`);
  return files;
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) audit();
