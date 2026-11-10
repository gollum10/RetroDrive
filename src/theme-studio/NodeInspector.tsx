import TextAlignmentControls from './TextAlignmentControls';
import TextBindingControls from './TextBindingControls';
import NumberField from './NumberField';
import type { Dimension, Scene, ThemeNode } from './types';
import { editableNode, pixels, positionFromPixels, previewColor, previewText, transformNode, layerState } from './scene-edit';



export default function NodeInspector({ scene, node, disabled, onChange, onEnd }: { scene: Scene; node: ThemeNode | null; disabled: boolean; onChange: (node: ThemeNode, label: string, group?: string) => void; onEnd: () => void }) {
  if (!node) return <section aria-label="Layer properties"><h3>Layer properties</h3><p>Click a layer to change it, or add a picture, text or a panel.</p></section>;
  if (layerState(node).locked || layerState(node).hidden) return <section aria-label="Layer properties"><h3>{node.name}</h3><p>This layer is locked or hidden. Unlock or show it in the layer list to change it.</p></section>;
  if (!editableNode(scene, node)) return <section aria-label="Layer properties"><h3>{node.name}</h3><p>This kind of layer can't be changed here yet. It's kept as it is.</p></section>;
  const geometry = (key: 'x' | 'y' | 'width' | 'height', dimension: Dimension) => onChange({ ...node, layout: { ...node.layout, [key]: dimension } }, `Change ${key}`, `layout:${node.id}:${key}`);
  const style = (key: string, value: ThemeNode['style'][string]) => onChange({ ...node, style: { ...node.style, [key]: value } }, `Change ${key}`, `style:${node.id}:${key}`);
  const font = node.style.fontSize;
  const fontSize = font && typeof font === 'object' && !Array.isArray(font) && typeof font.value === 'number' ? font.value : 36;
  const fontEditable = font === undefined || !!(font && typeof font === 'object' && !Array.isArray(font) && font.unit === 'px' && typeof font.value === 'number');
  const textValue = typeof node.style.text === 'string' ? node.style.text : node.name;
  const textEditable = !node.bindings.text && textValue.length <= 500 && (node.style.text === undefined || typeof node.style.text === 'string');
  const colorKey = node.type === 'text' ? 'color' : 'fill';
  const colorValue = node.style[colorKey];
  const opacity = node.style.opacity;
  const opacityEditable = opacity === undefined || (typeof opacity === 'number' && Number.isFinite(opacity) && opacity >= 0 && opacity <= 1);
  const transform = (property: 'rotation' | 'opacity', value: number) => {
    if (disabled) return;
    const next = transformNode(node, property, value);
    if (next !== node) onChange(next, `Change ${property}`, `transform:${node.id}:${property}`);
  };
  const colorEditable = colorValue === undefined || (typeof colorValue === 'string' && /^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(colorValue));
  return <section aria-label="Layer properties"><h3>Layer properties</h3><fieldset disabled={disabled} className="ts-property-fields" onBlur={onEnd}><legend className="ts-sr-only">Edit selected layer</legend>
    <label>Layer name<input maxLength={120} value={node.name} onChange={event => onChange({ ...node, name: event.target.value, style: node.type === 'text' && node.style.text === undefined && !node.bindings.text ? { ...node.style, text: node.name } : node.style }, 'Rename layer', `name:${node.id}`)} /></label>
    {(['x', 'y', 'width', 'height'] as const).map(key => <div className="ts-dimension" key={key}><NumberField label={key === 'x' ? 'X position' : key === 'y' ? 'Y position' : key === 'width' ? 'Width' : 'Height'} value={node.layout[key].value} min={key === 'width' || key === 'height' ? .000001 : -1_000_000} max={1_000_000} onChange={value => geometry(key, { ...node.layout[key], value })} onEnd={onEnd}/><label>{key} units<select value={node.layout[key].unit} onChange={event => { const unit = event.target.value as 'px' | '%'; const extent = key === 'x' || key === 'width' ? scene.resolution.width : scene.resolution.height; const dimension = positionFromPixels({ ...node.layout[key], unit }, pixels(node.layout[key], extent), extent); geometry(key, key === 'width' || key === 'height' ? { ...dimension, value: Math.max(.000001, dimension.value) } : dimension); onEnd(); }}><option value="px">px</option><option value="%">%</option></select></label></div>)}
    <NumberField label="Rotation (degrees)" value={node.layout.rotation} min={-36_000} max={36_000} onChange={value => transform('rotation', value)} onEnd={onEnd}/>
    <p>Rotation turns around the top-left corner. Opacity 0 makes the layer invisible (select it in the layer list).</p>
    {opacityEditable ? <NumberField label="Opacity (0 to 1)" value={typeof opacity === 'number' ? opacity : 1} min={0} max={1} onChange={value => transform('opacity', value)} onEnd={onEnd}/> : <p>This layer's opacity can't be changed here; it's kept.</p>}
    {node.type === 'text' && <><TextAlignmentControls node={node} disabled={disabled} onChange={onChange} onEnd={onEnd}/><TextBindingControls node={node} disabled={disabled} onChange={onChange} onEnd={onEnd}/><label>Text<textarea maxLength={500} disabled={!textEditable} value={node.bindings.text ? previewText(node) : textValue} onChange={event => style('text', event.target.value)} /></label>{node.bindings.text ? <p>This text is bound to {node.bindings.text.source}; the canvas uses the selected sample dataset or binding-placeholder mode.</p> : !textEditable && <p>This text can't be changed here yet; it's kept.</p>}{fontEditable ? <NumberField label="Font size (px)" value={fontSize} min={8} max={256} onChange={value => style('fontSize', { ...(font && typeof font === 'object' && !Array.isArray(font) ? font : {}), value, unit: 'px' })} onEnd={onEnd}/> : <p>This font setting can't be changed here; it's kept.</p>}</>}
    {node.type !== 'image' && <><label>{node.type === 'text' ? 'Text color' : 'Panel color'}<input type="color" disabled={!colorEditable} value={previewColor(colorValue, node.type === 'text' ? '#ffffff' : '#263f60').slice(0, 7)} onChange={event => { style(colorKey, event.target.value.slice(1) + (typeof colorValue === 'string' && /^[0-9a-f]{8}$/i.test(colorValue) ? colorValue.slice(6) : '')); }} /></label>{!colorEditable && <p>This colour can't be changed here; it's kept.</p>}</>}
  </fieldset></section>;
}
