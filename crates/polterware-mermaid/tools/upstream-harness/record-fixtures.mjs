import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderShape } from './render-shape.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const testdataDir = path.resolve(__dirname, '../../src/render/testdata/shapes');

const bundle = await import('/Users/erickpatrickbarcelos/codes/polterware-workspace/research/mermaid-port/harness/mermaid-render.bundle.mjs');

const shortLabel = 'Hi';
const longLabel = 'A much longer label';

export async function recordShape(shapeName, force = false) {
  const outPath = path.join(testdataDir, `${shapeName}.json`);
  if (!force && fs.existsSync(outPath)) {
    try {
      const existing = JSON.parse(fs.readFileSync(outPath, 'utf8'));
      // If it's a legacy test fixture used by an existing unit test, preserve it.
      if (
        existing.primitives ||
        existing.measure ||
        existing.label_font_size ||
        existing.labelSize ||
        existing.cases?.[0]?.elements ||
        existing.cases?.[0]?.bbox ||
        existing.cases?.[0]?.primitives ||
        existing.cases?.[0]?.bounds
      ) {
        return { shape: shapeName, ok: true, skipped: true };
      }
    } catch {
      // If invalid, overwrite
    }
  }

  try {
    const shortCase = await renderShape(shapeName, shortLabel, 'classic');
    const longCase = await renderShape(shapeName, longLabel, 'classic');

    const fixture = {
      shape: shapeName,
      look: 'classic',
      cases: [
        {
          label: shortLabel,
          width: shortCase.width,
          height: shortCase.height,
          labelTransform: shortCase.labelTransform,
          pathData: shortCase.pathData,
          polygonPoints: shortCase.polygonPoints,
          rectData: shortCase.rectData,
          intersects: shortCase.intersects,
          probes: shortCase.probes,
        },
        {
          label: longLabel,
          width: longCase.width,
          height: longCase.height,
          labelTransform: longCase.labelTransform,
          pathData: longCase.pathData,
          polygonPoints: longCase.polygonPoints,
          rectData: longCase.rectData,
          intersects: longCase.intersects,
          probes: longCase.probes,
        },
      ],
    };

    fs.writeFileSync(outPath, JSON.stringify(fixture, null, 2) + '\n', 'utf8');
    return { shape: shapeName, ok: true };
  } catch (err) {
    return { shape: shapeName, ok: false, error: err.message };
  }
}

if (process.argv[1] && process.argv[1].endsWith('record-fixtures.mjs')) {
  const shapesToRecord = process.argv.slice(2);
  const targets = shapesToRecord.length > 0 ? shapesToRecord : Object.keys(bundle.shapes);
  console.log(`Recording ${targets.length} shapes...`);
  let success = 0;
  let failed = 0;
  for (const s of targets) {
    const res = await recordShape(s, false);
    if (res.ok) {
      success++;
    } else {
      failed++;
      console.error(`Failed ${s}:`, res.error);
    }
  }
  console.log(`Done. Success: ${success}, Failed: ${failed}`);
}
