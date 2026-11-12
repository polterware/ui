// Generates the differential fixtures next to this script.
//
// Run with Node from the repository root:
//
//   node crates/polterware-mermaid/src/layout/sankey/testdata/generate.mjs
//
// It imports d3-sankey 0.12.3 from the copy Mermaid 12.3.0 depends on and writes
// one JSON file per case with the layout d3-sankey produced, including the
// `sankeyLinkHorizontal` path of every link.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  sankey,
  sankeyCenter,
  sankeyJustify,
  sankeyLeft,
  sankeyLinkHorizontal,
  sankeyRight,
} from "/Users/erickpatrickbarcelos/codes/polterware-workspace/research/mermaid-port/upstream/mermaid/node_modules/.pnpm/d3-sankey@0.12.3/node_modules/d3-sankey/src/index.js";

const ENERGY_CSV =
  "/Users/erickpatrickbarcelos/codes/polterware-workspace/research/mermaid-port/upstream/mermaid/packages/mermaid/src/diagrams/sankey/parser/energy.csv";

const here = dirname(fileURLToPath(import.meta.url));

const alignments = {
  left: sankeyLeft,
  right: sankeyRight,
  center: sankeyCenter,
  justify: sankeyJustify,
};

const graphs = [
  {
    name: "chain",
    nodes: ["Alpha", "Beta", "Gamma", "Delta"],
    links: [
      ["Alpha", "Beta", 30],
      ["Beta", "Gamma", 20],
      ["Gamma", "Delta", 12],
    ],
    extents: [[0, 0], [240, 200]],
    nodeWidth: 24,
    nodePadding: 8,
    alignments: ["justify", "left", "right", "center"],
  },
  {
    name: "fan_out",
    nodes: ["Hub", "One", "Two", "Three", "Four"],
    links: [
      ["Hub", "One", 8],
      ["Hub", "Two", 12],
      ["Hub", "Three", 5],
      ["Hub", "Four", 7],
    ],
    extents: [[0, 0], [320, 240]],
    nodeWidth: 24,
    nodePadding: 8,
    alignments: ["justify", "center"],
  },
  {
    name: "fan_in",
    nodes: ["One", "Two", "Three", "Four", "Sink"],
    links: [
      ["One", "Sink", 8],
      ["Two", "Sink", 12],
      ["Three", "Sink", 5],
      ["Four", "Sink", 7],
    ],
    extents: [[0, 0], [400, 180]],
    nodeWidth: 24,
    nodePadding: 8,
    alignments: ["justify", "right"],
  },
  {
    name: "diamond",
    nodes: ["Start", "Left", "Right", "End"],
    links: [
      ["Start", "Left", 10],
      ["Start", "Right", 6],
      ["Left", "End", 10],
      ["Right", "End", 6],
    ],
    extents: [[0, 0], [300, 220]],
    nodeWidth: 24,
    nodePadding: 8,
    alignments: ["justify", "center", "left"],
  },
  {
    name: "fixed_value",
    nodes: [
      { id: "Meter", fixedValue: 40 },
      { id: "Pipe" },
      { id: "Outlet" },
    ],
    links: [
      ["Meter", "Pipe", 10],
      ["Pipe", "Outlet", 10],
    ],
    extents: [[10, 20], [410, 320]],
    nodeWidth: 30,
    nodePadding: 12,
    alignments: ["justify"],
  },
  {
    name: "single_node",
    nodes: ["Lonely"],
    links: [],
    extents: [[0, 0], [100, 100]],
    nodeWidth: 24,
    nodePadding: 8,
    alignments: ["justify"],
  },
];

/// Parses one CSV record the way Mermaid's sankey.jison lexer does: quoted
/// fields, `""` as an escaped quote, and commas only outside quotes.
function parseRecord(line) {
  const fields = [];
  let field = "";
  let quoted = false;
  for (let i = 0; i < line.length; i++) {
    const character = line[i];
    if (quoted) {
      if (character === '"') {
        if (line[i + 1] === '"') {
          field += '"';
          i++;
        } else {
          quoted = false;
        }
      } else {
        field += character;
      }
    } else if (character === '"') {
      quoted = true;
    } else if (character === ",") {
      fields.push(field);
      field = "";
    } else {
      field += character;
    }
  }
  fields.push(field);
  return fields;
}

/// The graph `sankey.spec.ts` parses from energy.csv: comment lines removed,
/// rows trimmed, one node per first appearance, links in file order.
function energyGraph(text) {
  const nodes = [];
  const seen = new Set();
  const links = [];
  for (const raw of text.split(/\r?\n/)) {
    if (/^\s*%%(?!\{)/.test(raw)) continue;
    const line = raw.trim();
    if (line === "") continue;
    const [source, target, value] = parseRecord(line).map((field) =>
      field.trim().replaceAll('""', '"'),
    );
    for (const id of [source, target]) {
      if (!seen.has(id)) {
        seen.add(id);
        nodes.push(id);
      }
    }
    links.push([source, target, parseFloat(value)]);
  }
  return { nodes, links };
}

const energy = energyGraph(readFileSync(ENERGY_CSV, "utf8"));

const cases = [
  ...graphs.map((graph) => ({ ...graph, alignments: graph.alignments })),
  {
    name: "energy",
    nodes: energy.nodes,
    links: energy.links,
    extents: [[0, 0], [960, 500]],
    nodeWidth: 10,
    nodePadding: 12,
    alignments: ["justify", "center"],
  },
];

function nodeSpec(node) {
  return typeof node === "string" ? { id: node } : node;
}

for (const graph of cases) {
  for (const alignment of graph.alignments) {
    const nodes = graph.nodes.map(nodeSpec);
    const links = graph.links.map(([source, target, value]) => ({
      source,
      target,
      value,
    }));
    const extent = graph.extents;
    // Snapshot the input before layout mutates the objects with node references.
    const inputNodes = nodes.map((node) => ({ ...node }));
    const inputLinks = links.map(({ source, target, value }) => ({
      source,
      target,
      value,
    }));
    const layout = sankey()
      .nodeId((d) => d.id)
      .nodeWidth(graph.nodeWidth)
      .nodePadding(graph.nodePadding)
      .nodeAlign(alignments[alignment])
      .extent(extent)
      .iterations(6);
    layout({ nodes, links });
    const linkPath = sankeyLinkHorizontal();
    const fixture = {
      name: `${graph.name}_${alignment}`,
      nodes: inputNodes,
      links: inputLinks,
      extent,
      nodeWidth: graph.nodeWidth,
      nodePadding: graph.nodePadding,
      alignment,
      iterations: 6,
      output: {
        nodes: nodes.map((node) => ({
          id: node.id,
          index: node.index,
          value: node.value,
          depth: node.depth,
          height: node.height,
          layer: node.layer,
          x0: node.x0,
          x1: node.x1,
          y0: node.y0,
          y1: node.y1,
        })),
        links: links.map((link) => ({
          index: link.index,
          source: link.source.index,
          target: link.target.index,
          value: link.value,
          width: link.width,
          y0: link.y0,
          y1: link.y1,
          path: linkPath(link),
        })),
      },
    };
    writeFileSync(
      join(here, `${fixture.name}.json`),
      `${JSON.stringify(fixture, null, 2)}\n`,
    );
    console.log(fixture.name);
  }
}
