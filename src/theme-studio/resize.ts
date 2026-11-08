import type { ThemeProject } from './types';
import { pixels, positionFromPixels, updateNode } from './scene-edit';

export function resizeDelta(width: number, height: number, rotation: number, dx: number, dy: number) {
  const angle = rotation * Math.PI / 180;
  const dw = dx * Math.cos(angle) + dy * Math.sin(angle);
  const dh = -dx * Math.sin(angle) + dy * Math.cos(angle);
  return { width: Math.abs(dw) < 0.000001 ? width : Math.max(1, width + dw),
    height: Math.abs(dh) < 0.000001 ? height : Math.max(1, height + dh) };
}
export function resizeLayer(project: ThemeProject, sceneId: string, nodeId: string, width: number, height: number): ThemeProject {
  const scene = project.scenes.find(scene => scene.id === sceneId);
  const node = scene?.nodes.find(node => node.id === nodeId);
  if (!scene || !node || !Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return project;
  const dimensions = { width: node.layout.width, height: node.layout.height };
  for (const key of ['width', 'height'] as const) {
    const extent = key === 'width' ? scene.resolution.width : scene.resolution.height;
    const value = key === 'width' ? width : height;
    // An unchanged axis preserves its full source precision.
    if (Math.abs(value - pixels(node.layout[key], extent)) < 0.000001) continue;
    const maximum = node.layout[key].unit === '%' ? extent * 10_000 : 1_000_000;
    const dimension = positionFromPixels(node.layout[key], Math.max(1, Math.min(maximum, value)), extent);
    dimensions[key] = { ...dimension, value: Math.max(0.000001, dimension.value) };
  }
  if (dimensions.width.value === node.layout.width.value && dimensions.height.value === node.layout.height.value) return project;
  return updateNode(project, sceneId, nodeId, node => ({ ...node, layout: { ...node.layout, ...dimensions } }));
}
