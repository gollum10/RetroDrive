import { useEffect, useMemo, useRef, useState } from 'react';
import type { Scene } from './types';
import { pixels, pointerToScene, previewColor, previewText, rootNodes } from './scene-edit';

type Drag = { id: string; pointerId: number; x: number; y: number; startX: number; startY: number };
export default function SceneCanvas({ scene, selected, disabled, onSelect, onMove, onEndGroup }: {
  scene: Scene; selected: string | null; disabled: boolean; onSelect: (id: string | null) => void;
  onMove: (id: string, x: number, y: number, group?: string) => void; onEndGroup: () => void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<Drag | null>(null);
  const [preview, setPreview] = useState<{ id: string; x: number; y: number } | null>(null);
  const roots = useMemo(() => rootNodes(scene), [scene]);
  const visible = roots.filter(node => ['text', 'panel'].includes(node.type) && node.children.length === 0);
  const cancel = () => { drag.current = null; setPreview(null); };
  useEffect(() => { cancel(); }, [scene.id, disabled]);
  const point = (x: number, y: number) => svg.current ? pointerToScene(x, y, svg.current.getBoundingClientRect(), scene) : null;
  const destination = (x: number, y: number) => {
    const current = drag.current; const pointer = point(x, y);
    return current && pointer ? { id: current.id, x: current.x + pointer.x - current.startX, y: current.y + pointer.y - current.startY } : null;
  };
  return <section className="panel ts-canvas" aria-label="Scene canvas"><div className="ts-toolbar"><h3>{scene.name}</h3><small>{scene.resolution.width} × {scene.resolution.height}</small></div>
    <svg ref={svg} className="ts-canvas-svg" viewBox={`0 0 ${scene.resolution.width} ${scene.resolution.height}`} preserveAspectRatio="none" style={{ aspectRatio: `${scene.resolution.width} / ${scene.resolution.height}` }} tabIndex={0} role="group" aria-label="Editable scene canvas" aria-describedby="ts-canvas-help"
      onPointerDown={event => { if (event.target === event.currentTarget) onSelect(null); }}
      onPointerMove={event => { if (event.pointerId !== drag.current?.pointerId) return; const next = destination(event.clientX, event.clientY); if (next) setPreview(next); }}
      onPointerUp={event => { if (event.pointerId !== drag.current?.pointerId) return; const original = drag.current; const next = destination(event.clientX, event.clientY); cancel(); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); if (!disabled && next && Math.abs(next.x - original.x) + Math.abs(next.y - original.y) > 0.000001) onMove(next.id, next.x, next.y); }}
      onPointerCancel={cancel} onLostPointerCapture={cancel} onBlur={() => { cancel(); onEndGroup(); }}
      onKeyUp={event => { if (event.key.startsWith('Arrow')) onEndGroup(); }}
      onKeyDown={event => {
        if ((event.ctrlKey || event.metaKey) && ['z', 'y'].includes(event.key.toLowerCase())) { cancel(); return; }
        if (event.key === 'Escape') { cancel(); return; }
        const node = visible.find(n => n.id === selected);
        if (disabled || !node || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
        event.preventDefault(); const step = event.shiftKey ? 10 : 1;
        onMove(node.id, pixels(node.layout.x, scene.resolution.width) + (event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0), pixels(node.layout.y, scene.resolution.height) + (event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0), `keyboard-move:${node.id}`);
      }}>
      {visible.map(node => {
        const x = preview?.id === node.id ? preview.x : pixels(node.layout.x, scene.resolution.width);
        const y = preview?.id === node.id ? preview.y : pixels(node.layout.y, scene.resolution.height);
        const width = pixels(node.layout.width, scene.resolution.width); const height = pixels(node.layout.height, scene.resolution.height);
        const font = node.style.fontSize;
        const fontSize = font && typeof font === 'object' && !Array.isArray(font) && font.unit === 'px' && typeof font.value === 'number' ? Math.max(8, Math.min(256, font.value)) : 36;
        const opacity = typeof node.style.opacity === 'number' ? Math.max(0, Math.min(1, node.style.opacity)) : 1;
        return <g key={node.id} data-node-id={node.id} transform={`translate(${x} ${y}) rotate(${node.layout.rotation})`} style={{ cursor: disabled ? 'default' : 'move' }}
          onPointerDown={event => { if (disabled || event.button !== 0) return; event.stopPropagation(); svg.current?.focus(); onSelect(node.id); const start = point(event.clientX, event.clientY); if (!start) return; drag.current = { id: node.id, pointerId: event.pointerId, x, y, startX: start.x, startY: start.y }; svg.current?.setPointerCapture(event.pointerId); }}>
          <title>{node.name}</title><rect width={width} height={height} fill={node.type === 'panel' ? previewColor(node.style.fill, '#263f60') : 'transparent'} opacity={opacity}/>
          {node.type === 'text' && <svg width={width} height={height} overflow="hidden" pointerEvents="none"><text x={0} y={0} dominantBaseline="hanging" fontFamily="sans-serif" fontSize={fontSize} fill={previewColor(node.style.color, '#ffffff')} opacity={opacity}>{previewText(node).split('\n').slice(0, 20).map((line, index) => <tspan key={index} x={0} dy={index ? '1.2em' : 0}>{line || ' '}</tspan>)}</text></svg>}
        </g>;
      })}
      {(() => { const node = visible.find(node => node.id === selected); if (!node) return null; const x = preview?.id === node.id ? preview.x : pixels(node.layout.x, scene.resolution.width); const y = preview?.id === node.id ? preview.y : pixels(node.layout.y, scene.resolution.height); return <rect transform={`translate(${x} ${y}) rotate(${node.layout.rotation})`} width={pixels(node.layout.width, scene.resolution.width)} height={pixels(node.layout.height, scene.resolution.height)} fill="none" stroke="#65b6ff" strokeWidth={2} vectorEffect="non-scaling-stroke" pointerEvents="none"/>; })()}
    </svg>
    <p id="ts-canvas-help">Select a layer and drag to move it, or focus the canvas and use arrow keys (Shift: 10 pixels). Escape cancels a drag. Editor preview; native frontend behavior is unverified.</p>
    {visible.length < scene.nodes.length && <p>Only top-level text and panels are drawn here. Other layers and their source data are preserved.</p>}
  </section>;
}
