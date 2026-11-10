import type { ThemeNode } from './types';
import { layerState } from './scene-edit';
export type TextAlignment = 'left' | 'center' | 'right';
export function textAlignment(node: ThemeNode): TextAlignment | null {
  const value = node.style.textAlign;
  return value === undefined ? 'left' : value === 'left' || value === 'center' || value === 'right' ? value : null;
}
export function setTextAlignment(node: ThemeNode, value: string): ThemeNode {
  const state = layerState(node);
  if (node.type !== 'text' || node.children.length || state.locked || state.hidden || textAlignment(node) === null || !['left', 'center', 'right'].includes(value) || textAlignment(node) === value) return node;
  return { ...node, style: { ...node.style, textAlign: value } };
}
export function textPosition(node: ThemeNode, width: number): { x: number; anchor: 'start' | 'middle' | 'end' } {
  const alignment = textAlignment(node);
  return alignment === 'center' ? { x: width / 2, anchor: 'middle' } : alignment === 'right' ? { x: width, anchor: 'end' } : { x: 0, anchor: 'start' };
}
