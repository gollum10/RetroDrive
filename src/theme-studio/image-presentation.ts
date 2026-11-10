import type { ThemeNode } from './types';
import { layerState } from './scene-edit';
export type Crop = {
    x: number;
    y: number;
    width: number;
    height: number;
};
const full: Crop = { x: 0, y: 0, width: 100, height: 100 };
const round = (value: number) => Math.round(value * 1e6) / 1e6;
export function imagePresentation(node: ThemeNode) {
    const fit = node.style.imageFit;
    const raw = node.style.imageCrop;
    const object = raw !== null && typeof raw === 'object' && !Array.isArray(raw) ? raw : null;
    const crop = object as Crop | null;
    const cropEditable = raw === undefined || !!(crop && ['x', 'y', 'width', 'height'].every(key => typeof object![key] === 'number' && Number.isFinite(object![key])) && crop.x >= 0 && crop.y >= 0 && crop.width >= .1 && crop.height >= .1 && crop.x + crop.width <= 100.000001 && crop.y + crop.height <= 100.000001);
    return { fit: fit === 'cover' ? 'cover' as const : 'contain' as const, fitEditable: fit === undefined || fit === 'contain' || fit === 'cover', crop: cropEditable && crop ? { x: crop.x, y: crop.y, width: crop.width, height: crop.height } : full, cropEditable };
}
export function setImageFit(node: ThemeNode, fit: 'contain' | 'cover') {
    const state = imagePresentation(node);
    const protectedState = layerState(node);
    if (node.type !== 'image' || protectedState.locked || protectedState.hidden || !state.fitEditable || state.fit === fit)
        return node;
    return { ...node, style: { ...node.style, imageFit: fit } };
}
export function setImageCrop(node: ThemeNode, key: keyof Crop, value: number) {
    const state = imagePresentation(node);
    const protectedState = layerState(node);
    if (node.type !== 'image' || protectedState.locked || protectedState.hidden || !state.cropEditable || !Number.isFinite(value))
        return node;
    const crop = { ...state.crop, [key]: round(value) };
    if (key === 'x')
        crop.width = round(Math.min(crop.width, 100 - crop.x));
    if (key === 'y')
        crop.height = round(Math.min(crop.height, 100 - crop.y));
    const candidate = { ...node, style: { ...node.style, imageCrop: { ...(node.style.imageCrop && typeof node.style.imageCrop === 'object' && !Array.isArray(node.style.imageCrop) ? node.style.imageCrop : {}), ...crop } } };
    return imagePresentation(candidate).cropEditable && JSON.stringify(crop) !== JSON.stringify(state.crop) ? candidate : node;
}
export function resetImageCrop(node: ThemeNode) {
    const state = imagePresentation(node);
    const protectedState = layerState(node);
    if (node.type !== 'image' || protectedState.locked || protectedState.hidden || !state.cropEditable || Object.entries(full).every(([key, value]) => state.crop[key as keyof Crop] === value))
        return node;
    return { ...node, style: { ...node.style, imageCrop: { ...(node.style.imageCrop && typeof node.style.imageCrop === 'object' && !Array.isArray(node.style.imageCrop) ? node.style.imageCrop : {}), ...full } } };
}
export function cropPixels(crop: Crop, width: number, height: number) {
    return { x: crop.x / 100 * width, y: crop.y / 100 * height, width: crop.width / 100 * width, height: crop.height / 100 * height };
}
