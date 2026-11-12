# dagre differential fixtures

Each JSON file is one input graph and the exact coordinates `dagre-d3-es` 7.0.14
produced for it. They were generated with Node against the copy Mermaid 12.3.0
depends on, under
`research/mermaid-port/upstream/mermaid/node_modules/.pnpm/dagre-d3-es@7.0.14/node_modules/dagre-d3-es/src/dagre`,
repeating its `layout.js` pipeline so the layout graph could be inspected:

```js
const graph = new Graph({ multigraph: true, compound: true });
graph.setGraph(spec.graph);
graph.setDefaultEdgeLabel(() => ({}));
// then setNode / setParent / setEdge from the spec
layout(graph);
```

The files record the input (`graph`, `nodes`, `edges`, `parents`) and the output
(`output.graph`, `output.nodes`, `output.edges`) after layout. The `rankalign_*`
and `cluster_rankdir_*` inputs carry attributes dagre-d3-es does not read
(`rankalign`, and a cluster node's own `rankdir`), so their outputs show the
default behaviour: centred ranks and no per-cluster rotation. Optional attributes
are omitted instead of written as `null`, matching what JavaScript's `JSON.stringify`
does with `undefined`. `output.nodes[].rank` and `output.nodes[].order` come from the
internal layout graph, because dagre-d3-es copies only positions back to the input
graph. JSON has no NaN and `JSON.stringify` writes it as `null`, so a `null`
coordinate in `output.edges` means the port must produce a NaN there too.

`differential.rs` rebuilds each graph through this port, builds the layout graph,
runs the pipeline, copies the result back, and compares every node coordinate, rank,
order, cluster size, edge point, edge label coordinate, and the drawing size within
`1e-6`.
