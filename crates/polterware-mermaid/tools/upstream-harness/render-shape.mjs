// node render-shape.mjs <shape> <label> [look]: renders one Mermaid shape under jsdom and prints
// its SVG and the node box as JSON. Text is measured with a fixed monospace rule (each character
// 0.6 em wide, lines 1.15 em tall), the same rule as polterware-mermaid's FallbackMeasure for a
// family containing "mono", so fixtures and Rust tests measure labels identically.
import { createRequire } from 'node:module';
const require = createRequire('/Users/erickpatrickbarcelos/codes/polterware-workspace/research/mermaid-port/upstream/mermaid/packages/mermaid/package.json');
const { JSDOM } = require('jsdom');
const dom = new JSDOM('<!doctype html><html><body><svg id="svg" xmlns="http://www.w3.org/2000/svg"><g id="root"></g></svg></body></html>', { pretendToBeVisual: true });
for (const k of ['window', 'document', 'navigator', 'Element', 'SVGElement', 'HTMLElement', 'Node', 'DOMParser', 'getComputedStyle', 'Image']) {
  try { globalThis[k] = dom.window[k]; } catch { Object.defineProperty(globalThis, k, { value: dom.window[k], configurable: true }); }
}
if (!dom.window.HTMLImageElement.prototype.decode) {
  dom.window.HTMLImageElement.prototype.decode = async () => {};
}

const fontSize = (el) => parseFloat(dom.window.getComputedStyle(el).fontSize) || 16;
const textBox = function (el) {
  const rows = el.querySelectorAll ? el.querySelectorAll('.row') : [];
  const lines = rows.length > 0
    ? Array.from(rows).map((r) => r.textContent || '')
    : (el.textContent || '').split('\n');
  const size = fontSize(el);
  const width = Math.max(0, ...lines.map((l) => [...l].length)) * 0.6 * size;
  const height = (el.textContent || '')
    ? (lines.length <= 1 ? lines.length * 1.15 * size : (1.15 + (lines.length - 1) * 1.1) * size)
    : 0;
  return { x: 0, y: 0, width, height, top: 0, left: 0, right: width, bottom: height };
};

function arcExtrema(x0, y0, rx, ry, phiDeg, fa, fs, x1, y1) {
  if (rx === 0 || ry === 0) return [{ x: x1, y: y1 }];
  rx = Math.abs(rx);
  ry = Math.abs(ry);
  const phi = (phiDeg * Math.PI) / 180;
  const cosPhi = Math.cos(phi);
  const sinPhi = Math.sin(phi);

  const dx = (x0 - x1) / 2;
  const dy = (y0 - y1) / 2;
  const x1p = cosPhi * dx + sinPhi * dy;
  const y1p = -sinPhi * dx + cosPhi * dy;

  let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
  if (lambda > 1) {
    const sqrtLambda = Math.sqrt(lambda);
    rx *= sqrtLambda;
    ry *= sqrtLambda;
  }

  const sign = fa === fs ? -1 : 1;
  const numerator = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
  const sq = Math.max(0, numerator / (rx * rx * y1p * y1p + ry * ry * x1p * x1p));
  const coef = sign * Math.sqrt(sq);
  const cxp = coef * ((rx * y1p) / ry);
  const cyp = coef * (-(ry * x1p) / rx);

  const cx = cosPhi * cxp - sinPhi * cyp + (x0 + x1) / 2;
  const cy = sinPhi * cxp + cosPhi * cyp + (y0 + y1) / 2;

  function angle(ux, uy, vx, vy) {
    const sign = ux * vy - uy * vx < 0 ? -1 : 1;
    const dot = ux * vx + uy * vy;
    const len = Math.sqrt(ux * ux + uy * uy) * Math.sqrt(vx * vx + vy * vy);
    return sign * Math.acos(Math.max(-1, Math.min(1, dot / len)));
  }

  const v1x = (x1p - cxp) / rx;
  const v1y = (y1p - cyp) / ry;
  const v2x = (-x1p - cxp) / rx;
  const v2y = (-y1p - cyp) / ry;

  let theta1 = angle(1, 0, v1x, v1y);
  let dtheta = angle(v1x, v1y, v2x, v2y);
  if (!fs && dtheta > 0) dtheta -= 2 * Math.PI;
  if (fs && dtheta < 0) dtheta += 2 * Math.PI;

  const points = [{ x: x0, y: y0 }, { x: x1, y: y1 }];
  const steps = 16;
  for (let s = 1; s < steps; s++) {
    const theta = theta1 + (dtheta * s) / steps;
    const cosT = Math.cos(theta);
    const sinT = Math.sin(theta);
    const px = cosPhi * rx * cosT - sinPhi * ry * sinT + cx;
    const py = sinPhi * rx * cosT + cosPhi * ry * sinT + cy;
    points.push({ x: px, y: py });
  }
  return points;
}

