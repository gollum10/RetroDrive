import type { Scene } from './types';
import { layerState, supportedNode } from './scene-edit';

export default function LayerList({ scene, selected, disabled, onSelect, onState }: {
  scene: Scene; selected: string | null; disabled: boolean; onSelect: (id: string) => void;
  onState: (id: string, key: 'locked' | 'hidden', value: boolean) => void;
}) {
  return <section aria-label="Layers"><h3>Layers</h3><small>Back to front; hidden layers stay listed.</small>
    {scene.nodes.length ? <div className="ts-layer-list">{scene.nodes.map(node => {
      const state = layerState(node); const supported = supportedNode(scene,node) && state.recognized;
      return <div className="ts-layer-row" key={node.id}>
        <button aria-pressed={selected === node.id} onClick={() => onSelect(node.id)}>{node.name || node.type} <small>({node.type}){state.locked ? ' ? locked' : ''}{state.hidden ? ' ? hidden' : ''}</small></button>
        <div className="ts-layer-toggles">
          <button disabled={disabled || !supported} aria-label={`${state.locked ? 'Unlock' : 'Lock'} ${node.name || node.type}`} aria-pressed={state.locked} onClick={() => { if (!disabled && supported) onState(node.id,'locked',!state.locked); }}>{state.locked ? 'Unlock' : 'Lock'}</button>
          <button disabled={disabled || !supported} aria-label={`${state.hidden ? 'Show' : 'Hide'} ${node.name || node.type}`} aria-pressed={state.hidden} onClick={() => { if (!disabled && supported) onState(node.id,'hidden',!state.hidden); }}>{state.hidden ? 'Show' : 'Hide'}</button>
        </div>
        {!state.recognized && <small>Unrecognized editor data is preserved read-only.</small>}
      </div>;
    })}</div> : <p>No layers in this scene.</p>}
  </section>;
}
