import type { ThemeProject, ThemeNode } from './types';

export type NewTheme = { name: string; author: string; description: string; version: string; license: string; target: string; targetVersion: string; display: string; template: string };
export const displays = { '16:9': [1920, 1080], '4:3': [1440, 1080], Portrait: [1080, 1920] } as const;
export function newDocument(form: NewTheme): ThemeProject {
  const [width, height] = displays[form.display as keyof typeof displays] ?? displays['16:9'];
  const roles = ['startup', 'main_menu', 'system_select', 'game_list', 'game_detail', 'launch', 'idle', 'exit'];
  return {
    format: 'retrodrive.theme', schemaVersion: '1.0.0', id: 'new-theme',
    metadata: { name: form.name.trim(), author: form.author, description: form.description, version: form.version, license: form.license, tags: [] },
    targets: form.target ? [{ frontendFamily: form.target, frontendVersion: form.targetVersion.trim() || null }] : [],
    scenes: roles.map(role => {
      const name = role.split('_').map(s => s[0].toUpperCase() + s.slice(1)).join(' ');
      const nodes: ThemeNode[] = form.template === 'starter' ? [{
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
