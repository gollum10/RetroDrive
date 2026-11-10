import type { ThemeNode } from './types';
import { layerState } from './scene-edit';
import { textSources, textBindingEditable, setTextBinding } from './sample-data';
export default function TextBindingControls({ node, disabled, onChange, onEnd }: {
    node: ThemeNode;
    disabled: boolean;
    onChange: (node: ThemeNode, label: string) => void;
    onEnd: () => void;
}) {
    const state = layerState(node);
    const editable = textBindingEditable(node);
    const blocked = disabled || state.locked || state.hidden || !editable;
    return <div><label>Text content source<select disabled={blocked} value={editable ? node.bindings.text?.source ?? '' : 'unsupported'} onChange={event => { if (blocked)
        return; const next = setTextBinding(node, event.target.value); if (next !== node)
        onChange(next, 'Change text binding'); onEnd(); }}>
  <option value="">Static text</option>{textSources.map(option => <option key={option.source} value={option.source}>{option.label}</option>)}{!editable && <option value="unsupported">Unsupported binding ? preserved</option>}
 </select></label><p>Bindings use the selected canvas sample for preview. Static text is retained when a binding is selected; switch back to restore it.</p>{!editable && <p>This binding is preserved read-only; no expression or image source is evaluated as text.</p>}</div>;
}
