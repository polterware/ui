# Edge fixtures

`upstream-render.json` records geometry produced by mermaid 12.3.0 itself (MIT), not by this
port. The generator ran under node with jsdom so `insertEdge` could append its SVG path:

- `edges` keeps the older d3 curve and line-jump captures.
- `insertEdge` runs `edges.js` `insertEdge` for dagre, swimlane, cluster, corner, style, and
  `edgeMarker.ts` cases. `from`/`to` are node outlines with upstream coordinates: `x`/`y` are the
  node **centre**, and `clusterDb` entries use the same centre-based node shape.
  `expected.dataPoints` is the decoded `data-points` attribute (clipping happens before corner
  fixing and clusters, so this is the clipped route without marker offsets). `expected.d` is the
  rendered path: d3 rounds non-rounded paths to three decimals, while `generateRoundedPath`
  concatenates full-precision numbers.
- `intersection` captures `edges.js` `intersection`.
- `labels` captures `utils.calcTerminalLabelPosition`, `utils.calcLabelPosition`,
  `resolveEdgeLabelPosition`, `computeLabelTransform`, `setTerminalWidth`, and the background
  rectangle `createFormattedText` adds (measured text plus two pixels on every side).
- `lineJump` captures `lineJump.ts` `findEdgeIntersections` and `processEdgesWithJumps`,
  including corner clamping, adjacency shrinking, dropped hops, rounded-corner suppression, and
  gap style.
- `straightPath`/`curveSupport` capture `isStraightPath` and `curveSupportsLineHops`.

`rendering/testdata/edges/d3-curves.json` captures d3-shape's path output for the curve kinds.