function cubicExtrema(p0, p1, p2, p3) {
  const vals = [p0, p3];
  const a = 3 * (p3 - 3 * p2 + 3 * p1 - p0);
  const b = 6 * (p2 - 2 * p1 + p0);
  const c = 3 * (p1 - p0);
  if (Math.abs(a) < 1e-12) {
    if (Math.abs(b) > 1e-12) {
      const t = -c / b;
      if (t > 0 && t < 1) {
        vals.push((1 - t) ** 3 * p0 + 3 * (1 - t) ** 2 * t * p1 + 3 * (1 - t) * t ** 2 * p2 + t ** 3 * p3);
      }
    }
  } else {
    const disc = b * b - 4 * a * c;
    if (disc >= 0) {
      const s = Math.sqrt(disc);
      const t1 = (-b - s) / (2 * a);
      const t2 = (-b + s) / (2 * a);
      for (const t of [t1, t2]) {
        if (t > 0 && t < 1) {
          vals.push((1 - t) ** 3 * p0 + 3 * (1 - t) ** 2 * t * p1 + 3 * (1 - t) * t ** 2 * p2 + t ** 3 * p3);
        }
      }
    }
  }
  return vals;
}

function quadExtrema(p0, p1, p2) {
  const vals = [p0, p2];
  const denom = p2 - 2 * p1 + p0;
  if (Math.abs(denom) > 1e-12) {
    const t = (p0 - p1) / denom;
    if (t > 0 && t < 1) {
      vals.push((1 - t) ** 2 * p0 + 2 * (1 - t) * t * p1 + t ** 2 * p2);
    }
  }
  return vals;
}

