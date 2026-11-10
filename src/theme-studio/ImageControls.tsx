import type { Asset, Scene, ThemeNode } from './types';
import NumberField from './NumberField';
import { editableNode } from './scene-edit';
import { imagePresentation, setImageFit, setImageCrop, resetImageCrop, type Crop } from './image-presentation';
import usePngDimensions from './use-png-dimensions';
export default function ImageControls({ scene, node, asset, url, disabled, onChange, onEnd }: {
    scene: Scene;
    node: ThemeNode;
    asset: Asset | undefined;
    url: string | undefined;
    disabled: boolean;
    onChange: (node: ThemeNode, label: string, group?: string) => void;
    onEnd: () => void;
}) {
    const dimensions = usePngDimensions(url);
    const state = imagePresentation(node);
    const blocked = disabled || !editableNode(scene, node);
    const commit = (next: ThemeNode, label: string, group?: string) => { if (!blocked && next !== node)
        onChange(next, label, group); };
    return <section aria-label="Image controls"><h3>Image display</h3>
    <fieldset disabled={blocked}><legend>Fit and crop</legend><label>Image sizing<select disabled={!state.fitEditable} value={state.fit} onChange={event => { commit(setImageFit(node, event.target.value as 'contain' | 'cover'), 'Change image sizing'); onEnd(); }}><option value="contain">Fit ? show entire crop</option><option value="cover">Fill ? cover layer</option></select></label>
      {!state.fitEditable && <p>Unsupported sizing data is preserved read-only.</p>}
      {state.cropEditable ? <><p>Crop uses percentages of the original image. Moving left/top trims width/height to stay inside it. Stored pixels stay unchanged.</p>
        {(['x', 'y', 'width', 'height'] as (keyof Crop)[]).map(key => <NumberField key={key} label={`Crop ${key === 'x' ? 'left' : key === 'y' ? 'top' : key} (%)`} value={state.crop[key]} min={key === 'x' || key === 'y' ? 0 : .1} max={key === 'x' || key === 'y' ? 99.9 : key === 'width' ? Math.round((100 - state.crop.x) * 1e6) / 1e6 : Math.round((100 - state.crop.y) * 1e6) / 1e6} onChange={value => commit(setImageCrop(node, key, value), 'Crop image', `crop:${node.id}:${key}`)} onEnd={onEnd}/>)}
        <button type="button" onClick={() => { commit(resetImageCrop(node), 'Reset image crop'); onEnd(); }}>Reset crop</button>
      </> : <p>Unsupported crop data is preserved read-only.</p>}
    </fieldset>
    <h3>Asset information</h3>{asset ? <dl><dt>Dimensions</dt><dd>{dimensions && !dimensions.failed ? `${dimensions.width} ? ${dimensions.height}` : 'Preview dimensions unavailable'}</dd><dt>Format / size</dt><dd>{asset.mediaType} ? {asset.bytes.toLocaleString()} bytes</dd><dt>License</dt><dd>{asset.license || 'unknown'}</dd><dt>Attribution</dt><dd>{asset.attribution || 'Not recorded'}</dd><dt>Content hash</dt><dd className="ts-asset-hash">{asset.sha256}</dd></dl> : <p>Asset declaration unavailable.</p>}
  </section>;
}
