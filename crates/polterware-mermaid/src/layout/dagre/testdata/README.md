# dagre differential fixtures

Each JSON file is one input graph and the exact coordinates upstream
`@dagrejs/dagre` produced for it. They were generated with Node against the
upstream checkout at `research/mermaid-port/upstream/dagre`, using the build in
`dist/dagre.esm.js` (dagre 3.1.3), with:

```js
const graph = new Graph({ multigraph: true, compound: true });
graph.setGraph(spec.graph);
graph.setDefaultEdgeLabel(() => ({}));
// then setNode / setParent / setEdge from the spec
layout(graph);
```

The files record the input (`graph`, `nodes`, `edges`, `parents`) and the output
(`output.graph`, `output.nodes`, `output.edges`) after layout. Optional attributes
are omitted instead of written as `null`, matching what upstream's
`JSON.stringify` does with `undefined`.

`differential.rs` rebuilds each graph through this port, lays it out, and compares
every node coordinate, rank, order, cluster size, edge point, edge label
coordinate, and the drawing size within `1e-6`.
