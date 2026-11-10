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
    <fieldset disabled={blocked}><legend>Fit and crop</legend><label>Image sizing<select disabled={!state.fitEditable} value={state.fit} onChange={event => { commit(setImageFit(node, event.target.value as 'contain' | 'cover'), 'Change image sizing'); onEnd(); }}><option value="contain">Fit: show the whole picture</option><option value="cover">Fill: cover the whole area</option></select></label>
      {!state.fitEditable && <p>This picture's sizing can't be changed here; it's kept as it is.</p>}
      {state.cropEditable ? <><p className="ts-field-help">Crop in percent of the picture. Your picture itself is never changed.</p>
        {(['x', 'y', 'width', 'height'] as (keyof Crop)[]).map(key => <NumberField key={key} label={`Crop ${key === 'x' ? 'left' : key === 'y' ? 'top' : key} (%)`} value={state.crop[key]} min={key === 'x' || key === 'y' ? 0 : .1} max={key === 'x' || key === 'y' ? 99.9 : key === 'width' ? Math.round((100 - state.crop.x) * 1e6) / 1e6 : Math.round((100 - state.crop.y) * 1e6) / 1e6} onChange={value => commit(setImageCrop(node, key, value), 'Crop image', `crop:${node.id}:${key}`)} onEnd={onEnd}/>)}
        <button type="button" onClick={() => { commit(resetImageCrop(node), 'Reset image crop'); onEnd(); }}>Reset crop</button>
      </> : <p>This picture's crop can't be changed here; it's kept as it is.</p>}
    </fieldset>
    <h3>Picture details</h3>{asset ? <dl><dt>Dimensions</dt><dd>{dimensions && !dimensions.failed ? `${dimensions.width} × ${dimensions.height}` : 'Preview dimensions unavailable'}</dd><dt>Format / size</dt><dd>{asset.mediaType} · {asset.bytes.toLocaleString()} bytes</dd><dt>License</dt><dd>{asset.license || 'unknown'}</dd><dt>Credit</dt><dd>{asset.attribution || 'Not recorded'}</dd><dt>Fingerprint</dt><dd className="ts-asset-hash">{asset.sha256}</dd></dl> : <p>Picture details unavailable.</p>}
  </section>;
}
