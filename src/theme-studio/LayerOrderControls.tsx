import type { Scene } from './types';
import { canOrderLayers, type LayerOrder } from './scene-edit';

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
      const boundary = order === 'back' || order === 'backward' ? index <= 0 : index >= scene.nodes.length - 1;
      return <button key={order} disabled={boundary} onClick={() => { if (!disabled && supported && index >= 0 && !boundary) onOrder(order); }}>{label}</button>;
    })}
    {!supported && <p>Ordering is read-only in scenes with nested or unsupported layer types.</p>}
  </fieldset>;
}
