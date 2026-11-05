import {readFileSync,writeFileSync,readdirSync,existsSync} from 'node:fs';
import {resolve,dirname,relative} from 'node:path';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import {sourceFiles} from './source-audit.mjs';
export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const id = value => 'SPDXRef-'+sha256(Buffer.from(value)).slice(0,24);
export function dependencyInventory(metadata, lockText, info, assets=[]) {
  const checksums=new Map();
  for(const block of lockText.split('[[package]]').slice(1)) {
    const name=block.match(/^name = "([^"]+)"/m)?.[1],version=block.match(/^version = "([^"]+)"/m)?.[1],sum=block.match(/^checksum = "([a-f0-9]{64})"/m)?.[1];
    if(sum) checksums.set(name+'@'+version,sum);
  }
  const nodes=new Map(metadata.resolve.nodes.map(n=>[n.id,n])), active=new Set();
  function visit(key){if(active.has(key))return;active.add(key);for(const dep of nodes.get(key)?.deps??[])visit(dep.pkg);}
  metadata.workspace_members.forEach(visit);
  const packages=metadata.packages.filter(p=>active.has(p.id)).map(p=>{
    const packageId=id(p.id),checksum=checksums.get(p.name+'@'+p.version);
    return {SPDXID:packageId,name:p.name,versionInfo:p.version,downloadLocation:p.source?.startsWith('registry+')?`https://crates.io/api/v1/crates/${p.name}/${p.version}/download`:'NOASSERTION',
      filesAnalyzed:false,licenseConcluded:'NOASSERTION',licenseDeclared:p.license||'NOASSERTION',copyrightText:'NOASSERTION',
      ...(checksum?{checksums:[{algorithm:'SHA256',checksumValue:checksum}]}:{}),
      ...(p.source?{externalRefs:[{referenceCategory:'PACKAGE-MANAGER',referenceType:'purl',referenceLocator:`pkg:cargo/${p.name}@${p.version}`}]}:{sourceInfo:`ClipCraft source commit ${info.commit}`})};
  });
  const relationships=metadata.workspace_members.map(key=>({spdxElementId:'SPDXRef-DOCUMENT',relationshipType:'DESCRIBES',relatedSpdxElement:id(key)}));
  for(const [key,node] of nodes) if(active.has(key)) for(const dep of node.deps) if(active.has(dep.pkg))relationships.push({spdxElementId:id(key),relationshipType:'DEPENDS_ON',relatedSpdxElement:id(dep.pkg)});
  for(const asset of assets) {packages.push({SPDXID:id(asset.name),name:asset.name,versionInfo:asset.version,downloadLocation:'NOASSERTION',filesAnalyzed:false,licenseConcluded:'NOASSERTION',licenseDeclared:asset.license,copyrightText:'NOASSERTION',sourceInfo:asset.source});
    relationships.push({spdxElementId:'SPDXRef-DOCUMENT',relationshipType:'DESCRIBES',relatedSpdxElement:id(asset.name)});}
  return {spdxVersion:'SPDX-2.3',dataLicense:'CC0-1.0',SPDXID:'SPDXRef-DOCUMENT',name:`ClipCraft-${info.version}-windows-x64`,
    documentNamespace:`https://spdx.org/spdxdocs/clipcraft-${info.version}-${info.commit}-${sha256(Buffer.from(info.created)).slice(0,12)}`,
    creationInfo:{created:info.created,creators:['Tool: ClipCraft-release-metadata-1']},
    comment:'Resolved Windows workspace build/test dependencies plus bundled source/font assets. Not an assertion that all packages are linked into both executables.',
    packages,relationships};
}
export function writeMetadata(root, metadataPath, packageRoot, version) {
  const metadata=JSON.parse(readFileSync(metadataPath,'utf8').replace(/^\uFEFF/,'')), gitArgs=['-c',`safe.directory=${root.replaceAll('\\','/')}`];
  const git=(...args)=>execFileSync('git',[...gitArgs,...args],{cwd:root,encoding:'utf8'}).trim();
  const commit=git('rev-parse','HEAD');
  if(!/^[a-f0-9]{40}$/.test(commit))throw new Error('Missing exact source commit');
  const info={version,commit,created:new Date().toISOString().replace(/\.\d{3}Z$/,'Z'),target:'x86_64-pc-windows-msvc',
    builder:process.env.GITHUB_ACTIONS==='true'?'GitHub Actions':'local',
    repository:process.env.GITHUB_REPOSITORY||null,run:process.env.GITHUB_RUN_ID||null,
    rustc:execFileSync('rustc',['--version'],{encoding:'utf8'}).trim(),node:process.version,
    cargo_lock_sha256:sha256(readFileSync(resolve(root,'Cargo.lock'))),authenticode_signed:false};
  const assets=[
    {name:'ClipCraft bundled Ubuntu fonts',version:'bundled',license:'Ubuntu-font-1.0',source:'crates/core/assets/fonts; licenses/text-fonts/Ubuntu-*'},
    {name:'ClipCraft bundled Noto Emoji font',version:'Google Fonts',license:'OFL-1.1',source:'licenses/text-fonts/NotoEmoji-PROVENANCE.txt'},
    {name:'ClipCraft bundled Hack fonts',version:'bundled',license:'MIT AND Bitstream-Vera',source:'crates/core/assets/fonts; licenses/text-fonts/Hack-LICENSE.md'},
    {name:'epaint bundled font assets',version:metadata.packages.find(p=>p.name==='epaint_default_fonts')?.version||'bundled',license:'OFL-1.1 AND Ubuntu-font-1.0 AND MIT AND Bitstream-Vera',source:'epaint_default_fonts/fonts; embedded license texts in THIRD-PARTY-NOTICES.txt'},
    {name:'DesignCraft adapted geometry and rendering helpers',version:'ea08e4c2ba290f092063c99d70c4bdd5afc45887',license:'MIT',source:'licenses/designcraft/PROVENANCE.txt'},
    {name:'Lucide UI icons',version:'4b203ec9b6002fd74fd5dadc8612220e64289305',license:'ISC AND MIT',source:'licenses/lucide/PROVENANCE.txt'},
    {name:'WiX MSI dialogs and build tools',version:'3.14.1',license:'MS-RL',source:'licenses/wix/PROVENANCE.txt; upstream source and archive digest'}];
  const sbom=dependencyInventory(metadata,readFileSync(resolve(root,'Cargo.lock'),'utf8'),info,assets);
  writeFileSync(resolve(packageRoot,'sbom.spdx.json'),JSON.stringify(sbom,null,2)+'\n');
  writeFileSync(resolve(packageRoot,'build-info.json'),JSON.stringify(info,null,2)+'\n');
  const sources=sourceFiles(root);
  const blobs=execFileSync('git',[...gitArgs,'cat-file','--batch'],{cwd:root,input:sources.map(name=>`${commit}:${name}`).join('\n')+'\n',maxBuffer:128*1024*1024});
  let offset=0;
  const sourceHashes=sources.map(name=>{
    const end=blobs.indexOf(10,offset),header=blobs.toString('utf8',offset,end).split(' ');
    if(header[1]!=='blob')throw new Error(`Source is not a committed file: ${name}`);
    const size=Number(header[2]),start=end+1;
    offset=start+size+1;
    return `${sha256(blobs.subarray(start,start+size))}  ${name}`;
  });
  writeFileSync(resolve(packageRoot,'SOURCE-SHA256SUMS.txt'),sourceHashes.join('\n')+'\n');
  const included=new Set(sbom.packages.map(p=>p.SPDXID)), notices=['Third-party dependency and asset notices for ClipCraft\n'];
  for(const pkg of metadata.packages.filter(p=>p.source&&included.has(id(p.id))).sort((a,b)=>a.name.localeCompare(b.name)||a.version.localeCompare(b.version))) {
    notices.push(`\n===== ${pkg.name} ${pkg.version} [${pkg.license||'NOASSERTION'}] =====\n`);
    const dir=dirname(pkg.manifest_path), licenseFiles=new Set(readdirSync(dir).filter(n=>/^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)([.-]|$)/i.test(n)).map(n=>resolve(dir,n)));
    if(pkg.license_file)licenseFiles.add(resolve(dir,pkg.license_file));
    if(pkg.name==='epaint_default_fonts')for(const name of ['UFL.txt','OFL.txt','Hack-Regular.txt','emoji-icon-font-mit-license.txt'])licenseFiles.add(resolve(dir,'fonts',name));
    for(const path of [...licenseFiles].sort()){if(!existsSync(path))throw new Error(`Missing dependency notice: ${pkg.name}`);notices.push(`--- ${relative(dir,path)} ---\n`+readFileSync(path,'utf8'));}
    if(!licenseFiles.size)notices.push('No standalone notice in the registry package. Consult the declared license and upstream source.\n');
  }
  for(const dir of ['licenses/designcraft','licenses/text-fonts','licenses/wix','licenses/lucide'])for(const name of readdirSync(resolve(root,dir)).sort())notices.push(`\n===== ${dir}/${name} =====\n`+readFileSync(resolve(root,dir,name),'utf8'));
  writeFileSync(resolve(packageRoot,'THIRD-PARTY-NOTICES.txt'),notices.join('\n'));
  const notes=JSON.parse(readFileSync(resolve(root,'releases',version+'.json'),'utf8').replace(/^\uFEFF/,''));
  const markdown=[`# ${notes.title}`,notes.summary,'**Unsigned Windows x64 beta. Choose the MSI installer or portable ZIP below.**',...Object.entries(notes.changes).flatMap(([title,items])=>['## '+title,...items.map(s=>'- '+s)]),'## Known limitations',...notes.limitations.map(s=>'- '+s),`Source commit: ${commit}`,'MSI: installs for the current user, with one ClipCraft shortcut and image/video project associations. Portable ZIP: extract all files and run ClipCraft.exe. No personal projects, recordings, settings or Library entries are included. Source archives are linked below. See docs/installing.md for storage locations.'];
  writeFileSync(resolve(packageRoot,'release-notes.md'),markdown.join('\n\n')+'\n');
  return info;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href){
  const [metadataPath,packageRoot,version]=process.argv.slice(2);
  if(!metadataPath||!packageRoot||!/^\d+\.\d+\.\d+-[a-z0-9.]+$/.test(version??''))throw new Error('Usage: release-metadata.mjs METADATA PACKAGE VERSION');
  writeMetadata(process.cwd(),resolve(metadataPath),resolve(packageRoot),version);
}
