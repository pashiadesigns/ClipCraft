// Deterministic raster/Windows exports of the supplied vector master. No network.
const fs = require('node:fs');
const path = require('node:path');
const sharp = require('sharp');
const root = path.resolve(__dirname, '..');
const dir = path.join(root, 'apps/desktop/assets/brand');
const name = process.argv[2] || 'clipdesigner';
if (name !== 'clipdesigner') throw new Error('Unknown brand asset');
async function main() {
  const source = fs.readFileSync(path.join(dir, `${name}.svg`), 'utf8');
  // Give percentage-sized SVG exports a bounded raster viewport without changing
  // the master, viewBox, artwork, colors or aspect ratio.
  const svg = Buffer.from(source.replace(/<svg\b[^>]*>/, root => root
    .replace(/\bwidth="[^"]*"/, 'width="1024"')
    .replace(/\bheight="[^"]*"/, 'height="1024"')));
  const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256, 512];
  const pngs = [];
  for (const size of sizes) {
    const png = await sharp(svg, { density: 144 }).resize(size, size).png().toBuffer();
    fs.writeFileSync(path.join(dir, `${name}-${size}.png`), png);
    if (size <= 256) pngs.push({size, png});
  }
  const directory = Buffer.alloc(6 + pngs.length * 16);
  directory.writeUInt16LE(1, 2);
  directory.writeUInt16LE(pngs.length, 4);
  let offset = directory.length;
  pngs.forEach(({size, png}, index) => {
    const at = 6 + index * 16;
    directory[at] = directory[at + 1] = size === 256 ? 0 : size;
    directory.writeUInt16LE(1, at + 4);
    directory.writeUInt16LE(32, at + 6);
    directory.writeUInt32LE(png.length, at + 8);
    directory.writeUInt32LE(offset, at + 12);
    offset += png.length;
  });
  fs.writeFileSync(path.join(dir, `${name}.ico`), Buffer.concat([directory, ...pngs.map(x => x.png)]));
  console.log(`Exported ${sizes.length} PNG sizes and a ${pngs.length}-size Windows icon.`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
