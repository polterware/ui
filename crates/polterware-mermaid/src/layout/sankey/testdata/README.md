# d3-sankey differential fixtures

Each JSON file is one input graph, the generator options Mermaid sets, and the exact
layout `d3-sankey` 0.12.3 produced for it. They were generated with Node against the
copy Mermaid 12.3.0 depends on, under
`research/mermaid-port/upstream/mermaid/node_modules/.pnpm/d3-sankey@0.12.3/node_modules/d3-sankey/src`:

```sh
node crates/polterware-mermaid/src/layout/sankey/testdata/generate.mjs
```

The script runs `sankey().nodeId(d => d.id).nodeWidth(...).nodePadding(...)
.nodeAlign(...).extent(...).iterations(6)` over each graph and records every node
field (`index`, `value`, `depth`, `height`, `layer`, `x0`, `x1`, `y0`, `y1`), every
link field (`index`, `source`, `target`, `value`, `width`, `y0`, `y1`), and the path
`sankeyLinkHorizontal()` draws for the link.

The cases are a chain in all four alignments, a fan-out, a fan-in, a diamond, a node
with a `fixedValue`, a single node with no links, and Mermaid's own sankey example
data (`src/diagrams/sankey/parser/energy.csv`, parsed the way `sankey.jison` does:
comment lines removed, quoted fields unescaped, nodes in first-appearance order).

JSON has no NaN, so `JSON.stringify` writes it as `null`, and a `null` coordinate
means the port must produce a NaN there too: a graph with a single column divides
by `x - 1 = 0` in `kx`, and upstream writes NaN into the node's `x0`/`x1` (the single
node fixture also reaches `y0`/`y1` through `value * ky`, where `ky` is infinite and
the value zero).

`differential.rs` rebuilds each graph through this port and compares the same fields
within `1e-6`; link paths are compared exactly, because they are the strings
d3-path builds from JavaScript-formatted numbers.
