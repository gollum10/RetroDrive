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
  <option value="">Static text</option>{textSources.map(option => <option key={option.source} value={option.source}>{option.label}</option>)}{!editable && <option value="unsupported">Linked to something this editor can’t show (kept)</option>}
 </select></label><p className="ts-field-help">Shows each game's own title or description on the system; the preview uses the sample game. Your typed text is kept if you switch back.</p>{!editable && <p>This text is linked to something this editor can't change yet. It's kept as it is.</p>}</div>;
}
