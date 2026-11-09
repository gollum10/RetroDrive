import type { Scene } from './types';
import { canOrderLayers, layerState, type LayerOrder } from './scene-edit';

const actions: { order: LayerOrder; label: string }[] = [
  { order: 'back', label: 'Send to back' },
  { order: 'backward', label: 'Send backward' },
  { order: 'forward', label: 'Bring forward' },
  { order: 'front', label: 'Bring to front' },
];

export default function LayerOrderControls({ scene, selected, disabled, onOrder }: {
  scene: Scene; selected: string | null; disabled: boolean; onOrder: (order: LayerOrder) => void;
}) {
  const supported = canOrderLayers(scene);
  const index = scene.nodes.findIndex(node => node.id === selected);
  return <fieldset disabled={disabled || !supported || index < 0} className="ts-layer-order">
    <legend>Layer order</legend>
    {actions.map(({ order, label }) => {
      const destination = order === 'back' ? 0 : order === 'front' ? scene.nodes.length-1 : order === 'backward' ? Math.max(0,index-1) : Math.min(scene.nodes.length-1,index+1);
      const boundary = destination === index || scene.nodes.slice(Math.max(0,Math.min(index,destination)),Math.max(index,destination)+1).some(node => layerState(node).locked || layerState(node).hidden);
      return <button key={order} disabled={boundary} onClick={() => { if (!disabled && supported && index >= 0 && !boundary) onOrder(order); }}>{label}</button>;
    })}
    {!supported && <p>Ordering is read-only in scenes with nested or unsupported layer types.</p>}
  </fieldset>;
}