function parsePathBBox(d) {
  if (!d) return { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 };
  const tokens = d.match(/([a-df-z]|[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?)/gi) || [];
  let curX = 0, curY = 0, startX = 0, startY = 0;
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  function addPoint(x, y) {
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  let i = 0;
  let cmd = '';
  while (i < tokens.length) {
    const t = tokens[i];
    if (/^[a-df-z]$/i.test(t)) {
      cmd = t;
      i++;
    }
    const isRel = cmd === cmd.toLowerCase();
    const c = cmd.toUpperCase();
    if (c === 'M') {
      const x = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      curX = x; curY = y;
      startX = x; startY = y;
      addPoint(x, y);
      cmd = isRel ? 'l' : 'L';
    } else if (c === 'L') {
      const x = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      curX = x; curY = y;
      addPoint(x, y);
    } else if (c === 'H') {
      const x = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      curX = x;
      addPoint(curX, curY);
    } else if (c === 'V') {
      const y = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      curY = y;
      addPoint(curX, curY);
    } else if (c === 'C') {
      const x1 = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y1 = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      const x2 = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y2 = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      const x = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      const isLoop = Math.abs(curX - x) < 1e-4 && Math.abs(curY - y) < 1e-4;
      if (!isLoop) {
        for (const v of cubicExtrema(curX, x1, x2, x)) {
          if (v < minX) minX = v;
          if (v > maxX) maxX = v;
        }
        for (const v of cubicExtrema(curY, y1, y2, y)) {
          if (v < minY) minY = v;
          if (v > maxY) maxY = v;
        }
      }
      curX = x; curY = y;
    } else if (c === 'S') {
      const x2 = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y2 = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      const x = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      const x1 = curX;
      const y1 = curY;
      const isLoop = Math.abs(curX - x) < 1e-4 && Math.abs(curY - y) < 1e-4;
      if (!isLoop) {
        for (const v of cubicExtrema(curX, x1, x2, x)) {
          if (v < minX) minX = v;
          if (v > maxX) maxX = v;
        }
        for (const v of cubicExtrema(curY, y1, y2, y)) {
          if (v < minY) minY = v;
          if (v > maxY) maxY = v;
        }
      }
      curX = x; curY = y;
    } else if (c === 'Q') {
      const x1 = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y1 = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      const x = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      for (const v of quadExtrema(curX, x1, x)) {
        if (v < minX) minX = v;
        if (v > maxX) maxX = v;
      }
      for (const v of quadExtrema(curY, y1, y)) {
        if (v < minY) minY = v;
        if (v > maxY) maxY = v;
      }
      curX = x; curY = y;
    } else if (c === 'T') {
      const x = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      addPoint(x, y);
      curX = x; curY = y;
    } else if (c === 'A') {
      const rx = parseFloat(tokens[i++]);
      const ry = parseFloat(tokens[i++]);
      const rot = parseFloat(tokens[i++]);
      const fa = parseFloat(tokens[i++]);
      const fs = parseFloat(tokens[i++]);
      const x = (isRel ? curX : 0) + parseFloat(tokens[i++]);
      const y = (isRel ? curY : 0) + parseFloat(tokens[i++]);
      const pts = arcExtrema(curX, curY, rx, ry, rot, fa, fs, x, y);
      for (const p of pts) addPoint(p.x, p.y);
      curX = x; curY = y;
    } else if (c === 'Z') {
      curX = startX; curY = startY;
      addPoint(curX, curY);
    } else {
      i++;
    }
  }
  if (!isFinite(minX)) return { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 };
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY, top: minY, left: minX, right: maxX, bottom: maxY };
}

const getBBox = function () {
  const tag = (this.tagName || '').toLowerCase();
  if (tag === 'text' || tag === 'tspan') {
    return textBox(this);
  }
  if (tag === 'rect') {
    const x = parseFloat(this.getAttribute('x')) || 0;
    const y = parseFloat(this.getAttribute('y')) || 0;
    const width = parseFloat(this.getAttribute('width')) || 0;
    const height = parseFloat(this.getAttribute('height')) || 0;
    return { x, y, width, height, top: y, left: x, right: x + width, bottom: y + height };
  }
  if (tag === 'circle') {
    const cx = parseFloat(this.getAttribute('cx')) || 0;
    const cy = parseFloat(this.getAttribute('cy')) || 0;
    const r = parseFloat(this.getAttribute('r')) || 0;
    return { x: cx - r, y: cy - r, width: 2 * r, height: 2 * r, top: cy - r, left: cx - r, right: cx + r, bottom: cy + r };
  }
  if (tag === 'ellipse') {
    const cx = parseFloat(this.getAttribute('cx')) || 0;
    const cy = parseFloat(this.getAttribute('cy')) || 0;
    const rx = parseFloat(this.getAttribute('rx')) || 0;
    const ry = parseFloat(this.getAttribute('ry')) || 0;
    return { x: cx - rx, y: cy - ry, width: 2 * rx, height: 2 * ry, top: cy - ry, left: cx - rx, right: cx + rx, bottom: cy + ry };
  }
  if (tag === 'line') {
    const x1 = parseFloat(this.getAttribute('x1')) || 0;
    const y1 = parseFloat(this.getAttribute('y1')) || 0;
    const x2 = parseFloat(this.getAttribute('x2')) || 0;
    const y2 = parseFloat(this.getAttribute('y2')) || 0;
    const minX = Math.min(x1, x2), maxX = Math.max(x1, x2);
    const minY = Math.min(y1, y2), maxY = Math.max(y1, y2);
    return { x: minX, y: minY, width: maxX - minX, height: maxY - minY, top: minY, left: minX, right: maxX, bottom: maxY };
  }
  if (tag === 'polygon' || tag === 'polyline') {
    const pts = (this.getAttribute('points') || '').trim().split(/[\s,]+/).map(Number).filter((n) => !isNaN(n));
    if (pts.length >= 2) {
      let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
      for (let i = 0; i < pts.length; i += 2) {
        const x = pts[i];
        const y = pts[i + 1];
        if (x !== undefined && !isNaN(x)) { if (x < minX) minX = x; if (x > maxX) maxX = x; }
        if (y !== undefined && !isNaN(y)) { if (y < minY) minY = y; if (y > maxY) maxY = y; }
      }
      return { x: minX, y: minY, width: maxX - minX, height: maxY - minY, top: minY, left: minX, right: maxX, bottom: maxY };
    }
    return { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 };
  }
  if (tag === 'path') {
    return parsePathBBox(this.getAttribute('d'));
  }
  if (tag === 'g') {
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const child of this.children) {
      if (child.getBBox) {
        const b = child.getBBox();
        if (b && (b.width > 0 || b.height > 0)) {
          let cx = 0, cy = 0;
          const transform = child.getAttribute('transform');
          if (transform) {
            const m = transform.match(/translate\(\s*([-+]?\d*\.?\d+(?:[eE][-+]?\d+)?)\s*(?:[,\s]\s*([-+]?\d*\.?\d+(?:[eE][-+]?\d+)?))?\s*\)/);
            if (m) {
              cx = parseFloat(m[1]) || 0;
              cy = parseFloat(m[2]) || 0;
            }
          }
          const x1 = b.x + cx;
          const y1 = b.y + cy;
          const x2 = x1 + b.width;
          const y2 = y1 + b.height;
          if (x1 < minX) minX = x1;
          if (x2 > maxX) maxX = x2;
          if (y1 < minY) minY = y1;
          if (y2 > maxY) maxY = y2;
        }
      }
    }
    if (!isFinite(minX)) return { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 };
    return { x: minX, y: minY, width: maxX - minX, height: maxY - minY, top: minY, left: minX, right: maxX, bottom: maxY };
  }
  return textBox(this);
};

