import type { Scene, ThemeNode, ThemeProject } from './types';
import { editableNode, pixels, positionFromPixels, updateNode } from './scene-edit';

export type Alignment = 'left' | 'center-x' | 'right' | 'top' | 'center-y' | 'bottom';
export function alignedNode(scene: Scene, node: ThemeNode, alignment: Alignment): ThemeNode {
  if (!editableNode(scene, node)) return node;
  const width = pixels(node.layout.width, scene.resolution.width);
  const height = pixels(node.layout.height, scene.resolution.height);
  const angle = node.layout.rotation * Math.PI / 180;
  // The canvas rotates around the local top-left corner, then translates.
  const cos = Math.cos(angle); const sin = Math.sin(angle);
  const xs = [0, width * cos, -height * sin, width * cos - height * sin];
  const ys = [0, width * sin, height * cos, width * sin + height * cos];
  const horizontal = ['left', 'center-x', 'right'].includes(alignment);
  const axis = horizontal ? 'x' : 'y';
  const extent = horizontal ? scene.resolution.width : scene.resolution.height;
  const minimum = Math.min(...(horizontal ? xs : ys));
  const maximum = Math.max(...(horizontal ? xs : ys));
  const target = alignment === 'left' || alignment === 'top' ? -minimum
    : alignment === 'right' || alignment === 'bottom' ? extent - maximum
    : (extent - minimum - maximum) / 2;
  const raw = node.layout[axis].unit === '%' ? target / extent * 100 : target;
  // Refuse an unrepresentable position instead of silently clamping alignment.
  if (!Number.isFinite(raw) || Math.abs(raw) > 1_000_000) return node;
  const dimension = positionFromPixels(node.layout[axis], target, extent);
  if (dimension.value === node.layout[axis].value) return node;
  return { ...node, layout: { ...node.layout, [axis]: dimension } };
}
export function alignLayer(project: ThemeProject, sceneId: string, nodeId: string, alignment: Alignment): ThemeProject {
  const scene = project.scenes.find(scene => scene.id === sceneId);
  const node = scene?.nodes.find(node => node.id === nodeId);
  if (!scene || !node) return project;
  const next = alignedNode(scene, node, alignment);
  return next === node ? project : updateNode(project, sceneId, nodeId, () => next);
}
