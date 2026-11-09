import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import SceneCanvas from '../../src/theme-studio/SceneCanvas';
import { newDocument } from '../../src/theme-studio/document';
import { addNode, moveNode, updateNode, transformNode } from '../../src/theme-studio/scene-edit';
import { resizeLayer } from '../../src/theme-studio/resize';
import '../../src/theme-studio/theme-studio.css';

const initial = newDocument({ name: 'Browser regression', author: '', description: '', version: '1.0.0', license: 'original', target: '', targetVersion: '', display: '16:9', template: 'blank' });
const added = addNode(initial, initial.scenes[0].id, 'panel');
function Harness() {
  const [document, setDocument] = useState(added.project);
  const [commits, setCommits] = useState(0);
  const [workspaceWidth, setWorkspaceWidth] = useState(850);
  const [disabled, setDisabled] = useState(false);
  Object.assign(window, { canvasTest: { document, commits, setWorkspaceWidth, setDisabled, resetDocument: () => setDocument(added.project), setRotation: (angle: number) => setDocument(current => updateNode(current, initial.scenes[0].id, added.id, node => transformNode(node, 'rotation', angle))) } });
  return <div className="theme-studio" style={{ width: workspaceWidth }}><div className="ts-editor-grid">
    <aside className="ts-scenes">Scenes</aside>
    <SceneCanvas scene={document.scenes[0]} selected={added.id} disabled={disabled} onSelect={() => {}}
      onMove={(id, x, y) => { setDocument(current => moveNode(current, initial.scenes[0].id, id, x, y)); setCommits(count => count + 1); }}
      onResize={(id, width, height) => { setDocument(current => resizeLayer(current, initial.scenes[0].id, id, width, height)); setCommits(count => count + 1); }} onEndGroup={() => {}}/>
    <aside className="ts-inspector">Inspector<input aria-label="Outside canvas"/></aside>
  </div></div>;
}
createRoot(document.getElementById('root')!).render(<Harness/>);
