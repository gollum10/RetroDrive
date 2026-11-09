import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import ImageImportForm from '../../src/theme-studio/ImageImportForm';
import { attachImage } from '../../src/theme-studio/image-assets';
import LayerList from '../../src/theme-studio/LayerList';
import SceneCanvas from '../../src/theme-studio/SceneCanvas';
import { newDocument } from '../../src/theme-studio/document';
import { addNode, moveNode, updateNode, transformNode, setLayerState } from '../../src/theme-studio/scene-edit';
import { resizeLayer } from '../../src/theme-studio/resize';
import '../../src/theme-studio/theme-studio.css';

const initial = newDocument({ name: 'Browser regression', author: '', description: '', version: '1.0.0', license: 'original', target: '', targetVersion: '', display: '16:9', template: 'blank' });
const added = addNode(initial, initial.scenes[0].id, 'panel');
function Harness() {
  const [document, setDocument] = useState(added.project);
  const [selected,setSelected]=useState<string|null>(added.id);
  const [previewImages,setPreviewImages]=useState<Record<string,string>>({});
  const [commits, setCommits] = useState(0);
  const [workspaceWidth, setWorkspaceWidth] = useState(850);
  const [disabled, setDisabled] = useState(false);
  Object.assign(window, { canvasTest: { document, commits, setWorkspaceWidth, setDisabled, resetDocument: () => {setDocument(added.project);setSelected(added.id);setPreviewImages({});}, setRotation: (angle: number) => setDocument(current => updateNode(current, initial.scenes[0].id, added.id, node => transformNode(node, 'rotation', angle))) } });
  return <div className="theme-studio" style={{ width: workspaceWidth }}><div className="ts-editor-grid">
    <aside className="ts-scenes"><LayerList scene={document.scenes[0]} selected={selected} disabled={disabled} onSelect={setSelected} onState={(id,key,value) => { setDocument(current => setLayerState(current,initial.scenes[0].id,id,key,value)); setCommits(count => count+1); }}/></aside>
    <SceneCanvas previewImages={previewImages} scene={document.scenes[0]} selected={selected} disabled={disabled} onSelect={setSelected}
      onMove={(id, x, y) => { setDocument(current => moveNode(current, initial.scenes[0].id, id, x, y)); setCommits(count => count + 1); }}
      onResize={(id, width, height) => { setDocument(current => resizeLayer(current, initial.scenes[0].id, id, width, height)); setCommits(count => count + 1); }} onEndGroup={() => {}}/>
    <aside className="ts-inspector">Inspector<input aria-label="Outside canvas"/></aside>
  </div><ImageImportForm sceneName="Browser fixture" onCancel={()=>{}} onImport={async(encoded,license,attribution)=>{
    const next=attachImage(document,initial.scenes[0].id,{sha256:'b'.repeat(64),path:`assets/images/${'b'.repeat(64)}.png`,bytes:atob(encoded).length,width:1,height:1},license,attribution);
    setDocument(next.project);setSelected(next.id);setPreviewImages(current=>({...current,[next.project.assets[0].id]:`data:image/png;base64,${encoded}`}));setCommits(count=>count+1);
  }}/></div>;
}
createRoot(document.getElementById('root')!).render(<Harness/>);
