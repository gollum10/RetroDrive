import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import ImageLayer from '../../src/theme-studio/ImageLayer';
import ImageControls from '../../src/theme-studio/ImageControls';
import { newDocument } from '../../src/theme-studio/document';
import { attachImage } from '../../src/theme-studio/image-assets';
import '../../src/theme-studio/theme-studio.css';
const canvas = document.createElement('canvas');
canvas.width = 200;
canvas.height = 100;
const context = canvas.getContext('2d')!;
context.fillStyle = '#ff0000';
context.fillRect(0, 0, 100, 100);
context.fillStyle = '#00ff00';
context.fillRect(100, 0, 100, 100);
const url = canvas.toDataURL('image/png');
const project = newDocument({ name: 'Pixel fixture', author: '', description: '', version: '1.0.0', license: 'original', target: '', targetVersion: '', display: '16:9', template: 'blank' });
const attached = attachImage(project, project.scenes[0].id, { sha256: 'c'.repeat(64), path: `assets/images/${'c'.repeat(64)}.png`, bytes: 1000, width: 200, height: 100 }, 'original', 'Fixture creator');
const initial = attached.project.scenes[0].nodes[0];
function Harness() {
    const [node, setNode] = useState(initial);
    const [disabled, setDisabled] = useState(false);
    const [commits, setCommits] = useState(0);
    Object.assign(window, { imageControlsTest: { node, commits, setNode, setDisabled, reset: () => setNode(initial) } });
    return <div className="theme-studio"><svg data-pixel-test="true" xmlns="http://www.w3.org/2000/svg" width={400} height={400}><ImageLayer node={node} url={url} width={400} height={400} opacity={1}/></svg>
    <ImageControls scene={{ ...attached.project.scenes[0], nodes: [node] }} node={node} asset={attached.project.assets[0]} url={url} disabled={disabled} onEnd={() => { }} onChange={next => { setNode(next); setCommits(value => value + 1); }}/>
  </div>;
}
createRoot(document.getElementById('root')!).render(<React.StrictMode><Harness /></React.StrictMode>);
