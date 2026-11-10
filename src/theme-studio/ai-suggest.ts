// Ask AI for design suggestions (ADR-120). Two jobs, both pure:
// - describeScreen: the one screen sent to the AI, as data. No picture files, file names,
//   paths, library games or settings; pictures are just "image" layers.
// - applyEdits: check every suggested edit against the current theme and apply the ones that
//   fit through the editor's own functions (so locked and hidden layers stay as they are).
//   Anything that doesn't fit is skipped and listed, never guessed at.
import type { Dimension, Scene, ThemeNode, ThemeProject } from './types';
import { addNode, editableNode, layerState, orderLayer, pixels, positionFromPixels, setLayerState, transformNode, updateNode, type LayerOrder } from './scene-edit';
import { setTextBinding, textSources } from './sample-data';
import { setTextAlignment } from './text-presentation';

export const AI_OPS = ['move', 'resize', 'color', 'fill', 'selected_color', 'highlight_color', 'text', 'font_size', 'link', 'align', 'opacity', 'order', 'hide', 'add_text', 'add_panel'] as const;
export const MAX_AI_EDITS = 40;

const round = (n: number) => Math.round(n * 10) / 10;
const percent = (d: Dimension, extent: number) => round(pixels(d, extent) / extent * 100);
const colour = (value: unknown) => typeof value === 'string' && /^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(value) ? value : undefined;
const fontPx = (value: unknown) => value && typeof value === 'object' && !Array.isArray(value) && (value as { unit?: unknown }).unit === 'px' && typeof (value as { value?: unknown }).value === 'number' ? (value as { value: number }).value : undefined;

/** The screen as the AI sees it: size, and each layer back to front. */
export function describeScreen(scene: Scene) {
  const { width, height } = scene.resolution;
  return {
    screen: { name: scene.name, role: scene.role, width, height },
    layers: scene.nodes.map(node => {
      const state = layerState(node);
      const layer: Record<string, unknown> = {
        id: node.id, type: node.type, name: node.name.slice(0, 80),
        x: percent(node.layout.x, width), y: percent(node.layout.y, height), width: percent(node.layout.width, width), height: percent(node.layout.height, height),
      };
      if (state.locked) layer.locked = true;
      if (state.hidden) layer.hidden = true;
      if (node.type === 'text') {
        if (node.bindings.text) layer.showsGameDetail = node.bindings.text.source;
        else if (typeof node.style.text === 'string') layer.text = node.style.text.slice(0, 500);
        if (node.style.textAlign) layer.align = node.style.textAlign;
      }
      if (node.type === 'video') layer.video = node.bindings.video ? 'game video' : 'your clip';
      for (const [key, as] of [['color', 'color'], ['fill', 'fill'], ['selectedColor', 'selectedColor'], ['selectorColor', 'highlightColor']] as const) {
        const value = colour(node.style[key]); if (value) layer[as] = value;
      }
      const size = fontPx(node.style.fontSize); if (size !== undefined) layer.fontSize = size;
      if (typeof node.style.opacity === 'number') layer.opacity = node.style.opacity;
      return layer;
    }),
  };
}

