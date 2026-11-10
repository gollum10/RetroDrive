// Save for RetroBat (ADR-113): turn a theme into an export document RetroBat can show.
// Everything static on a screen (panels, pictures, plain text, rotation, fading) is drawn into
// full-screen pictures exactly as the editor shows it ("baked"); the parts that change per game
// (game list, game video, text linked to game data) stay as RetroBat's own elements.
import type { Asset, Scene, ThemeNode, ThemeProject } from './types';
import { layerState, pixels, previewColor } from './scene-edit';
import { imagePresentation, cropPixels } from './image-presentation';
import { textPosition } from './text-presentation';

/** RetroBat screens a theme can style, in the order they're saved. */
export const RETROBAT_SCREENS = [['system_select', 'System Select'], ['game_list', 'Game List']] as const;
const LIVE_TEXT = new Set(['game.title', 'game.description', 'system.name', 'game.players', 'game.year']);
const ROLE_NAMES: Record<string, string> = { startup: 'Startup', main_menu: 'Main Menu', game_detail: 'Game Detail', launch: 'Launch', idle: 'Idle', exit: 'Exit' };

export type Group = { kind: 'bake'; nodes: ThemeNode[] } | { kind: 'live'; node: ThemeNode };
export type ScreenPlan = { scene: Scene; groups: Group[] };
export type ExportPlan = { screens: ScreenPlan[]; notes: string[]; live: string[]; bakeCount: number };

/** Is this layer one RetroBat draws itself (changes per game) on this screen? */
export function liveLayer(scene: Scene, node: ThemeNode): boolean {
  if (node.type === 'video' && !node.bindings.video && node.assets.length === 1) return true; // your own clip (ADR-117)
  if (scene.role === 'system_select') return node.type === 'system_carousel' || (node.type === 'text' && node.bindings.text?.source === 'system.games');
  if (scene.role !== 'game_list') return false;
  if (node.type === 'game_list') return true;
  if (node.type === 'video') return node.bindings.video?.source === 'game.video';
  return node.type === 'text' && !!node.bindings.text && LIVE_TEXT.has(node.bindings.text.source);
}

/** What will be saved, and what can't be (with plain reasons). Nothing is changed. */
export function planExport(project: ThemeProject): ExportPlan {
  const notes: string[] = []; const live = new Set<string>(); let bakeCount = 0;
  const skipped = project.scenes.filter(s => !RETROBAT_SCREENS.some(([role]) => role === s.role) && s.nodes.length > 0).map(s => ROLE_NAMES[s.role] ?? s.name);
  if (skipped.length) notes.push(`RetroBat themes style the System Select and Game List screens. Your ${skipped.join(', ')} ${skipped.length === 1 ? 'screen isn\'t' : 'screens aren\'t'} included; they stay in your design.`);
  const screens: ScreenPlan[] = [];
  for (const [role] of RETROBAT_SCREENS) {
    const scene = project.scenes.find(s => s.role === role);
    if (!scene) continue;
    const groups: Group[] = [];
    for (const node of scene.nodes) {
      if (layerState(node).hidden || node.children.length) continue;
      if (liveLayer(scene, node)) {
        live.add(node.type === 'game_list' ? 'the game list' : node.type === 'video' ? (node.bindings.video ? 'the game video' : `your clip “${node.name}”`) : node.type === 'system_carousel' ? 'the system carousel' : `“${node.name}”`);
        groups.push({ kind: 'live', node });
        if (node.layout.rotation !== 0 || (typeof node.style.opacity === 'number' && node.style.opacity !== 1)) notes.push(`“${node.name}” is shown upright and solid in RetroBat (it can't turn or fade changing text, lists or videos).`);
        continue;
      }
      if (node.bindings.text || node.bindings.video || node.type === 'video' || node.type === 'game_list' || node.type === 'system_carousel') {
        notes.push(role === 'system_select'
          ? `“${node.name}” isn't included on System Select: RetroBat shows each system's name and logo itself.`
          : `“${node.name}” isn't included: RetroBat can't show that game detail there yet.`);
        continue;
      }
      if (!['text', 'panel', 'image'].includes(node.type)) { notes.push(`“${node.name}” isn't included (this kind of layer can't be saved yet).`); continue; }
      const last = groups[groups.length - 1];
      if (last?.kind === 'bake') last.nodes.push(node); else { groups.push({ kind: 'bake', nodes: [node] }); bakeCount++; }
    }
    if (role === 'system_select' && !groups.some(g => g.kind === 'live' && g.node.type === 'system_carousel')) notes.push("Your System Select screen has no system carousel layer, so RetroBat's own light band is used. Add a System carousel layer to place and colour it.");
    if (role === 'game_list' && !groups.some(g => g.kind === 'live' && g.node.type === 'game_list')) notes.push("Your Game List screen has no game list layer, so RetroBat's list goes on the left with standard colours. Add a Game list layer to place and colour it yourself.");

    screens.push({ scene, groups });
  }
  return { screens, notes, live: [...live], bakeCount };
}

