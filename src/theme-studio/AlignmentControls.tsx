import type { Scene, ThemeNode } from './types';
import { alignedNode, type Alignment } from './alignment';
import { editableNode } from './scene-edit';

const actions: { alignment: Alignment; label: string; short: string }[] = [
  { alignment: 'left', label: 'Align left', short: 'Left' }, { alignment: 'center-x', label: 'Center horizontally', short: 'Centre' },
  { alignment: 'right', label: 'Align right', short: 'Right' }, { alignment: 'top', label: 'Align top', short: 'Top' },
  { alignment: 'center-y', label: 'Center vertically', short: 'Middle' }, { alignment: 'bottom', label: 'Align bottom', short: 'Bottom' },
];
export default function AlignmentControls({ scene, node, disabled, onAlign }: {
  scene: Scene; node: ThemeNode | null; disabled: boolean; onAlign: (alignment: Alignment) => void;
}) {
  if (!node) return null; // shown once a layer is selected (ADR-109)
  const supported = editableNode(scene, node);
  return <fieldset disabled={disabled || !supported} className="ts-layer-order ts-align"><legend>Align on the screen</legend>
    {actions.map(({ alignment, label, short }) => {
      const unchanged = !node || alignedNode(scene, node, alignment) === node;
      return <button key={alignment} aria-label={label} title={label} disabled={unchanged} onClick={() => { if (!disabled && supported && !unchanged) onAlign(alignment); }}>{short}</button>;
    })}
    <small className="ts-field-help">Lines up the layer's box (including any see-through parts) with the screen.</small>
  </fieldset>;
}
