import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorText } from './document';
import type { Placement } from './image-assets';
export async function encodePngFile(file: File) {
    if (file.size === 0 || file.size > 16 * 1024 * 1024)
        throw new Error('Choose a PNG picture up to 16 MB.');
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (bytes.length !== file.size || bytes.length > 16 * 1024 * 1024)
        throw new Error('Image size changed while reading.');
    const signature = [137, 80, 78, 71, 13, 10, 26, 10];
    if (!signature.every((value, index) => bytes[index] === value))
        throw new Error("That file isn't a PNG picture. Save it as PNG first (most image and AI tools can).");
    let binary = '';
    for (let i = 0; i < bytes.length; i += 8192)
        binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
    return btoa(binary);
}
const LIMIT = 16 * 1024 * 1024, AXIS = 4096, PIXELS = 4194304;
const toBase64 = (bytes: Uint8Array) => { let binary = ''; for (let i = 0; i < bytes.length; i += 8192) binary += String.fromCharCode(...bytes.subarray(i, i + 8192)); return btoa(binary); };
/** PNG size from its header, or null when it isn't a PNG. */
function pngSize(bytes: Uint8Array): [number, number] | null {
    const signature = [137, 80, 78, 71, 13, 10, 26, 10];
    if (bytes.length < 24 || !signature.every((value, index) => bytes[index] === value)) return null;
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    return [view.getUint32(16), view.getUint32(20)];
}
/** Any picture the system can read (PNG, JPG, WebP, …) as PNG for the image store (ADR-110).
 *  PNGs within the limits pass through unchanged; anything else is redrawn as PNG on this PC,
 *  made smaller when needed to fit 4096 pixels a side and 4 million pixels. Nothing is uploaded. */
