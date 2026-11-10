import type { ThemeProject, ThemeNode } from './types';

export type NewTheme = { name: string; author: string; description: string; version: string; license: string; target: string; targetVersion: string; display: string; template: string };
export const displays = { '16:9': [1920, 1080], '4:3': [1440, 1080], Portrait: [1080, 1920] } as const;

/** Display-name suggestion only; distinct project IDs remain the identity. */
export function suggestCopyName(name: string, existingNames: string[] = []): string {
  let base = name.trim() || 'Untitled theme';
  let number = 1;
  const modern = /^(.*) \(copy(?: (\d{1,6}))?\)$/i.exec(base);
  const legacy = /(?: copy)+$/i.exec(base);
  if (modern && modern[1].trim()) { base = modern[1].trim(); number = Number(modern[2] ?? 1) + 1; }
  else if (legacy && legacy.index > 0) { base = base.slice(0, legacy.index).trim(); number = legacy[0].trim().split(/\s+/).length + 1; }
  const taken = new Set(existingNames.map(n => n.trim().toLowerCase()));
  for (;;) {
    const suffix = number === 1 ? ' (copy)' : ` (copy ${number})`;
    const candidate = Array.from(base).slice(0, 120 - suffix.length).join('').trimEnd() + suffix;
    if (!taken.has(candidate.toLowerCase())) return candidate;
    number++;
  }
}
type Box = [number, number, number, number];
const box = ([x, y, width, height]: Box) => ({ x: { value: x, unit: '%' as const }, y: { value: y, unit: '%' as const }, width: { value: width, unit: '%' as const }, height: { value: height, unit: '%' as const }, rotation: 0 });
const panel = (id: string, name: string, at: Box, fill: string): ThemeNode => ({ id, type: 'panel', name, layout: box(at), style: { fill }, children: [], assets: [], bindings: {} });
const text = (id: string, name: string, at: Box, words: string, size: number, color = 'ffffff', source?: string): ThemeNode => ({
  id, type: 'text', name, layout: box(at), style: { text: words, color, fontSize: { value: size, unit: 'px' } }, children: [], assets: [],
  bindings: source ? { text: { source, kind: source === 'game.year' || source === 'game.players' ? 'number' : 'text' } } : {} });

/** The "Ready layout" starting point (ADR-108): an original, plain design to change. */
function readyLayout(role: string, name: string): ThemeNode[] {
  const nodes: ThemeNode[] = [panel(`${role}-background`, 'Background', [0, 0, 100, 100], '10141f')];
  if (role === 'system_select') nodes.push(text(`${role}-system`, 'System name', [10, 40, 80, 14], 'System name', 96, 'ffffff', 'system.name'), text(`${role}-hint`, 'Hint', [10, 58, 80, 6], 'Choose a system', 32, '9fb3cb'));
  else if (role === 'game_list' || role === 'game_detail') nodes.push(
    text(`${role}-heading`, 'Heading', [4, 4, 92, 8], name, 48, '9fb3cb', role === 'game_list' ? 'system.name' : undefined),
    panel(`${role}-list`, role === 'game_list' ? 'Game list area' : 'Picture area', [4, 15, 44, 80], '1b2233'),
    text(`${role}-title`, 'Game title', [52, 15, 44, 9], 'Game title', 52, 'ffffff', 'game.title'),
    text(`${role}-year`, 'Year', [52, 25, 20, 5], 'Year', 28, 'f4c96b', 'game.year'),
    text(`${role}-description`, 'Description', [52, 32, 44, 50], 'Description', 28, 'c8d0e0', 'game.description'));
  else nodes.push(text(`${role}-title`, 'Title', [5, 42, 90, 14], name, 72));
  return nodes;
}

export function newDocument(form: NewTheme): ThemeProject {
  const [width, height] = displays[form.display as keyof typeof displays] ?? displays['16:9'];
  const roles = ['startup', 'main_menu', 'system_select', 'game_list', 'game_detail', 'launch', 'idle', 'exit'];
  return {
    format: 'retrodrive.theme', schemaVersion: '1.0.0', id: 'new-theme',
    metadata: { name: form.name.trim(), author: form.author, description: form.description, version: form.version, license: form.license, tags: [] },
    targets: form.target ? [{ frontendFamily: form.target, frontendVersion: form.targetVersion.trim() || null }] : [],
    scenes: roles.map(role => {
      const name = role.split('_').map(s => s[0].toUpperCase() + s.slice(1)).join(' ');
      const nodes: ThemeNode[] = form.template === 'ready' ? readyLayout(role, name) : form.template === 'starter' ? [{
        id: `${role}-title`, type: 'text', name,
        layout: { x: { value: 5, unit: '%' }, y: { value: 5, unit: '%' }, width: { value: 90, unit: '%' }, height: { value: 10, unit: '%' }, rotation: 0 },
        style: {}, children: [], assets: [], bindings: role === 'game_list' ? { text: { source: 'game.title', kind: 'text' } } : {},
      }] : [];
      return { id: role, role, name, resolution: { width, height }, nodes };
    }), assets: [], tokens: {}, variants: {},
  };
}

export function errorText(error: unknown): string {
  if (Array.isArray(error)) return error.map(errorText).join('; ');
  if (error && typeof error === 'object' && 'message' in error) return String(error.message);
  return String(error);
}
