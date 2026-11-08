import type { ThemeProject, ThemeNode, Dimension, Binding } from '../../src/theme-studio/types';

// This typed authoring fixture must serialize exactly like the Rust blank fixture.
export const blank: ThemeProject = {
  format: 'retrodrive.theme', schemaVersion: '1.0.0', id: 'universal-blank',
  metadata: { name: 'Untitled theme', author: '', description: '', version: '1.0.0', license: 'unknown', tags: [] },
  targets: [],
  scenes: [{ id: 'systems', role: 'system_select', name: 'Systems', resolution: { width: 1920, height: 1080 }, nodes: [] }],
  assets: [], tokens: {}, variants: {},
};

export const futureNode: ThemeNode = {
  id: 'future', type: 'future_hologram', name: 'Future node',
  layout: {
    x: { value: 0, unit: 'px' }, y: { value: 0, unit: 'px' },
    width: { value: 100, unit: '%' }, height: { value: 100, unit: '%' }, rotation: 0,
  },
  style: {}, children: [], assets: [], bindings: { title: { source: 'game.title', kind: 'text' } },
  animations: { tracks: [] }, futurePayload: { preserved: true },
};

export const starter: ThemeProject = {
  ...blank, id: 'original-starter', metadata: { ...blank.metadata, name: 'Original starter' },
  scenes: ['startup', 'main_menu', 'system_select', 'game_list', 'game_detail', 'launch', 'idle', 'exit'].map(role => ({
    id: role, role, name: role.split('_').map(word => word[0].toUpperCase() + word.slice(1)).join(' '),
    resolution: { width: 1920, height: 1080 },
    nodes: role === 'game_list' ? [{
      id: 'title', type: 'text', name: 'Title',
      layout: { x: { value: 40, unit: 'px' }, y: { value: 40, unit: 'px' }, width: { value: 800, unit: 'px' }, height: { value: 100, unit: 'px' }, rotation: 0 },
      style: { color: 'ffffff' }, children: [], assets: [], bindings: { text: { source: 'game.title', kind: 'text' } },
    }] : [],
  })),
};

// Negative compile-time assertions protect key boundaries from accidental widening.
// @ts-expect-error Portable v1 supports explicit pixels or percentages.
const wrongUnit: Dimension = { value: 10, unit: 'em' };
// @ts-expect-error Bindings have explicit result types.
const wrongBinding: Binding = { source: 'game.title', kind: 'script' };
// @ts-expect-error A source document always declares its scene collection.
const missingScenes: ThemeProject = { format: 'retrodrive.theme', schemaVersion: '1.0.0', id: 'x', metadata: blank.metadata, targets: [], assets: [], tokens: {}, variants: {} };
void [wrongUnit, wrongBinding, missingScenes];
