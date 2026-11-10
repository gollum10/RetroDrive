import { Eye, EyeOff, Lock, Unlock } from 'lucide-react';
import type { Scene } from './types';
import { layerState, supportedNode } from './scene-edit';

export default function LayerList({ scene, selected, disabled, onSelect, onState }: {
  scene: Scene; selected: string | null; disabled: boolean; onSelect: (id: string) => void;
  onState: (id: string, key: 'locked' | 'hidden', value: boolean) => void;
}) {
  return <section aria-label="Layers"><h3>Layers</h3><small className="ts-field-help">Top of the list is drawn first; later layers cover it.</small>
    {scene.nodes.length ? <div className="ts-layer-list">{scene.nodes.map(node => {
      const state = layerState(node); const supported = supportedNode(scene,node) && state.recognized;
      return <div className="ts-layer-row" key={node.id}>
        <button aria-pressed={selected === node.id} onClick={() => onSelect(node.id)}>{node.name || node.type} <small>{node.type === 'image' ? 'picture' : node.type === 'video' ? 'game video' : node.type === 'game_list' ? 'game list' : node.type}{state.locked ? ' · locked' : ''}{state.hidden ? ' · hidden' : ''}</small></button>
        <div className="ts-layer-toggles">
          <button disabled={disabled || !supported} aria-label={`${state.locked ? 'Unlock' : 'Lock'} ${node.name || node.type}`} aria-pressed={state.locked} title={state.locked ? 'Unlock' : 'Lock'} onClick={() => { if (!disabled && supported) onState(node.id,'locked',!state.locked); }}>{state.locked ? <Lock size={14}/> : <Unlock size={14}/>}</button>
          <button disabled={disabled || !supported} aria-label={`${state.hidden ? 'Show' : 'Hide'} ${node.name || node.type}`} aria-pressed={state.hidden} title={state.hidden ? 'Show' : 'Hide'} onClick={() => { if (!disabled && supported) onState(node.id,'hidden',!state.hidden); }}>{state.hidden ? <EyeOff size={14}/> : <Eye size={14}/>}</button>
        </div>
        {!state.recognized && <small>Some details can't be changed here; they're kept.</small>}
      </div>;
    })}</div> : <p>Nothing on this screen yet.</p>}
  </section>;
}
