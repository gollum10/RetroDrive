// Tool levels (ADR-108, roadmap step 2): Simple, Standard and Pro show more or fewer
// tools. They never change the theme itself: every level opens and saves the same
// document, and a layer set up in Pro stays as it is when edited in Simple.
// Remembered per PC in localStorage (a convenience; Standard when it can't be read).
export const LEVELS = [
  { id: 'simple', name: 'Simple', hint: 'Pictures, text and colours. Drag things where you want them.' },
  { id: 'standard', name: 'Standard', hint: 'Adds panels, layer order, exact positions and sizes, and theme details.' },
  { id: 'pro', name: 'Pro', hint: 'Every control: units, rotation, fading, exact crop, the system check and more.' },
] as const;
export type Level = typeof LEVELS[number]['id'];

/** Tools that only some levels show. Anything not listed here is in every level. */
export type Tool = 'panel' | 'order' | 'numbers' | 'layerName' | 'details' | 'units' | 'transform' | 'crop' | 'check' | 'timeline';
const FROM: Record<Tool, Level> = {
  panel: 'standard', order: 'standard', numbers: 'standard', layerName: 'standard', details: 'standard',
  units: 'pro', transform: 'pro', crop: 'pro', check: 'pro', timeline: 'pro',
};
const RANK: Record<Level, number> = { simple: 0, standard: 1, pro: 2 };

export function shows(level: Level, tool: Tool): boolean { return RANK[level] >= RANK[FROM[tool]]; }

const KEY = 'rd.themeStudio.level';
export function savedLevel(): Level {
  try { const v = localStorage.getItem(KEY); return LEVELS.some(l => l.id === v) ? v as Level : 'standard'; } catch { return 'standard'; }
}
export function saveLevel(level: Level) {
  try { localStorage.setItem(KEY, level); } catch { /* not remembered */ }
}
