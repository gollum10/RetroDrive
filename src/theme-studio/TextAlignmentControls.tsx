import type { ThemeNode } from './types';
import { layerState } from './scene-edit';
import { setTextAlignment, textAlignment } from './text-presentation';
export default function TextAlignmentControls({ node, disabled, onChange, onEnd }: {
  node: ThemeNode; disabled: boolean; onChange: (node: ThemeNode, label: string) => void; onEnd: () => void;
}) {
  const alignment = textAlignment(node);
  const state = layerState(node);
  const blocked = disabled || node.type !== 'text' || !!node.children.length || state.locked || state.hidden || alignment === null;
  return <label>Text alignment<select disabled={blocked} value={alignment ?? 'unsupported'} onChange={event => {
    if (blocked) return;
    const next = setTextAlignment(node, event.target.value);
    if (next !== node) onChange(next, 'Change text alignment');
    onEnd();
  }}><option value="left">Left</option><option value="center">Center</option><option value="right">Right</option>{alignment === null && <option value="unsupported">Unsupported alignment (preserved)</option>}</select></label>;
}