export async function encodePictureFile(file: File): Promise<{ encoded: string; note: string }> {
    if (file.size === 0 || file.size > 64 * 1024 * 1024) throw new Error('Choose a picture up to 64 MB.');
    const bytes = new Uint8Array(await file.arrayBuffer());
    const size = pngSize(bytes);
    if (size && bytes.length <= LIMIT && size[0] <= AXIS && size[1] <= AXIS && size[0] * size[1] <= PIXELS) return { encoded: await encodePngFile(file), note: '' };
    let bitmap: ImageBitmap;
    try { bitmap = await createImageBitmap(file); } catch { throw new Error("RetroDrive can't read this picture. Save it as PNG, JPG or WebP and try again."); }
    try {
        let scale = Math.min(1, AXIS / bitmap.width, AXIS / bitmap.height, Math.sqrt(PIXELS / (bitmap.width * bitmap.height)));
        for (let attempt = 0; attempt < 5; attempt++, scale *= .75) {
            const canvas = document.createElement('canvas');
            canvas.width = Math.max(1, Math.floor(bitmap.width * scale)); canvas.height = Math.max(1, Math.floor(bitmap.height * scale));
            const context = canvas.getContext('2d'); if (!context) throw new Error("RetroDrive couldn't convert this picture.");
            context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
            const blob = await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, 'image/png'));
            if (!blob) throw new Error("RetroDrive couldn't convert this picture.");
            if (blob.size <= LIMIT) return { encoded: toBase64(new Uint8Array(await blob.arrayBuffer())), note: `${size ? '' : 'Converted to PNG. '}${scale < 1 ? `Made smaller to fit: ${canvas.width} × ${canvas.height} pixels.` : ''}`.trim() };
        }
        throw new Error('This picture is too detailed to store even when made smaller. Try a smaller version.');
    } finally { bitmap.close(); }
}
export type ImportOptions = { placement: Placement; aiMade: boolean };
type PictureStatus = { openaiConnected: boolean; openaiModel: string | null; programUrl: string | null; programKind: string | null; programModel: string | null };
/** Picture services ready to use (ADR-122): OpenAI with a chosen model, and/or a picture program. */
export function pictureServices(s: PictureStatus | null): [string, string][] {
    if (!s) return [];
    return [
        ...(s.openaiConnected && s.openaiModel ? [['openai', `OpenAI (${s.openaiModel})`] as [string, string]] : []),
        ...(s.programUrl ? [['program', `${s.programKind === 'comfyui' ? 'ComfyUI' : 'Automatic1111'} on your PC`] as [string, string]] : []),
    ];
}
/** A picture that came back from an AI service, as a file for the usual picture checks. */
export function aiPictureFile(base64: string): File {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    const type = bytes[0] === 0x89 ? 'image/png' : bytes[0] === 0xff ? 'image/jpeg' : 'image/webp';
    return new File([bytes], `ai-picture.${type.split('/')[1]}`, { type });
}
export default function ImageImportForm({ sceneName, overName, initialPlacement = 'free', onImport, onCancel }: {
    /** Which placement is chosen when the form opens (for example a frame over a game video). */
    initialPlacement?: Placement;
    sceneName: string;
    /** The selected layer's name, when a picture can be placed over it (a frame). */
    overName?: string;
    onImport: (encoded: string, license: string, attribution: string, options: ImportOptions) => Promise<void>;
    onCancel: () => void;
}) {
    const [file, setFile] = useState<File | null>(null);
    const [license, setLicense] = useState('unknown');
    const [attribution, setAttribution] = useState('');
    const [placement, setPlacement] = useState<Placement>(initialPlacement === 'over' && !overName ? 'free' : initialPlacement);
    const [aiMade, setAiMade] = useState(false);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState('');
    const [note, setNote] = useState('');
    // Make one with AI (ADR-122).
    const [source, setSource] = useState<'file' | 'ai'>('file');
    const [services, setServices] = useState<[string, string][]>([]);
    const [service, setService] = useState('');
    const [describe, setDescribe] = useState('');
    const [shape, setShape] = useState(initialPlacement === 'background' ? 'wide' : 'square');
    const [making, setMaking] = useState(false);
    const [preview, setPreview] = useState('');
    useEffect(() => { invoke<PictureStatus>('ai_picture_status').then(s => { const list = pictureServices(s); setServices(list); setService(list[0]?.[0] ?? ''); }).catch(() => setServices([])); }, []);
    useEffect(() => () => { if (preview) URL.revokeObjectURL(preview); }, [preview]);
    const make = async () => {
        setMaking(true); setError(''); setNote('');
        try {
            const picture = aiPictureFile(await invoke<string>('ai_make_picture', { provider: service, prompt: describe, shape }));
            setFile(picture); setPreview(URL.createObjectURL(picture)); setAiMade(true);
            const label = services.find(([id]) => id === service)?.[1] ?? 'an AI tool';
            if (!attribution.trim() || /^Made with /.test(attribution)) setAttribution(`Made with ${label.replace(/ on your PC$/, '')}`);
            if (license === 'unknown') setLicense('AI-generated (see the service\'s terms)');
        } catch (e) { setError(errorText(e)); } finally { setMaking(false); }
    };
    const active = useRef(false);
    const formRef = useRef<HTMLFormElement>(null);
    // Bring the form into view when it opens: it appears above the editor (ADR-111).
    useEffect(() => { formRef.current?.scrollIntoView?.({ block: 'start', behavior: 'smooth' }); }, []);
    const submit = (where: Placement) => { if (!file || active.current)
        return; active.current = true; setPlacement(where); setBusy(true); setError(''); setNote(''); void encodePictureFile(file).then(picture => { setNote(picture.note); return onImport(picture.encoded, license, attribution, { placement: where, aiMade }); }).catch(error => setError(errorText(error))).finally(() => { active.current = false; setBusy(false); }); };
    return <form ref={formRef} className="panel" aria-label="Import PNG" onSubmit={event => { event.preventDefault(); submit(placement); }}>
    <h3>Add a picture to {sceneName}</h3><p>Use any picture: your own art, a photo, or an image you made with an AI tool. PNG, JPG and WebP all work; RetroDrive keeps its own copy as PNG, so moving or deleting the original later is fine. Very large pictures are made smaller to fit (4096 × 4096 pixels at most).</p>
    <fieldset disabled={busy || making}>
    <div className="ts-levels" role="radiogroup" aria-label="Where the picture comes from"><button type="button" role="radio" aria-checked={source === 'file'} className={source === 'file' ? 'on' : ''} onClick={() => { setSource('file'); setFile(null); setPreview(''); }}>From a file</button><button type="button" role="radio" aria-checked={source === 'ai'} className={source === 'ai' ? 'on' : ''} onClick={() => { setSource('ai'); setFile(null); }}>Make one with AI</button></div>
    {source === 'file' ? <><label>Picture<input type="file" accept=".png,.jpg,.jpeg,.webp,image/png,image/jpeg,image/webp" onChange={event => { setFile(event.target.files?.[0] ?? null); setError(''); setNote(''); }}/></label>
    {file && <p>{file.name} · {(file.size / 1024).toLocaleString(undefined, { maximumFractionDigits: 0 })} KB</p>}</>
    : services.length === 0 ? <p>Set up OpenAI or a picture program on your PC in <b>Settings → AI helpers → AI pictures</b> first.</p>
    : <div className="ts-ai-picture">
      <label>Describe the picture<textarea maxLength={1000} rows={3} value={describe} placeholder="For example: a neon synthwave sunset over a purple grid, no text" onChange={e => setDescribe(e.target.value)}/></label>
      <div className="actions">
        <label>Shape <select value={shape} onChange={e => setShape(e.target.value)}><option value="wide">Wide (backgrounds)</option><option value="square">Square</option><option value="tall">Tall</option></select></label>
        {services.length > 1 && <label>Make it with <select value={service} onChange={e => setService(e.target.value)}>{services.map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select></label>}
        <button type="button" className="primary" disabled={!describe.trim() || making} onClick={() => { void make(); }}>{making ? 'Making… (can take a minute)' : file ? 'Make another' : 'Make picture'}</button>
      </div>
      <p className="ts-field-help">Sent: only your description and the size{services.length === 1 ? `, to ${services[0][1]}` : ''}.</p>
      {preview && <><img className="ts-ai-preview" src={preview} alt="The picture the AI made"/>
        <div className="actions"><button type="button" className="primary" onClick={() => submit('background')}>Use as the background</button><button type="button" onClick={() => submit('free')}>Put it on the screen</button>{overName && <button type="button" onClick={() => submit('over')}>Use as a frame over “{overName}”</button>}</div>
        <p className="ts-field-help">Credited as “{attribution}”. You can change the credit and license below.</p></>}
    </div>}
    <fieldset className="ts-placement"><legend>Put it</legend>
      <label><input type="radio" name="ts-placement" checked={placement === 'free'} onChange={() => setPlacement('free')}/> On the screen (then drag it where you want)</label>
      <label><input type="radio" name="ts-placement" checked={placement === 'background'} onChange={() => { setPlacement('background'); if (!file) setShape('wide'); }}/> As the screen background (fills the screen, behind everything)</label>
      {overName && <label><input type="radio" name="ts-placement" checked={placement === 'over'} onChange={() => setPlacement('over')}/> Over “{overName}”, as a frame (a picture with see-through parts, such as an arcade cabinet around the game)</label>}
    </fieldset>
    <label className="ts-check"><input type="checkbox" checked={aiMade} onChange={event => setAiMade(event.target.checked)}/> Made with an AI tool (added to the picture's credit)</label>
        <label>License (if you share the theme)<small className="ts-field-help">For an AI-made picture, check the AI tool's terms; for someone else's art, their license.</small><input maxLength={256} value={license} onChange={event => setLicense(event.target.value)}/></label><label>Credit (who made it)<input maxLength={512} value={attribution} onChange={event => setAttribution(event.target.value)}/></label>
    <button type="submit" disabled={!file}>{busy ? 'Adding…' : 'Add picture'}</button><button type="button" onClick={onCancel}>Cancel</button></fieldset>
    {note && <p role="status">{note}</p>}
    {error && <p role="alert">{error}</p>}
  </form>;
}
