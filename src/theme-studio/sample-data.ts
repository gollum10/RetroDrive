import type { ThemeNode } from './types';
import { previewText, layerState } from './scene-edit';
export const textSources = [
    { source: 'game.title', label: 'Game title', kind: 'text' },
    { source: 'game.description', label: 'Game description', kind: 'text' },
    { source: 'game.year', label: 'Release year', kind: 'number' },
    { source: 'game.players', label: 'Players', kind: 'number' },
    { source: 'game.rating', label: 'Rating (0–1)', kind: 'number' },
    { source: 'system.name', label: 'System name', kind: 'text' },
    { source: 'system.games', label: 'Number of games (System Select)', kind: 'text' },
] as const;
type Source = typeof textSources[number]['source'];
export const previewDatasets = [
    { id: 'sample', label: 'Sample game', values: { 'game.title': 'Sample Adventure', 'game.description': 'Explore a colorful world.\nFind the hidden route home.', 'game.year': 1994, 'game.players': 2, 'game.rating': 0.85, 'system.name': 'Demo Console', 'system.games': '4885 games' } },
    { id: 'long', label: 'Long text / Unicode', values: { 'game.title': 'Sample Adventure: A Very Long Journey ? ??', 'game.description': 'This original sample description tests a busy game-details scene.\nA second line helps check text height and clipping.\nPreview data never enters the saved project.', 'game.year': 2001, 'game.players': 4, 'game.rating': 1, 'system.name': 'Demonstration System With a Long Name', 'system.games': '12 games' } },
    { id: 'missing', label: 'Missing metadata', values: { 'game.title': 'Untitled sample', 'game.description': null, 'game.year': null, 'game.players': null, 'game.rating': null, 'system.name': 'Unknown sample system', 'system.games': null } },
] as const;
export function textBindingEditable(node: ThemeNode) {
    const binding = node.bindings.text;
    return !binding || textSources.some(option => option.source === binding.source && option.kind === binding.kind);
}
export function setTextBinding(node: ThemeNode, source: string): ThemeNode {
    const state = layerState(node);
    if (node.type !== 'text' || state.locked || state.hidden || !textBindingEditable(node))
        return node;
    const current = node.bindings.text;
    if (source === '') {
        if (!current)
            return node;
        const bindings = { ...node.bindings };
        delete bindings.text;
        return { ...node, bindings };
    }
    const option = textSources.find(option => option.source === source);
    if (!option || current?.source === source)
        return node;
    return { ...node, bindings: { ...node.bindings, text: { ...current, source: option.source, kind: option.kind } } };
}
export function sampleText(node: ThemeNode, dataset: string): string {
    const binding = node.bindings.text;
    if (!binding || !textBindingEditable(node))
        return previewText(node);
    const sample = previewDatasets.find(sample => sample.id === dataset);
    if (!sample)
        return previewText(node);
    const value = sample.values[binding.source as Source];
    return value === null || value === undefined ? previewText(node) : String(value).slice(0, 500);
}
