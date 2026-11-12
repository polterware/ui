// Generates the differential fixtures for src/layout/hierarchy, with the `d3-hierarchy`
// 3.1.2 copy Mermaid 12.3.0 depends on. Run from this directory:
//
//   node generate.mjs
//
// D3_HIERARCHY overrides the path to the package's src/index.js.

import { existsSync, writeFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The package lives at the workspace root, above this worktree; walk up until it is found.
const packagePath =
  'research/mermaid-port/upstream/mermaid/node_modules/.pnpm/d3-hierarchy@3.1.2/node_modules/d3-hierarchy/src/index.js';
function findSource() {
  if (process.env.D3_HIERARCHY) return process.env.D3_HIERARCHY;
  let base = new URL('.', import.meta.url);
  for (let i = 0; i < 12; i += 1) {
    const candidate = fileURLToPath(new URL(packagePath, base));
    if (existsSync(candidate)) return candidate;
    base = new URL('../', base);
  }
  throw new Error(`cannot find ${packagePath} above ${import.meta.url}`);
}
const source = findSource();
const {
  hierarchy,
  treemap,
  treemapBinary,
  treemapDice,
  treemapSlice,
  treemapSliceDice,
  treemapSquarify,
  treemapResquarify,
} = await import(pathToFileURL(source));

const trees = {
  flat: {
    name: 'root',
    children: [6, 6, 4, 3, 2, 2].map((value, index) => ({
      name: `leaf-${index}`,
      value,
    })),
  },
  wide: {
    name: 'root',
    children: [9, 8, 7, 6, 5, 4, 3, 3, 2, 1, 1, 1].map((value, index) => ({
      name: `leaf-${index}`,
      value,
    })),
  },
  nestedTwo: {
    name: 'root',
    children: [
      {
        name: 'alpha',
        children: [
          { name: 'alpha-1', value: 6 },
          { name: 'alpha-2', value: 3 },
        ],
      },
      {
        name: 'beta',
        children: [
          { name: 'beta-1', value: 4 },
          { name: 'beta-2', value: 2 },
          { name: 'beta-3', value: 1 },
        ],
      },
      {
        name: 'gamma',
        children: [
          { name: 'gamma-1', value: 5 },
          { name: 'gamma-2', value: 2 },
        ],
      },
    ],
  },
  nestedThree: {
    name: 'root',
    children: [
      {
        name: 'alpha',
        children: [
          {
            name: 'alpha-a',
            children: [
              { name: 'alpha-a-1', value: 4 },
              { name: 'alpha-a-2', value: 2 },
            ],
          },
          { name: 'alpha-b', value: 3 },
        ],
      },
      {
        name: 'beta',
        children: [
          { name: 'beta-a', value: 5 },
          {
            name: 'beta-b',
            children: [
              { name: 'beta-b-1', value: 3 },
              { name: 'beta-b-2', value: 3 },
              { name: 'beta-b-3', value: 1 },
            ],
          },
        ],
      },
    ],
  },
  zeroValues: {
    name: 'root',
    children: [
      {
        name: 'alpha',
        children: [
          { name: 'alpha-1', value: 10 },
          { name: 'alpha-2', value: 0 },
        ],
      },
      {
        name: 'beta',
        children: [
          { name: 'beta-1', value: 0 },
          { name: 'beta-2', value: 5 },
        ],
      },
      { name: 'gamma', value: 0 },
      { name: 'delta', value: 8 },
    ],
  },
  zeroTotal: {
    name: 'root',
    children: [
      {
        name: 'alpha',
        children: [
          { name: 'alpha-1', value: 0 },
          { name: 'alpha-2', value: 0 },
        ],
      },
      { name: 'beta', value: 0 },
    ],
  },
};

const tiles = {
  squarify: () => treemap(),
  'squarify-ratio': (ratio) => treemap().tile(treemapSquarify.ratio(ratio)),
  binary: () => treemap().tile(treemapBinary),
  dice: () => treemap().tile(treemapDice),
  slice: () => treemap().tile(treemapSlice),
  sliceDice: () => treemap().tile(treemapSliceDice),
  resquarify: () => treemap().tile(treemapResquarify),
  'resquarify-ratio': (ratio) => treemap().tile(treemapResquarify.ratio(ratio)),
};

function paddingOf(spec) {
  if (spec === undefined) return 0;
  if (typeof spec === 'number') return spec;
  const [whenChildren, otherwise] = spec.if_children;
  return (d) => (d.children && d.children.length > 0 ? whenChildren : otherwise);
}

function paths(root) {
  const byNode = new Map();
  const walk = (node, path) => {
    byNode.set(node, path);
    for (const [index, child] of (node.children ?? []).entries()) {
      walk(child, path === '' ? `${index}` : `${path}.${index}`);
    }
  };
  walk(root, '');
  return byNode;
}

function fixture(name, tree, { size = [960, 500], tile = 'squarify', ratio, round = false, padding = {} } = {}) {
  const root = hierarchy(tree, (d) => d.children)
    .sum((d) => d.value ?? 0)
    .sort((a, b) => (b.value ?? 0) - (a.value ?? 0));

  const layout = tiles[tile](ratio)
    .size(size)
    .paddingInner(paddingOf(padding.inner))
    .paddingTop(paddingOf(padding.top))
    .paddingRight(paddingOf(padding.right))
    .paddingBottom(paddingOf(padding.bottom))
    .paddingLeft(paddingOf(padding.left))
    .round(round);
  layout(root);

  const pathsOf = paths(root);
  const output = root.descendants().map((node) => ({
    path: pathsOf.get(node),
    name: node.data.name,
    depth: node.depth,
    value: node.value ?? null,
    x0: node.x0,
    y0: node.y0,
    x1: node.x1,
    y1: node.y1,
  }));

  return {
    name,
    size,
    tile,
    ...(ratio === undefined ? {} : { ratio }),
    round,
    padding,
    tree,
    output,
  };
}

const mermaidPadding = {
  inner: 10,
  top: { if_children: [35, 0] },
  left: { if_children: [10, 0] },
  right: { if_children: [10, 0] },
  bottom: { if_children: [10, 0] },
};

const fixtures = [
  fixture('flat_squarify', trees.flat),
  fixture('flat_wide_squarify', trees.wide),
  fixture('nested_two_squarify', trees.nestedTwo),
  fixture('nested_three_squarify', trees.nestedThree),
  fixture('zero_values_squarify', trees.zeroValues),
  fixture('zero_total_squarify', trees.zeroTotal),
  fixture('tiled_squarify_ratio', trees.nestedTwo, { tile: 'squarify-ratio', ratio: 2 }),
  fixture('tiled_binary', trees.nestedTwo, { tile: 'binary' }),
  fixture('tiled_dice', trees.nestedTwo, { tile: 'dice' }),
  fixture('tiled_slice', trees.nestedTwo, { tile: 'slice' }),
  fixture('tiled_slice_dice', trees.nestedTwo, { tile: 'sliceDice' }),
  fixture('tiled_resquarify', trees.nestedTwo, { tile: 'resquarify' }),
  fixture('tiled_resquarify_ratio', trees.nestedTwo, {
    tile: 'resquarify-ratio',
    ratio: 1.4,
  }),
  fixture('padding_mermaid', trees.nestedThree, { padding: mermaidPadding, round: true }),
  fixture('padding_constants', trees.nestedTwo, {
    padding: { inner: 6, top: 12, right: 8, bottom: 4, left: 20 },
  }),
  fixture('padding_mermaid_flat', trees.flat, { padding: mermaidPadding, round: true }),
];

for (const data of fixtures) {
  writeFileSync(new URL(`./${data.name}.json`, import.meta.url), `${JSON.stringify(data, null, 2)}\n`);
}
console.log(`wrote ${fixtures.length} fixtures`);
