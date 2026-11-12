// The box a browser reports for the SVG labels Mermaid creates with htmlLabels: false.
//
// `createFormattedText` puts every wrapped line in an outer `<tspan class="text-outer-tspan
// row">` whose `y`/`dy` place it on its own baseline; only a line's first word has no
// leading space, so the element's raw `textContent` runs the lines together. A browser's
// `getBBox()` measures the wrapped block instead: the widest line, and the ink of the first
// line (1.15em in this harness's monospace rule) plus the 1.1em baseline advance of every
// line after it. This module is the one place that rule lives, shared by the layout, shape
// and diagram harnesses.
export const MONOSPACE_ADVANCE_EM = 0.6;
export const LINE_INK_EM = 1.15;
export const LINE_ADVANCE_EM = 1.1;

/** The wrapped lines of a `<text>`: its outer tspan rows, or `\n`-separated text. */
export function textLines(el) {
  const children = el.children ? Array.from(el.children) : [];
  const rows = children.filter((child) => {
    if ((child.tagName || '').toLowerCase() !== 'tspan') return false;
    return (
      child.hasAttribute('y') ||
      child.classList?.contains('text-outer-tspan') ||
      child.classList?.contains('row')
    );
  });
  if (rows.length > 0) {
    return rows.map((row) => row.textContent || '');
  }
  return (el.textContent || '').split('\n');
}

/** The `getBBox()` box of a text element measured by `size` px in the monospace rule. */
export function textBox(el, size) {
  const nonEmpty = (el.textContent || '').length > 0;
  const lines = textLines(el);
  const width = Math.max(0, ...lines.map((line) => [...line].length)) * MONOSPACE_ADVANCE_EM * size;
  const height = nonEmpty
    ? (LINE_INK_EM + Math.max(0, lines.length - 1) * LINE_ADVANCE_EM) * size
    : 0;
  return { x: 0, y: 0, width, height, top: 0, left: 0, right: width, bottom: height };
}