/** Load the theme's pictures used on these screens as drawable images. */
export async function loadPictures(project: ThemeProject, screens: ScreenPlan[], preview: (assetId: string) => Promise<string>): Promise<Map<string, HTMLImageElement>> {
  const ids = new Set(screens.flatMap(s => s.groups.flatMap(g => g.kind === 'bake' ? g.nodes.filter(n => n.type === 'image').flatMap(n => n.assets) : [])));
  const out = new Map<string, HTMLImageElement>();
  // One at a time: the picture store serves one request at a time (and the editor may be
  // loading previews too), so a busy store is retried briefly.
  for (const id of ids) {
    let url = '';
    for (let attempt = 0; ; attempt++) {
      try { url = await preview(id); break; }
      catch (error) {
        const busy = typeof error === 'object' && error !== null && (error as { code?: string }).code === 'image_busy';
        if (!busy || attempt >= 20) throw error;
        await new Promise(resolve => setTimeout(resolve, 150));
      }
    }
    if (!url.startsWith('data:image/png;base64,')) throw new Error('A picture preview was refused.');
    const image = new Image(); image.src = url; await image.decode(); out.set(id, image);
  }
  return out;
}

/** Draw layers onto a 2D context the way the editor's canvas shows them. Returns whether anything was drawn. */
export function drawLayers(context: CanvasRenderingContext2D, scene: Scene, nodes: ThemeNode[], pictures: Map<string, HTMLImageElement>): boolean {
  let drew = false;
  for (const node of nodes) {
    const x = pixels(node.layout.x, scene.resolution.width), y = pixels(node.layout.y, scene.resolution.height);
    const w = pixels(node.layout.width, scene.resolution.width), h = pixels(node.layout.height, scene.resolution.height);
    if (!(w > 0 && h > 0)) continue;
    const opacity = typeof node.style.opacity === 'number' ? Math.max(0, Math.min(1, node.style.opacity)) : 1;
    if (opacity === 0) continue;
    context.save();
    context.globalAlpha = opacity;
    context.translate(x, y); context.rotate(node.layout.rotation * Math.PI / 180);
    context.beginPath(); context.rect(0, 0, w, h); context.clip();
    if (node.type === 'panel') { context.fillStyle = previewColor(node.style.fill, '#263f60'); context.fillRect(0, 0, w, h); drew = true; }
    else if (node.type === 'image') {
      const image = pictures.get(node.assets[0]);
      if (image) {
        const state = imagePresentation(node); const crop = cropPixels(state.crop, image.naturalWidth, image.naturalHeight);
        const scale = (state.fit === 'cover' ? Math.max : Math.min)(w / crop.width, h / crop.height);
        const dw = crop.width * scale, dh = crop.height * scale;
        context.drawImage(image, crop.x, crop.y, crop.width, crop.height, (w - dw) / 2, (h - dh) / 2, dw, dh); drew = true;
      }
    } else if (node.type === 'text') {
      const font = node.style.fontSize; const size = font && typeof font === 'object' && !Array.isArray(font) && font.unit === 'px' && typeof font.value === 'number' ? Math.max(8, Math.min(256, font.value)) : 36;
      const placement = textPosition(node, w);
      context.font = `${size}px sans-serif`; context.textBaseline = 'top';
      context.textAlign = placement.anchor === 'middle' ? 'center' : placement.anchor === 'end' ? 'right' : 'left';
      context.fillStyle = previewColor(node.style.color, '#ffffff');
      const text = (typeof node.style.text === 'string' ? node.style.text : node.name).slice(0, 500);
      text.split('\n').slice(0, 20).forEach((line, i) => context.fillText(line, placement.x, i * size * 1.2));
      drew = drew || text.trim().length > 0;
    }
    context.restore();
  }
  return drew;
}