type Edit = Record<string, unknown>;
const num = (v: unknown, min: number, max: number) => typeof v === 'number' && Number.isFinite(v) && v >= min && v <= max ? v : undefined;
const cleanText = (v: unknown) => typeof v === 'string' ? v.replace(/[\u0000-\u0009\u000b-\u001f\u007f]/g, '').trim().slice(0, 500) : '';
const hex = (v: unknown) => { const s = typeof v === 'string' ? v.trim().replace(/^#/, '').toLowerCase() : ''; return /^[0-9a-f]{6}([0-9a-f]{2})?$/.test(s) ? s : undefined; };
const toUnit = (d: Dimension, pct: number, extent: number): Dimension => positionFromPixels(d, pct * extent / 100, extent);

/** What one edit does, in words, for the result list. */
function words(op: string, name: string): string {
  const n = `“${name}”`;
  switch (op) {
    case 'move': return `Moved ${n}`; case 'resize': return `Resized ${n}`;
    case 'color': return `Changed the text colour of ${n}`; case 'fill': return `Changed the colour of ${n}`;
    case 'selected_color': return `Changed the selected-game colour of ${n}`; case 'highlight_color': return `Changed the highlight bar of ${n}`;
    case 'text': return `Changed the words in ${n}`; case 'font_size': return `Changed the text size of ${n}`;
    case 'link': return `Changed what ${n} shows`; case 'align': return `Lined up the text in ${n}`;
    case 'opacity': return `Changed how see-through ${n} is`; case 'order': return `Changed the layer order of ${n}`;
    case 'hide': return `Hid ${n}`; default: return `Changed ${n}`;
  }
}

/** Check and apply suggested edits to one screen. Returns the new theme and what happened. */
export function applyEdits(project: ThemeProject, sceneId: string, edits: unknown[]): { project: ThemeProject; done: string[]; skipped: string[] } {
  let current = project;
  const done: string[] = []; const skipped: string[] = [];
  const list = Array.isArray(edits) ? edits.slice(0, MAX_AI_EDITS) : [];
  if (Array.isArray(edits) && edits.length > MAX_AI_EDITS) skipped.push(`Only the first ${MAX_AI_EDITS} changes were used.`);
  for (const raw of list) {
    const scene = current.scenes.find(s => s.id === sceneId);
    if (!scene) break;
    const edit = (raw && typeof raw === 'object' && !Array.isArray(raw) ? raw : {}) as Edit;
    const op = typeof edit.op === 'string' && (AI_OPS as readonly string[]).includes(edit.op) ? edit.op : '';
    if (!op) { skipped.push('A change of a kind Theme Studio doesn’t offer.'); continue; }
    const { width: W, height: H } = scene.resolution;
    const x = num(edit.x, -50, 150), y = num(edit.y, -50, 150), w = num(edit.width, 0.5, 200), h = num(edit.height, 0.5, 200);

    if (op === 'add_text' || op === 'add_panel') {
      if (x === undefined || y === undefined || w === undefined || h === undefined) { skipped.push('A new layer without a clear place and size.'); continue; }
      const text = cleanText(edit.text);
      if (op === 'add_text' && !text) { skipped.push('A new text without words.'); continue; }
      const added = addNode(current, sceneId, op === 'add_text' ? 'text' : 'panel');
      if (!added.id) { skipped.push('This screen can’t take another layer.'); continue; }
      const c = hex(edit.color); const size = num(edit.fontSize, 8, 256);
      current = updateNode(added.project, sceneId, added.id, node => ({
        ...node,
        name: op === 'add_text' ? 'AI text' : 'AI panel',
        layout: { ...node.layout, x: { value: round(x), unit: '%' }, y: { value: round(y), unit: '%' }, width: { value: round(w), unit: '%' }, height: { value: round(h), unit: '%' } },
        style: op === 'add_text'
          ? { ...node.style, text, ...(c ? { color: c } : {}), ...(size ? { fontSize: { value: Math.round(size), unit: 'px' } } : {}) }
          : { ...node.style, ...(c ? { fill: c } : {}) },
      }));
      done.push(op === 'add_text' ? `Added the text “${text.slice(0, 40)}”` : 'Added a panel');
      continue;
    }

    const node = scene.nodes.find(n => n.id === edit.layer);
    if (!node) { skipped.push('A change to a layer that isn’t on this screen.'); continue; }
    if (!editableNode(scene, node)) { skipped.push(`“${node.name}” is locked or hidden, so it was left as it is.`); continue; }
    const before = current;
    const change = (fn: (n: ThemeNode) => ThemeNode) => { current = updateNode(current, sceneId, node.id, fn); };
    const style = (key: string, value: unknown) => change(n => ({ ...n, style: { ...n.style, [key]: value } as ThemeNode['style'] }));
    let why = '';
    switch (op) {
      case 'move':
        if (x === undefined || y === undefined) { why = 'no clear position'; break; }
        change(n => ({ ...n, layout: { ...n.layout, x: toUnit(n.layout.x, x, W), y: toUnit(n.layout.y, y, H) } })); break;
      case 'resize':
        if (w === undefined || h === undefined) { why = 'no clear size'; break; }
        change(n => ({ ...n, layout: { ...n.layout, width: toUnit(n.layout.width, w, W), height: toUnit(n.layout.height, h, H), ...(x !== undefined && y !== undefined ? { x: toUnit(n.layout.x, x, W), y: toUnit(n.layout.y, y, H) } : {}) } })); break;
      case 'color': case 'fill': case 'selected_color': case 'highlight_color': {
        const allowed = { color: ['text', 'game_list', 'system_carousel'], fill: ['panel', 'system_carousel'], selected_color: ['game_list'], highlight_color: ['game_list'] }[op];
        const key = { color: 'color', fill: 'fill', selected_color: 'selectedColor', highlight_color: 'selectorColor' }[op];
        const c = hex(edit.color);
        if (!allowed.includes(node.type)) { why = 'that colour doesn’t apply to this kind of layer'; break; }
        if (!c) { why = 'not a colour'; break; }
        if (node.style[key] !== undefined && !colour(node.style[key])) { why = 'its colour is set up in a way the editor keeps as it is'; break; }
        style(key, c); break;
      }
      case 'text': {
        const text = cleanText(edit.text);
        if (node.type !== 'text' || node.bindings.text) { why = 'only typed text can be reworded'; break; }
        if (!text) { why = 'no words given'; break; }
        style('text', text); break;
      }
      case 'font_size': {
        const size = num(edit.fontSize, 8, 256);
        if (node.type !== 'text' && node.type !== 'game_list') { why = 'that layer has no text'; break; }
        if (size === undefined) { why = 'a text size from 8 to 256 is needed'; break; }
        if (node.style.fontSize !== undefined && fontPx(node.style.fontSize) === undefined) { why = 'its text size is set up in a way the editor keeps as it is'; break; }
        style('fontSize', { value: Math.round(size), unit: 'px' }); break;
      }
      case 'link': {
        const source = typeof edit.source === 'string' ? edit.source : null;
        if (source === null || (source !== '' && !textSources.some(o => o.source === source))) { why = 'not a game detail Theme Studio knows'; break; }
        change(n => setTextBinding(n, source)); break;
      }
      case 'align':
        change(n => setTextAlignment(n, typeof edit.align === 'string' ? edit.align : '')); break;
      case 'opacity': {
        const o = num(edit.opacity, 0, 1);
        if (o === undefined) { why = 'see-through must be from 0 to 1'; break; }
        change(n => transformNode(n, 'opacity', o)); break;
      }
      case 'order': {
        const order = ['front', 'back', 'forward', 'backward'].includes(edit.order as string) ? edit.order as LayerOrder : null;
        if (!order) { why = 'no clear order'; break; }
        current = orderLayer(current, sceneId, node.id, order); break;
      }
      case 'hide':
        current = setLayerState(current, sceneId, node.id, 'hidden', true); break;
    }
    if (why) skipped.push(`${words(op, node.name)}: skipped (${why}).`);
    else if (current === before || JSON.stringify(current) === JSON.stringify(before)) { current = before; }
    else done.push(words(op, node.name));
  }
  return { project: current, done, skipped };
}
