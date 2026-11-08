import type { Dimension, Scene, ThemeNode, ThemeProject } from './types';

export function rootNodes(scene: Scene): ThemeNode[] {
  const children = new Set(scene.nodes.flatMap(node => node.children));
  return scene.nodes.filter(node => !children.has(node.id));
}
export type LayerOrder = 'back' | 'backward' | 'forward' | 'front';
export function canOrderLayers(scene: Scene): boolean {
  return scene.nodes.every(node => ['text', 'panel'].includes(node.type) && node.children.length === 0);
}
/** Array order is the editor's back-to-front paint order, not a native export promise. */
export function orderLayer(project: ThemeProject, sceneId: string, nodeId: string, order: LayerOrder): ThemeProject {
  const scene = project.scenes.find(scene => scene.id === sceneId);
  if (!scene || !canOrderLayers(scene)) return project;
  const index = scene.nodes.findIndex(node => node.id === nodeId);
  if (index < 0) return project;
  const destination = order === 'back' ? 0 : order === 'front' ? scene.nodes.length - 1
    : order === 'backward' ? Math.max(0, index - 1) : Math.min(scene.nodes.length - 1, index + 1);
  if (destination === index) return project;
  const nodes = [...scene.nodes];
  const [node] = nodes.splice(index, 1);
  nodes.splice(destination, 0, node);
  return { ...project, scenes: project.scenes.map(current => current.id === sceneId ? { ...current, nodes } : current) };
}
export function editableNode(scene: Scene, node: ThemeNode): boolean {
  return ['text', 'panel'].includes(node.type) && node.children.length === 0 && rootNodes(scene).some(root => root.id === node.id);
}
export function pixels(dimension: Dimension, extent: number): number { return dimension.unit === '%' ? dimension.value * extent / 100 : dimension.value; }
export function positionFromPixels(dimension: Dimension, value: number, extent: number): Dimension {
  if (!Number.isFinite(value) || !Number.isFinite(extent) || extent <= 0) return dimension;
  const normalized = dimension.unit === '%' ? value / extent * 100 : value;
  return { ...dimension, value: Math.round(Math.max(-1_000_000, Math.min(1_000_000, normalized)) * 1_000_000) / 1_000_000 };
}
export function pointerToScene(x: number, y: number, rect: { left: number; top: number; width: number; height: number }, scene: Scene) {
  if (rect.width <= 0 || rect.height <= 0) return null;
  return { x: (x - rect.left) * scene.resolution.width / rect.width, y: (y - rect.top) * scene.resolution.height / rect.height };
}
export function updateNode(project: ThemeProject, sceneId: string, nodeId: string, change: (node: ThemeNode) => ThemeNode): ThemeProject {
  const scene = project.scenes.find(s => s.id === sceneId);
  const node = scene?.nodes.find(n => n.id === nodeId);
  if (!scene || !node || !editableNode(scene, node)) return project;
  return { ...project, scenes: project.scenes.map(scene => scene.id !== sceneId ? scene : { ...scene, nodes: scene.nodes.map(node => node.id === nodeId ? change(node) : node) }) };
}
export function moveNode(project: ThemeProject, sceneId: string, nodeId: string, x: number, y: number): ThemeProject {
  const scene = project.scenes.find(s => s.id === sceneId);
  if (!scene || !Number.isFinite(x) || !Number.isFinite(y)) return project;
  return updateNode(project, sceneId, nodeId, node => ({ ...node, layout: { ...node.layout, x: positionFromPixels(node.layout.x, x, scene.resolution.width), y: positionFromPixels(node.layout.y, y, scene.resolution.height) } }));
}
export function addNode(project: ThemeProject, sceneId: string, kind: 'text' | 'panel'): { project: ThemeProject; id: string } {
  if (!project.scenes.some(scene => scene.id === sceneId) || project.scenes.reduce((count, scene) => count + scene.nodes.length, 0) >= 10_000) return { project, id: '' };
  const ids = new Set(project.scenes.flatMap(scene => scene.nodes.map(node => node.id)));
  let index = 1; while (ids.has(`${kind}-${index}`)) index++;
  const id = `${kind}-${index}`;
  const node: ThemeNode = { id, type: kind, name: kind === 'text' ? 'Text' : 'Panel',
    layout: { x: { value: 10, unit: '%' }, y: { value: 15, unit: '%' }, width: { value: 30, unit: '%' }, height: { value: kind === 'text' ? 10 : 30, unit: '%' }, rotation: 0 },
    style: kind === 'text' ? { text: 'Your text', color: 'ffffff', fontSize: { value: 36, unit: 'px' } } : { fill: '263f60' }, children: [], assets: [], bindings: {} };
  return { project: { ...project, scenes: project.scenes.map(scene => scene.id === sceneId ? { ...scene, nodes: [...scene.nodes, node] } : scene) }, id };
}
export function previewColor(value: unknown, fallback: string): string { return typeof value === 'string' && /^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(value) ? `#${value}` : fallback; }
export function previewText(node: ThemeNode): string {
  if (node.bindings.text) return `{${node.bindings.text.source}}`;
  return (typeof node.style.text === 'string' ? node.style.text : node.name).slice(0, 500);
}

export function transformNode(node: ThemeNode, property: 'rotation' | 'opacity', value: number): ThemeNode {
  if (!Number.isFinite(value)) return node;
  if (property === 'rotation') {
    if (Math.abs(value) > 36_000 || value === node.layout.rotation) return node;
    return { ...node, layout: { ...node.layout, rotation: value } };
  }
  const current = node.style.opacity;
  // Preserve future expressions and out-of-range source data read-only.
  if (current !== undefined && (typeof current !== 'number' || !Number.isFinite(current) || current < 0 || current > 1)) return node;
  if (value < 0 || value > 1 || value === (current ?? 1)) return node;
  return { ...node, style: { ...node.style, opacity: value } };
}
