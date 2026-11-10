import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { newDocument } from '../../src/theme-studio/document';
import { updateNode } from '../../src/theme-studio/scene-edit';
import SceneCanvas from '../../src/theme-studio/SceneCanvas';
import NodeInspector from '../../src/theme-studio/NodeInspector';
import '../../src/theme-studio/theme-studio.css';
const initial = newDocument({ name: 'Sample preview fixture', author: '', description: '', version: '1.0.0', license: 'original', target: '', targetVersion: '', display: '16:9', template: 'starter' });
function Harness() {
    const [project, setProject] = useState(initial);
    const [commits, setCommits] = useState(0);
    const scene = project.scenes.find(scene => scene.role === 'game_list')!;
    const node = scene.nodes[0];
    Object.assign(window, { samplePreview: { source: JSON.stringify(project), commits, node } });
    return <div className="theme-studio" style={{ width: 1000 }}><SceneCanvas scene={scene} selected={node.id} disabled={false} onSelect={() => { }} onMove={() => { }} onResize={() => { }} onEndGroup={() => { }}/>
 <NodeInspector scene={scene} node={node} disabled={false} onEnd={() => { }} onChange={next => { setProject(current => updateNode(current, scene.id, node.id, () => next)); setCommits(value => value + 1); }}/></div>;
}
createRoot(document.getElementById('root')!).render(<React.StrictMode><Harness /></React.StrictMode>);
