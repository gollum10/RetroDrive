import type { Scene, ThemeNode } from './types';
import { alignedNode, type Alignment } from './alignment';
import { editableNode } from './scene-edit';

const actions: { alignment: Alignment; label: string }[] = [
  { alignment: 'left', label: 'Align left' }, { alignment: 'center-x', label: 'Center horizontally' },
  { alignment: 'right', label: 'Align right' }, { alignment: 'top', label: 'Align top' },
  { alignment: 'center-y', label: 'Center vertically' }, { alignment: 'bottom', label: 'Align bottom' },
];
export default function AlignmentControls({ scene, node, disabled, onAlign }: {
  scene: Scene; node: ThemeNode | null; disabled: boolean; onAlign: (alignment: Alignment) => void;
}) {
  const supported = node !== null && editableNode(scene, node);
  return <fieldset disabled={disabled || !supported} className="ts-layer-order"><legend>Align to canvas</legend>
    {actions.map(({ alignment, label }) => {
      const unchanged = !node || alignedNode(scene, node, alignment) === node;
      return <button key={alignment} disabled={unchanged} onClick={() => { if (!disabled && supported && !unchanged) onAlign(alignment); }}>{label}</button>;
    })}
    <small>Uses the rotated layer rectangle, including transparent space.</small>
  </fieldset>;
}
