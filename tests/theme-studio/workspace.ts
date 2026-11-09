import { newDocument, suggestCopyName, type NewTheme } from '../../src/theme-studio/document';
import { ProjectSession, type Writer } from '../../src/theme-studio/session';
import type { ProjectRecord } from '../../src/theme-studio/api';

export const form: NewTheme = { name: 'Original theme', author: 'Creator', description: '', version: '1.0.0', license: 'original', target: '', targetVersion: '', display: 'Portrait', template: 'starter' };
export const project = newDocument(form);
export function record(): ProjectRecord {
  return { id: project.id, name: project.metadata.name, schemaVersion: '1.0.0', savedJson: JSON.stringify(project), draftJson: null, revision: 0, createdAt: '2026-11-08 12:00:00', updatedAt: '2026-11-08 12:00:00', savedAt: '2026-11-08 12:00:00' };
}
export { ProjectSession, newDocument, suggestCopyName };
export { guardedWindowClose } from '../../src/theme-studio/window-close';
export type { Writer };
export { EditHistory } from '../../src/theme-studio/history';
export { canOrderLayers, orderLayer } from '../../src/theme-studio/scene-edit';
export { addNode, moveNode, updateNode, rootNodes, pixels, positionFromPixels, pointerToScene, previewColor } from '../../src/theme-studio/scene-edit';

export { transformNode } from '../../src/theme-studio/scene-edit';
export { alignedNode, alignLayer } from '../../src/theme-studio/alignment';

export { resizeDelta, resizeLayer } from '../../src/theme-studio/resize';

export { bounds, snapSize, snapPosition, guides } from '../../src/theme-studio/snapping';

export { layerState, setLayerState, editableNode } from '../../src/theme-studio/scene-edit';