/** A baked picture ready to send: PNG as base64, and how the backend says to declare it. */
export type Baked = { encoded: string; info: { sha256: string; path: string; bytes: number; width: number; height: number } };

/** Bake each static group to a full-screen PNG (skipping empty ones). */
export async function bakeScreens(plan: ExportPlan, pictures: Map<string, HTMLImageElement>, prepare: (encoded: string) => Promise<Baked['info']>): Promise<Map<Group, Baked>> {
  const out = new Map<Group, Baked>();
  for (const screen of plan.screens) for (const group of screen.groups) {
    if (group.kind !== 'bake') continue;
    const canvas = document.createElement('canvas');
    canvas.width = screen.scene.resolution.width; canvas.height = screen.scene.resolution.height;
    const context = canvas.getContext('2d'); if (!context) throw new Error("RetroDrive couldn't draw the theme's pictures.");
    if (!drawLayers(context, screen.scene, group.nodes, pictures)) continue;
    const blob = await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, 'image/png'));
    if (!blob) throw new Error("RetroDrive couldn't draw the theme's pictures.");
    const bytes = new Uint8Array(await blob.arrayBuffer()); let binary = '';
    for (let i = 0; i < bytes.length; i += 8192) binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
    const encoded = btoa(binary);
    out.set(group, { encoded, info: await prepare(encoded) });
  }
  return out;
}

/** The export document: two screens, baked pictures plus live layers, nothing else. */
export function exportDocument(project: ThemeProject, plan: ExportPlan, baked: Map<Group, Baked>): ThemeProject {
  const assets: Asset[] = [];
  const scenes: Scene[] = plan.screens.map(({ scene, groups }) => {
    const nodes: ThemeNode[] = [];
    groups.forEach((group, index) => {
      if (group.kind === 'live') {
        const { node } = group;
        const style = Object.fromEntries(Object.entries(node.style).filter(([key]) => key !== 'opacity'));
        for (const id of node.assets) { const declared = project.assets.find(a => a.id === id); if (declared && !assets.some(a => a.id === id)) assets.push({ ...declared }); }
        nodes.push({ id: node.id, type: node.type, name: node.name, layout: { x: { ...node.layout.x }, y: { ...node.layout.y }, width: { ...node.layout.width }, height: { ...node.layout.height }, rotation: 0 }, style, children: [], assets: [...node.assets], bindings: node.bindings });
        return;
      }
      const picture = baked.get(group); if (!picture) return;
      const id = `baked-${scene.role}-${index + 1}`;
      if (!assets.some(a => a.sha256 === picture.info.sha256)) assets.push({ id, path: picture.info.path, sha256: picture.info.sha256, bytes: picture.info.bytes, mediaType: 'image/png', license: project.metadata.license || 'unknown', attribution: "This theme's own art, combined by RetroDrive", source: 'Baked by RetroDrive Theme Studio' });
      const assetId = assets.find(a => a.sha256 === picture.info.sha256)!.id;
      const full = { value: 100, unit: '%' as const }, zero = { value: 0, unit: '%' as const };
      nodes.push({ id, type: 'image', name: 'Design', layout: { x: { ...zero }, y: { ...zero }, width: { ...full }, height: { ...full }, rotation: 0 }, style: { imageFit: 'contain' }, children: [], assets: [assetId], bindings: {} });
    });
    return { id: scene.id, role: scene.role, name: scene.name, resolution: { width: scene.resolution.width, height: scene.resolution.height }, nodes };
  });
  const { name, author, description, version, license, tags } = project.metadata;
  return { format: 'retrodrive.theme', schemaVersion: '1.0.0', id: project.id, metadata: { name, author, description, version, license, tags: [...tags] }, targets: [], scenes, assets, tokens: {}, variants: {} };
}