dom.window.SVGElement.prototype.getBBox = getBBox;
dom.window.SVGElement.prototype.getComputedTextLength = function () { return getBBox.call(this).width; };
dom.window.HTMLElement.prototype.getBoundingClientRect = getBBox;
dom.window.Element.prototype.getBoundingClientRect = getBBox;

const m = await import('/Users/erickpatrickbarcelos/codes/polterware-workspace/research/mermaid-port/harness/mermaid-render.bundle.mjs');
const d3 = require('d3');

// Map aliases for shapes that throw or use alternative names
const aliases = {
  bpmnStart: 'bpmn-start',
  bpmnIntermediate: 'bpmn-intermediate',
  bpmnBoundary: 'bpmn-boundary',
  bpmnEnd: 'bpmn-end',
  bpmnGateway: 'bpmn-gateway',
  bpmnActivity: 'bpmn-activity',
  bpmnDataObject: 'bpmn-data',
  bpmnDataStore: 'bpmn-data-store',
  bpmnAnnotation: 'bpmn-annotation',
  curlyBraces: 'braces',
  curlyBraceLeft: 'brace',
  curlyBraceRight: 'brace-r',
  bowTieRect: 'bow-rect',
  doubleCircle: 'dbl-circ',
  forkJoin: 'fork',
  halfRoundedRectangle: 'delay',
  hexagon: 'hex',
  hourglass: 'hourglass',
  invertedTrapezoid: 'trap-t',
  leanLeft: 'lean-l',
  leanRight: 'lean-r',
  lightningBolt: 'bolt',
  linedCylinder: 'lin-cyl',
  linedWaveEdgedRect: 'lin-doc',
  multiRect: 'st-rect',
  multiWaveEdgedRectangle: 'docs',
  rectLeftInvArrow: 'odd',
  rectWithTitle: 'rectWithTitle',
  shadedProcess: 'lin-rect',
  slopedRect: 'sl-rect',
  squareRect: 'rect',
  stadium: 'stadium',
  stateEnd: 'fr-circ',
  stateStart: 'sm-circ',
  subroutine: 'fr-rect',
  taggedRect: 'tag-rect',
  taggedWaveEdgedRectangle: 'tag-doc',
  tiltedCylinder: 'h-cyl',
  trapezoid: 'trap-b',
  trapezoidalPentagon: 'notch-pent',
  triangle: 'tri',
  waveEdgedRectangle: 'doc',
  waveRectangle: 'flag',
  windowPane: 'win-pane',
  cylinder: 'cyl',
  dividedRect: 'div-rect',
  filledCircle: 'f-circ',
  flippedTriangle: 'flip-tri',
  roundedRect: 'rounded',
};

