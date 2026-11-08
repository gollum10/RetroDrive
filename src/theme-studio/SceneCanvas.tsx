import { useEffect, useMemo, useRef, useState } from 'react';
import { resizeDelta } from './resize';
import type { Scene } from './types';
import { pixels, pointerToScene, previewColor, previewText, rootNodes } from './scene-edit';

type Drag = { id: string; pointerId: number; x: number; y: number; startX: number; startY: number; mode: 'move' | 'resize'; width: number; height: number; rotation: number };
export default function SceneCanvas({ scene, selected, disabled, onSelect, onMove, onResize, onEndGroup }: {
  scene: Scene; selected: string | null; disabled: boolean; onSelect: (id: string | null) => void;
  onMove: (id: string, x: number, y: number, group?: string) => void; onResize: (id: string, width: number, height: number, group?: string) => void; onEndGroup: () => void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<Drag | null>(null);
  const [preview, setPreview] = useState<{ id: string; x: number; y: number; width?: number; height?: number } | null>(null);
  const roots = useMemo(() => rootNodes(scene), [scene]);
  const visible = roots.filter(node => ['text', 'panel'].includes(node.type) && node.children.length === 0);
  const cancel = () => { drag.current = null; setPreview(null); };
  useEffect(() => { cancel(); }, [scene.id, disabled]);
  const point = (x: number, y: number) => svg.current ? pointerToScene(x, y, svg.current.getBoundingClientRect(), scene) : null;
  const destination = (x: number, y: number) => {
    const current = drag.current; const pointer = point(x, y);
    if (!current || !pointer) return null;
    const dx = pointer.x - current.startX; const dy = pointer.y - current.startY;
    if (Math.abs(dx) + Math.abs(dy) < 0.000001) return current.mode === 'resize' ? { id: current.id, x: current.x, y: current.y, width: current.width, height: current.height } : { id: current.id, x: current.x, y: current.y };
    return current.mode === 'resize' ? { id: current.id, x: current.x, y: current.y, ...resizeDelta(current.width, current.height, current.rotation, dx, dy) }
      : { id: current.id, x: current.x + dx, y: current.y + dy };
  };
  return <section className="panel ts-canvas" aria-label="Scene canvas"><div className="ts-toolbar"><h3>{scene.name}</h3><small>{scene.resolution.width} × {scene.resolution.height}</small></div>
    <svg ref={svg} className="ts-canvas-svg" viewBox={`0 0 ${scene.resolution.width} ${scene.resolution.height}`} preserveAspectRatio="none" style={{ aspectRatio: `${scene.resolution.width} / ${scene.resolution.height}` }} tabIndex={0} role="group" aria-label="Editable scene canvas" aria-describedby="ts-canvas-help"
      onPointerDown={event => { if (event.target === event.currentTarget) onSelect(null); }}
      onPointerMove={event => { if (event.pointerId !== drag.current?.pointerId) return; const next = destination(event.clientX, event.clientY); if (next) setPreview(next); }}
      onPointerUp={event => { if (event.pointerId !== drag.current?.pointerId) return; const original = drag.current; const next = destination(event.clientX, event.clientY); cancel(); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); if (disabled || !next) return; if (original.mode === 'resize' && 'width' in next && next.width !== undefined && next.height !== undefined) { if (Math.abs(next.width - original.width) + Math.abs(next.height - original.height) > 0.000001) onResize(next.id, next.width, next.height); } else if (Math.abs(next.x - original.x) + Math.abs(next.y - original.y) > 0.000001) onMove(next.id, next.x, next.y); }}
      onPointerCancel={cancel} onLostPointerCapture={cancel} onBlur={() => { cancel(); onEndGroup(); }}
      onKeyUp={event => { if (event.key.startsWith('Arrow')) onEndGroup(); }}
      onKeyDown={event => {
        if ((event.ctrlKey || event.metaKey) && ['z', 'y'].includes(event.key.toLowerCase())) { cancel(); return; }
        if (event.key === 'Escape') { cancel(); return; }
        const node = visible.find(n => n.id === selected);
        if (disabled || !node || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
        event.preventDefault(); if (drag.current) { cancel(); return; } const step = event.shiftKey ? 10 : 1;
        onMove(node.id, pixels(node.layout.x, scene.resolution.width) + (event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0), pixels(node.layout.y, scene.resolution.height) + (event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0), `keyboard-move:${node.id}`);
      }}>
      {visible.map(node => {
        const x = preview?.id === node.id ? preview.x : pixels(node.layout.x, scene.resolution.width);
        const y = preview?.id === node.id ? preview.y : pixels(node.layout.y, scene.resolution.height);
        const width = preview?.id === node.id && preview.width !== undefined ? preview.width : pixels(node.layout.width, scene.resolution.width); const height = preview?.id === node.id && preview.height !== undefined ? preview.height : pixels(node.layout.height, scene.resolution.height);
        const font = node.style.fontSize;
        const fontSize = font && typeof font === 'object' && !Array.isArray(font) && font.unit === 'px' && typeof font.value === 'number' ? Math.max(8, Math.min(256, font.value)) : 36;
        const opacity = typeof node.style.opacity === 'number' ? Math.max(0, Math.min(1, node.style.opacity)) : 1;
        return <g key={node.id} data-node-id={node.id} transform={`translate(${x} ${y}) rotate(${node.layout.rotation})`} style={{ cursor: disabled ? 'default' : 'move' }}
          onPointerDown={event => { if (disabled || event.button !== 0) return; event.stopPropagation(); svg.current?.focus(); onSelect(node.id); const start = point(event.clientX, event.clientY); if (!start) return; drag.current = { id: node.id, pointerId: event.pointerId, x, y, startX: start.x, startY: start.y, mode: 'move', width, height, rotation: node.layout.rotation }; svg.current?.setPointerCapture(event.pointerId); }}>
          <title>{node.name}</title><rect width={width} height={height} fill={node.type === 'panel' ? previewColor(node.style.fill, '#263f60') : 'transparent'} opacity={opacity}/>
          {node.type === 'text' && <svg width={width} height={height} overflow="hidden" pointerEvents="none"><text x={0} y={0} dominantBaseline="hanging" fontFamily="sans-serif" fontSize={fontSize} fill={previewColor(node.style.color, '#ffffff')} opacity={opacity}>{previewText(node).split('\n').slice(0, 20).map((line, index) => <tspan key={index} x={0} dy={index ? '1.2em' : 0}>{line || ' '}</tspan>)}</text></svg>}
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
          {!disabled && <circle cx={width} cy={height} r={scene.resolution.width / 90} fill="#65b6ff" stroke="#101b2c" strokeWidth={2} vectorEffect="non-scaling-stroke" tabIndex={0} role="button" aria-label={`Resize ${node.name || node.type}`} style={{ cursor: 'nwse-resize' }}
            onPointerDown={event => { if (event.button !== 0) return; event.stopPropagation(); svg.current?.focus(); const start = point(event.clientX, event.clientY); if (!start) return; drag.current = { id: node.id, pointerId: event.pointerId, x, y, startX: start.x, startY: start.y, mode: 'resize', width, height, rotation: node.layout.rotation }; svg.current?.setPointerCapture(event.pointerId); }}
            onKeyDown={event => {
              if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
              event.preventDefault(); event.stopPropagation(); if (drag.current) { cancel(); return; } const step = event.shiftKey ? 10 : 1;
              onResize(node.id, event.key === 'ArrowLeft' ? Math.max(1, width - step) : event.key === 'ArrowRight' ? width + step : width, event.key === 'ArrowUp' ? Math.max(1, height - step) : event.key === 'ArrowDown' ? height + step : height, `keyboard-resize:${node.id}`);
            }}/>}<title>Resize bottom-right corner; arrow keys adjust local width/height.</title>
        </g>;
      })()}
    </svg>
    <p id="ts-canvas-help">Select a layer and drag to move it, or focus the canvas and use arrow keys (Shift: 10 pixels). Drag the bottom-right handle to resize; focus it and use arrows for numeric steps. Escape cancels a drag or resize. Editor preview; native frontend behavior is unverified.</p>
    {visible.length < scene.nodes.length && <p>Only top-level text and panels are drawn here. Other layers and their source data are preserved.</p>}
  </section>;
}
