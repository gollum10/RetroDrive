import { useRef, useState } from 'react';
import { errorText } from './document';
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
export default function ImageImportForm({ sceneName, onImport, onCancel }: {
    sceneName: string;
    onImport: (encoded: string, license: string, attribution: string) => Promise<void>;
    onCancel: () => void;
}) {
    const [file, setFile] = useState<File | null>(null);
    const [license, setLicense] = useState('unknown');
    const [attribution, setAttribution] = useState('');
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState('');
    const active = useRef(false);
    return <form className="panel" aria-label="Import PNG" onSubmit={event => { event.preventDefault(); if (!file || active.current)
        return; active.current = true; setBusy(true); setError(''); void encodePngFile(file).then(encoded => onImport(encoded, license, attribution)).catch(error => setError(errorText(error))).finally(() => { active.current = false; setBusy(false); }); }}>
    <h3>Add a picture to {sceneName}</h3><p>Use any PNG: your own art, a photo, or an image you made with an AI tool. RetroDrive keeps its own copy, so moving or deleting the original later is fine. Up to 16 MB and 4096 × 4096 pixels.</p>
    <fieldset disabled={busy}><label>Picture (PNG)<input type="file" accept=".png,image/png" onChange={event => { setFile(event.target.files?.[0] ?? null); setError(''); }}/></label>
    {file && <p>{file.name} · {(file.size / 1024).toLocaleString(undefined, { maximumFractionDigits: 0 })} KB</p>}
    <label>License (if you share the theme)<small className="ts-field-help">For an AI-made picture, check the AI tool's terms; for someone else's art, their license.</small><input maxLength={256} value={license} onChange={event => setLicense(event.target.value)}/></label><label>Credit (who made it)<input maxLength={512} value={attribution} onChange={event => setAttribution(event.target.value)}/></label>
    <button type="submit" disabled={!file}>{busy ? 'Adding…' : 'Add picture'}</button><button type="button" onClick={onCancel}>Cancel</button></fieldset>
    {error && <p role="alert">{error}</p>}
  </form>;
}