for (const [k, v] of Object.entries(aliases)) {
  if (!m.shapes[k] && m.shapes[v]) {
    m.shapes[k] = m.shapes[v];
  }
}

// Upstream state.ts omitted labelPaddingX/Y in options to drawRect, yielding NaN.
// Route state to roundedRect with rx: 5, ry: 5 so it renders with classic padding.
if (m.shapes.state && m.shapes.rounded) {
  m.shapes.state = m.shapes.rounded;
}

export async function renderShape(shape, label = 'label', look = 'classic') {
  m.setConfig({ fontFamily: 'monospace', look, htmlLabels: false, flowchart: { htmlLabels: false } });
  const root = d3.select(dom.window.document.getElementById('root'));
  root.selectAll('*').remove();
  const node = {
    id: 'n',
    domId: 'n',
    label,
    name: label,
    shape,
    padding: 8,
    look,
    cssClasses: '',
    cssStyles: [],
    labelStyle: '',
    attributes: [],
    members: [],
    methods: [],
    annotations: [],
  };
  if (shape === 'block_arrow' || shape === 'blockArrow') {
    node.directions = ['right'];
  }
  if (shape === 'kanbanItem' || shape === 'kanban_item') {
    node.width = 200;
  }
  const el = await m.insertNode(root, node, { config: m.getConfig(), dir: 'TB' });
  node.x = 0; node.y = 0;
  const probes = [{ x: 200, y: 0 }, { x: 0, y: 200 }, { x: -150, y: -90 }];
  const intersects = probes.map((p) => {
    try {
      return node.intersect ? node.intersect(p) : null;
    } catch (e) {
      return String(e);
    }
  });

  // Extract geometry and label transform
  const labelEl = el.select('.label');
  const labelTransform = labelEl.node() ? labelEl.attr('transform') : null;
  const pathEl = el.select('path.label-container, path.basic, path:not(.background)');
  const pathData = pathEl.node() ? pathEl.attr('d') : null;
  const polygonEl = el.select('polygon.label-container, polygon');
  const polygonPoints = polygonEl.node() ? polygonEl.attr('points') : null;
  const rectEl = el.select('rect.label-container, rect.basic');
  const rectData = rectEl.node() ? {
    x: parseFloat(rectEl.attr('x')) || 0,
    y: parseFloat(rectEl.attr('y')) || 0,
    width: parseFloat(rectEl.attr('width')) || 0,
    height: parseFloat(rectEl.attr('height')) || 0,
    rx: parseFloat(rectEl.attr('rx')) || 0,
    ry: parseFloat(rectEl.attr('ry')) || 0,
  } : null;

  return {
    shape,
    label,
    look,
    width: node.width,
    height: node.height,
    svg: el.node().outerHTML,
    labelTransform,
    pathData,
    polygonPoints,
    rectData,
    intersects,
    probes,
  };
}

if (process.argv[1] && process.argv[1].endsWith('render-shape.mjs')) {
  const [shape, label = 'label', look = 'classic'] = process.argv.slice(2);
  const result = await renderShape(shape, label, look);
  console.log(JSON.stringify(result, null, 1));
}
