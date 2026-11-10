import { textPosition } from './text-presentation';
import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { previewDatasets,sampleText } from './sample-data';
import ImageLayer from './ImageLayer';
import { safePngPreview } from './use-image-previews';
import { snapPosition, snapSize, guides } from './snapping';
import { resizeDelta } from './resize';
import type { Scene } from './types';
import { pixels, pointerToScene, previewColor, rootNodes, editableNode, layerState } from './scene-edit';

type Drag = { id: string; pointerId: number; x: number; y: number; startX: number; startY: number; mode: 'move' | 'resize'; width: number; height: number; rotation: number };
export default function SceneCanvas({ scene, selected, disabled, onSelect, onMove, onResize, onEndGroup, previewImages = {} }: {
  previewImages?: Record<string,string>;
  scene: Scene; selected: string | null; disabled: boolean; onSelect: (id: string | null) => void;
  onMove: (id: string, x: number, y: number, group?: string) => void; onResize: (id: string, width: number, height: number, group?: string) => void; onEndGroup: () => void;
}) {
  const [dataset,setDataset]=useState('sample');
  const [showGrid, setShowGrid] = useState(false);
  const [snapping, setSnapping] = useState(false);
  const [spacing, setSpacing] = useState(16);
  const patternId = `grid-${useId().replace(/:/g, '')}`;
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<Drag | null>(null);
  const [preview, setPreview] = useState<{ id: string; x: number; y: number; width?: number; height?: number } | null>(null);
  const safeImages=useMemo(()=>Object.fromEntries(Object.entries(previewImages).filter(([,value])=>safePngPreview(value))),[previewImages]);
  const roots = useMemo(() => rootNodes(scene), [scene]);
  const visible = roots.filter(node => ['text', 'panel', 'image'].includes(node.type) && node.children.length === 0 && !layerState(node).hidden);
  const cancel = () => { drag.current = null; setPreview(null); };
  const selectedState = scene.nodes.find(node => node.id === selected);
  const locked = selectedState ? layerState(selectedState).locked : false;
  const hidden = selectedState ? layerState(selectedState).hidden : false;
  useEffect(() => { cancel(); }, [scene.id, disabled, locked, hidden]);
  const point = (x: number, y: number) => svg.current ? pointerToScene(x, y, svg.current.getBoundingClientRect(), scene) : null;
  const destination = (x: number, y: number, bypass = false) => {
    const current = drag.current; const pointer = point(x, y);
    if (!current || !pointer) return null;
    const dx = pointer.x - current.startX; const dy = pointer.y - current.startY;
    if (Math.abs(dx) + Math.abs(dy) < 0.000001) return current.mode === 'resize' ? { id: current.id, x: current.x, y: current.y, width: current.width, height: current.height } : { id: current.id, x: current.x, y: current.y };
    if (current.mode === 'resize') {
      const size = resizeDelta(current.width, current.height, current.rotation, dx, dy);
      return { id: current.id, x: current.x, y: current.y, ...(snapping && !bypass ? snapSize(size.width,size.height,spacing) : size) };
    }
    const box = { x: current.x+dx, y: current.y+dy, width: current.width, height: current.height, rotation: current.rotation };
    const tolerance = svg.current ? 6*scene.resolution.width/svg.current.getBoundingClientRect().width : 6;
    return { id: current.id, ...(snapping && !bypass ? snapPosition(box,scene.resolution.width,scene.resolution.height,spacing,tolerance) : { x: box.x, y: box.y }) };
  };
  return <section className="panel ts-canvas" aria-label="Scene canvas"><div className="ts-toolbar"><h3>{scene.name}</h3><small>{scene.resolution.width} × {scene.resolution.height}</small></div>
    <fieldset className="ts-canvas-tools" disabled={disabled || preview !== null}><legend>Canvas aids</legend><label>Preview data<select value={dataset} onChange={event=>setDataset(event.target.value)}><option value="placeholders">Binding placeholders</option>{previewDatasets.map(sample=><option key={sample.id} value={sample.id}>{sample.label}</option>)}</select></label>
      <label><input type="checkbox" checked={showGrid} onChange={event => setShowGrid(event.target.checked)}/>Show grid</label>
      <label><input type="checkbox" checked={snapping} onChange={event => setSnapping(event.target.checked)}/>Snap to grid and canvas</label>
      <label>Grid spacing<select value={spacing} onChange={event => setSpacing(Number(event.target.value))}>{[8,16,32,64].map(value => <option key={value} value={value}>{value} px</option>)}</select></label>
    </fieldset>
    <svg ref={svg} className="ts-canvas-svg" viewBox={`0 0 ${scene.resolution.width} ${scene.resolution.height}`} preserveAspectRatio="none" style={{ aspectRatio: `${scene.resolution.width} / ${scene.resolution.height}` }} tabIndex={0} role="group" aria-label="Editable scene canvas" aria-describedby="ts-canvas-help"
      onPointerDown={event => { if (event.target === event.currentTarget) onSelect(null); }}
      onPointerMove={event => { if (event.pointerId !== drag.current?.pointerId) return; const next = destination(event.clientX, event.clientY, event.altKey); if (next) setPreview(next); }}
      onPointerUp={event => { if (event.pointerId !== drag.current?.pointerId) return; const original = drag.current; const next = destination(event.clientX, event.clientY, event.altKey); cancel(); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); if (disabled || !next || !visible.some(node => node.id === next.id && editableNode(scene,node))) return; if (original.mode === 'resize' && 'width' in next && next.width !== undefined && next.height !== undefined) { if (Math.abs(next.width - original.width) + Math.abs(next.height - original.height) > 0.000001) onResize(next.id, next.width, next.height); } else if (Math.abs(next.x - original.x) + Math.abs(next.y - original.y) > 0.000001) onMove(next.id, next.x, next.y); }}
      onPointerCancel={cancel} onLostPointerCapture={cancel} onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) { cancel(); onEndGroup(); } }}
      onKeyUp={event => { if (event.key.startsWith('Arrow')) onEndGroup(); }}
      onKeyDown={event => {
        if ((event.ctrlKey || event.metaKey) && ['z', 'y'].includes(event.key.toLowerCase())) { cancel(); return; }
        if (event.key === 'Escape') { cancel(); return; }
        const node = visible.find(n => n.id === selected);
        if (disabled || !node || !editableNode(scene,node) || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
        event.preventDefault(); if (drag.current) { cancel(); return; } const step = event.shiftKey ? 10 : 1;
        onMove(node.id, pixels(node.layout.x, scene.resolution.width) + (event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0), pixels(node.layout.y, scene.resolution.height) + (event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0), `keyboard-move:${node.id}`);
      }}>
      {showGrid && <g pointerEvents="none" aria-hidden="true"><defs><pattern id={patternId} width={spacing} height={spacing} patternUnits="userSpaceOnUse"><path d={`M ${spacing} 0 H 0 V ${spacing}`} fill="none" stroke="#648099" strokeOpacity={0.35} strokeWidth={0.75}/></pattern></defs><rect width={scene.resolution.width} height={scene.resolution.height} fill={`url(#${patternId})`}/></g>}
      {visible.map(node => {
        const x = preview?.id === node.id ? preview.x : pixels(node.layout.x, scene.resolution.width);
        const y = preview?.id === node.id ? preview.y : pixels(node.layout.y, scene.resolution.height);
        const width = preview?.id === node.id && preview.width !== undefined ? preview.width : pixels(node.layout.width, scene.resolution.width); const height = preview?.id === node.id && preview.height !== undefined ? preview.height : pixels(node.layout.height, scene.resolution.height);
        const textPlacement = textPosition(node, width);
        const font = node.style.fontSize;
        const fontSize = font && typeof font === 'object' && !Array.isArray(font) && font.unit === 'px' && typeof font.value === 'number' ? Math.max(8, Math.min(256, font.value)) : 36;
        const opacity = typeof node.style.opacity === 'number' ? Math.max(0, Math.min(1, node.style.opacity)) : 1;
        return <g key={node.id} data-node-id={node.id} transform={`translate(${x} ${y}) rotate(${node.layout.rotation})`} style={{ cursor: disabled || !editableNode(scene,node) ? 'default' : 'move' }}
          onPointerDown={event => { if (disabled || event.button !== 0) return; event.stopPropagation(); svg.current?.focus(); onSelect(node.id); if (!editableNode(scene,node)) return; const start = point(event.clientX, event.clientY); if (!start) return; drag.current = { id: node.id, pointerId: event.pointerId, x, y, startX: start.x, startY: start.y, mode: 'move', width, height, rotation: node.layout.rotation }; svg.current?.setPointerCapture(event.pointerId); }}>
          <title>{node.name}</title><rect width={width} height={height} fill={node.type === 'panel' ? previewColor(node.style.fill, '#263f60') : 'transparent'} opacity={opacity}/>
          {node.type === 'image' && (safeImages[node.assets[0]] ? <ImageLayer node={node} url={safeImages[node.assets[0]]} width={width} height={height} opacity={opacity}/> : <text x={4} y={20} fill="#abc0d9" fontSize={18} pointerEvents="none">Image unavailable</text>)}
          {node.type === 'text' && <svg width={width} height={height} overflow="hidden" pointerEvents="none"><text x={textPlacement.x} textAnchor={textPlacement.anchor} y={0} dominantBaseline="hanging" fontFamily="sans-serif" fontSize={fontSize} fill={previewColor(node.style.color, '#ffffff')} opacity={opacity}>{sampleText(node,dataset).split('\n').slice(0, 20).map((line, index) => <tspan key={index} x={textPlacement.x} dy={index ? '1.2em' : 0}>{line || ' '}</tspan>)}</text></svg>}
        </g>;
      })}
      {(() => {
        const node = visible.find(node => node.id === selected); if (!node) return null;
        const x = preview?.id === node.id ? preview.x : pixels(node.layout.x, scene.resolution.width);
        const y = preview?.id === node.id ? preview.y : pixels(node.layout.y, scene.resolution.height);
        const width = preview?.id === node.id && preview.width !== undefined ? preview.width : pixels(node.layout.width, scene.resolution.width);
        const height = preview?.id === node.id && preview.height !== undefined ? preview.height : pixels(node.layout.height, scene.resolution.height);
        return <g transform={`translate(${x} ${y}) rotate(${node.layout.rotation})`}>
          <rect width={width} height={height} fill="none" stroke="#65b6ff" strokeWidth={2} vectorEffect="non-scaling-stroke" pointerEvents="none"/>
          {!disabled && editableNode(scene,node) && <circle cx={width} cy={height} r={scene.resolution.width / 90} fill="#65b6ff" stroke="#101b2c" strokeWidth={2} vectorEffect="non-scaling-stroke" tabIndex={0} role="button" aria-label={`Resize ${node.name || node.type}`} style={{ cursor: 'nwse-resize' }}
            onPointerDown={event => { if (event.button !== 0) return; event.preventDefault(); event.stopPropagation(); event.currentTarget.focus(); const start = point(event.clientX, event.clientY); if (!start) return; drag.current = { id: node.id, pointerId: event.pointerId, x, y, startX: start.x, startY: start.y, mode: 'resize', width, height, rotation: node.layout.rotation }; svg.current?.setPointerCapture(event.pointerId); }}
            onKeyDown={event => {
              if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
              event.preventDefault(); event.stopPropagation(); if (drag.current) { cancel(); return; } const step = event.shiftKey ? 10 : 1;
              onResize(node.id, event.key === 'ArrowLeft' ? Math.max(1, width - step) : event.key === 'ArrowRight' ? width + step : width, event.key === 'ArrowUp' ? Math.max(1, height - step) : event.key === 'ArrowDown' ? height + step : height, `keyboard-resize:${node.id}`);
            }}/>}<title>Resize bottom-right corner; arrow keys adjust local width/height.</title>
        </g>;
      })()}
      {preview && (() => { const node = visible.find(node => node.id === preview.id); if (!node) return null;
        const lines = guides({ x: preview.x, y: preview.y, width: preview.width ?? pixels(node.layout.width,scene.resolution.width), height: preview.height ?? pixels(node.layout.height,scene.resolution.height), rotation: node.layout.rotation },scene.resolution.width,scene.resolution.height);
        return <g data-canvas-guides="true" pointerEvents="none" aria-hidden="true" stroke="#f4c96b" strokeWidth={1} strokeDasharray="6 4">{lines.x.map(x => <line key={`x${x}`} x1={x} x2={x} y1={0} y2={scene.resolution.height} vectorEffect="non-scaling-stroke"/>)}{lines.y.map(y => <line key={`y${y}`} x1={0} x2={scene.resolution.width} y1={y} y2={y} vectorEffect="non-scaling-stroke"/>)}</g>;
      })()}
    </svg>
    <p>Sample data is synthetic and session-only; no emulator or personal library is read. Grid and snapping are session-only aids. Hold Alt to bypass pointer snapping; arrow keys keep precise steps. Guides mark canvas edges and centers.</p>
    <p id="ts-canvas-help">Select a layer and drag to move it, or focus the canvas and use arrow keys (Shift: 10 pixels). Drag the bottom-right handle to resize; focus it and use arrows for numeric steps. Escape cancels a drag or resize. Editor preview; native frontend behavior is unverified.</p>
    {visible.length < scene.nodes.length && <p>Only supported top-level layers are drawn here. Other layers and their source data are preserved.</p>}
  </section>;
}
