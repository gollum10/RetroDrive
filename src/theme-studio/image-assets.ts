import type { Layout, ThemeNode, ThemeProject } from './types';

/** Where a new picture goes (ADR-110). `over` needs `overId`, the layer it covers. */
export type Placement = 'free' | 'background' | 'over';
export type AttachOptions = { placement?: Placement; overId?: string | null; aiMade?: boolean };
const AI_NOTE = 'made with an AI tool';
const pct = (value: number) => ({ value, unit: '%' as const });
const fullScreen = (layout: Layout) => (['x', 'y'] as const).every(k => layout[k].value === 0) && (['width', 'height'] as const).every(k => layout[k].unit === '%' && layout[k].value >= 100);
/** Credit text for a picture: says when it was made with an AI tool. */
export function creditFor(attribution: string, aiMade: boolean): string {
    const text = attribution.trim();
    if (!aiMade || /\bAI\b/i.test(text)) return text;
    return text ? `${text} (${AI_NOTE})` : `Made with an AI tool`;
}
import type { StoredImage, StoredVideo } from './api';
export function attachImage(project: ThemeProject, sceneId: string, image: StoredImage, license: string, attribution: string, options: AttachOptions = {}) {
    const placement = options.placement ?? 'free';
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
    const asset = existing ?? { id, path: image.path, sha256: image.sha256, bytes: image.bytes, mediaType: 'image/png', license: license.trim() || 'unknown', attribution: attribution.trim(), source: options.aiMade ? 'User-supplied picture made with an AI tool, normalized by RetroDrive' : 'User-supplied PNG, normalized by RetroDrive' };
    const credit = creditFor(attribution, !!options.aiMade);
    if (!existing) asset.attribution = credit;
    const ids = new Set(project.scenes.flatMap(scene => scene.nodes.map(node => node.id)));
    let index = 1;
    while (ids.has(`image-${index}`))
        index++;
    const nodeId = `image-${index}`;
    const scale = Math.min(scene.resolution.width * .4 / image.width, scene.resolution.height * .4 / image.height);
    let node: ThemeNode = { id: nodeId, type: 'image', name: 'Picture', layout: { x: pct(10), y: pct(15), width: { value: image.width * scale, unit: 'px' as const }, height: { value: image.height * scale, unit: 'px' as const }, rotation: 0 }, style: {}, children: [], assets: [asset.id], bindings: {} };
    let at = scene.nodes.length;
    if (placement === 'background') {
        // Fills the screen behind everything, but above plain full-screen colour panels (such as the Ready layout's background).
        node = { ...node, name: 'Background picture', layout: { x: pct(0), y: pct(0), width: pct(100), height: pct(100), rotation: 0 }, style: { imageFit: 'cover' } };
        at = 0;
        while (at < scene.nodes.length && scene.nodes[at].type === 'panel' && fullScreen(scene.nodes[at].layout)) at++;
    } else if (placement === 'over') {
        const index = scene.nodes.findIndex(n => n.id === options.overId);
        if (index < 0) throw new Error('Select the layer the picture should cover first.');
        const under = scene.nodes[index];
        node = { ...node, name: 'Frame', layout: { ...under.layout, x: { ...under.layout.x }, y: { ...under.layout.y }, width: { ...under.layout.width }, height: { ...under.layout.height } }, style: { imageFit: 'contain' } };
        at = index + 1;
    }
    const nodes = [...scene.nodes.slice(0, at), node, ...scene.nodes.slice(at)];
    return { id: nodeId, project: { ...project, assets: existing ? project.assets : [...project.assets, asset], scenes: project.scenes.map(current => current.id === sceneId ? { ...current, nodes } : current) } };
}

/** Is this layer the user's own clip (a video layer with no game link)? (ADR-117) */
export function isClip(node: ThemeNode): boolean { return node.type === 'video' && !node.bindings.video && node.assets.length === 1; }

/** Add the user's own clip (ADR-117): plays looping and muted where it's placed. */
export function attachVideo(project: ThemeProject, sceneId: string, video: StoredVideo, license: string, attribution: string) {
    if (!/^[0-9a-f]{64}$/.test(video.sha256) || video.path !== `assets/video/${video.sha256}.mp4` || !Number.isInteger(video.bytes) || video.bytes < 1 || video.bytes > 50 * 1024 * 1024)
        throw new Error('Invalid clip descriptor.');
    const scene = project.scenes.find(scene => scene.id === sceneId);
    if (!scene || project.scenes.reduce((sum, scene) => sum + scene.nodes.length, 0) >= 10000) throw new Error('This screen cannot accept another layer.');
    const existing = project.assets.find(asset => asset.sha256 === video.sha256 && asset.mediaType === 'video/mp4');
    let id = `video-${video.sha256}`; let suffix = 1;
    while (!existing && project.assets.some(asset => asset.id === id)) id = `video-${video.sha256}-${suffix++}`;
    const asset = existing ?? { id, path: video.path, sha256: video.sha256, bytes: video.bytes, mediaType: 'video/mp4', license: license.trim() || 'unknown', attribution: attribution.trim(), source: 'User-supplied clip' };
    const ids = new Set(project.scenes.flatMap(scene => scene.nodes.map(node => node.id)));
    let index = 1; while (ids.has(`clip-${index}`)) index++;
    const nodeId = `clip-${index}`;
    const node: ThemeNode = { id: nodeId, type: 'video', name: 'Clip', layout: { x: pct(30), y: pct(30), width: pct(40), height: pct(40), rotation: 0 }, style: {}, children: [], assets: [asset.id], bindings: {} };
    return { id: nodeId, project: { ...project, assets: existing ? project.assets : [...project.assets, asset], scenes: project.scenes.map(current => current.id === sceneId ? { ...current, nodes: [...current.nodes, node] } : current) } };
}

/** Make a layer fill the screen (used for clips as moving backgrounds). */
export function fillScreen(node: ThemeNode): ThemeNode {
    return { ...node, layout: { ...node.layout, x: pct(0), y: pct(0), width: pct(100), height: pct(100), rotation: 0 } };
}
