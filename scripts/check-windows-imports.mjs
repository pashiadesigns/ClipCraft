// Inspect the packaged PE file without launching it. Unit-test executables can
// omit unused production imports, so this check belongs after the release build.
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';

const file = process.argv[2];
if (!file) throw new Error('Usage: node scripts/check-windows-imports.mjs <ClipDesigner.exe> [--require-self-contained]');
const requireSelfContained = process.argv.includes('--require-self-contained');
for (const arg of process.argv.slice(3)) {
  if (arg !== '--require-self-contained') throw new Error(`Unknown option: ${arg}`);
}
const bytes = readFileSync(file);
const u16 = p => bytes.readUInt16LE(p);
const u32 = p => bytes.readUInt32LE(p);
const pe = u32(0x3c);
if (u16(0) !== 0x5a4d || u32(pe) !== 0x4550) throw new Error('Invalid Windows PE file');
const optional = pe + 24;
const is64 = u16(optional) === 0x20b;
if (!is64 && u16(optional) !== 0x10b) throw new Error('Unknown PE optional header');
const sections = [];
for (let i = 0; i < u16(pe + 6); i++) {
  const p = optional + u16(pe + 20) + i * 40;
  sections.push({ address: u32(p + 12), length: u32(p + 16), offset: u32(p + 20) });
}
function offset(rva) {
  if (rva < u32(optional + 60)) return rva;
  const s = sections.find(s => rva >= s.address && rva < s.address + s.length);
  if (!s) throw new Error(`Unmapped import address ${rva}`);
  return s.offset + rva - s.address;
}
function string(p) {
  const end = bytes.indexOf(0, p);
  if (end < 0) throw new Error('Unterminated import name');
  return bytes.toString('ascii', p, end);
}
const imports = [];
const importRva = u32(optional + (is64 ? 112 : 96) + 8);
if (!importRva) throw new Error('Executable has no import table');
for (let p = offset(importRva); u32(p + 12); p += 20) {
  const dll = string(offset(u32(p + 12)));
  const symbols = [];
  for (let t = offset(u32(p) || u32(p + 16)); ; t += is64 ? 8 : 4) {
    const entry = is64 ? bytes.readBigUInt64LE(t) : BigInt(u32(t));
    if (!entry) break;
    const ordinal = is64 ? 1n << 63n : 1n << 31n;
    symbols.push(entry & ordinal ? `#${entry & 0xffffn}` : string(offset(Number(entry)) + 2));
  }
  imports.push({ dll, symbols });
}
// These exports are missing from the default ComCtl32 assembly on Windows.
// Keep startup independent of a Common Controls v6 activation manifest.
const forbidden = new Set(['LoadIconMetric', 'LoadIconWithScaleDown']);
const failures = imports.flatMap(({ dll, symbols }) =>
  symbols.filter(s => forbidden.has(s)).map(s => `${dll}!${s}`));
const redistributableDependencies = imports
  .map(i => i.dll)
  .filter(dll => /^(vcruntime|msvcp|concrt)\d.*\.dll$/i.test(dll));
if (requireSelfContained) failures.push(...redistributableDependencies.map(dll =>
  `Missing bundled runtime: ${dll} (release must statically link the Visual C++ runtime)`));
const result = {
  executable: file,
  sha256: createHash('sha256').update(bytes).digest('hex'),
  passed: failures.length === 0,
  libraries: imports.length,
  symbols: imports.reduce((n, i) => n + i.symbols.length, 0),
  user32_load_image: imports.some(i => i.dll.toLowerCase() === 'user32.dll' && i.symbols.includes('LoadImageW')),
  forbidden_imports: failures,
  require_self_contained: requireSelfContained,
  redistributable_dependencies: redistributableDependencies,
  executed: false,
};
console.log(JSON.stringify(result, null, 2));
if (!result.passed) process.exitCode = 1;
