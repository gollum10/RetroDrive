import type { ThemeProject } from './types';
import type { StoredImage } from './api';
export function attachImage(project: ThemeProject, sceneId: string, image: StoredImage, license: string, attribution: string) {
    if (!/^[0-9a-f]{64}$/.test(image.sha256) || image.path !== `assets/images/${image.sha256}.png` || !Number.isInteger(image.bytes) || image.bytes < 1 || image.bytes > 20 * 1024 * 1024 || !Number.isInteger(image.width) || !Number.isInteger(image.height) || image.width < 1 || image.height < 1 || image.width > 4096 || image.height > 4096 || image.width * image.height > 4194304)
        throw new Error('Invalid managed PNG descriptor.');
    const scene = project.scenes.find(scene => scene.id === sceneId);
    if (!scene || project.scenes.reduce((sum, scene) => sum + scene.nodes.length, 0) >= 10000)
        throw new Error('This scene cannot accept another layer.');
    const existing = project.assets.find(asset => asset.sha256 === image.sha256 && asset.mediaType === 'image/png');
    if (existing && (existing.bytes !== image.bytes || existing.path !== image.path))
        throw new Error('Existing image declaration differs; source was preserved.');
    if (!existing && project.assets.length >= 2000)
        throw new Error('The project asset limit is reached.');
    let id = `image-${image.sha256}`;
    let suffix = 1;
    while (!existing && project.assets.some(asset => asset.id === id))
        id = `image-${image.sha256}-${suffix++}`;
    const asset = existing ?? { id, path: image.path, sha256: image.sha256, bytes: image.bytes, mediaType: 'image/png', license: license.trim() || 'unknown', attribution: attribution.trim(), source: 'User-supplied PNG, normalized by RetroDrive' };
    const ids = new Set(project.scenes.flatMap(scene => scene.nodes.map(node => node.id)));
    let index = 1;
    while (ids.has(`image-${index}`))
        index++;
    const nodeId = `image-${index}`;
    const scale = Math.min(scene.resolution.width * .4 / image.width, scene.resolution.height * .4 / image.height);
    const node = { id: nodeId, type: 'image', name: 'Image', layout: { x: { value: 10, unit: '%' as const }, y: { value: 15, unit: '%' as const }, width: { value: image.width * scale, unit: 'px' as const }, height: { value: image.height * scale, unit: 'px' as const }, rotation: 0 }, style: {}, children: [], assets: [asset.id], bindings: {} };
    return { id: nodeId, project: { ...project, assets: existing ? project.assets : [...project.assets, asset], scenes: project.scenes.map(current => current.id === sceneId ? { ...current, nodes: [...current.nodes, node] } : current) } };
}
