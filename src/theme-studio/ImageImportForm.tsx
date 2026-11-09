import { useRef, useState } from 'react';
import { errorText } from './document';
export async function encodePngFile(file: File) {
    if (file.size === 0 || file.size > 16 * 1024 * 1024)
        throw new Error('Choose a nonempty PNG up to 16 MiB.');
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (bytes.length !== file.size || bytes.length > 16 * 1024 * 1024)
        throw new Error('Image size changed while reading.');
    const signature = [137, 80, 78, 71, 13, 10, 26, 10];
    if (!signature.every((value, index) => bytes[index] === value))
        throw new Error('Only PNG images are supported.');
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
    <h3>Import PNG into {sceneName}</h3><p>Choose your own static PNG. Import stores a normalized copy in the local catalog. Text/profile metadata is removed; 16-bit samples reduce to 8-bit. Limit: 16 MiB, 4096 per axis and 4 million pixels.</p>
    <fieldset disabled={busy}><label>PNG file<input type="file" accept=".png,image/png" onChange={event => { setFile(event.target.files?.[0] ?? null); setError(''); }}/></label>
    {file && <p>{file.name} ? {file.size.toLocaleString()} bytes</p>}
    <label>License<input maxLength={256} value={license} onChange={event => setLicense(event.target.value)}/></label><label>Attribution<input maxLength={512} value={attribution} onChange={event => setAttribution(event.target.value)}/></label>
    <button type="submit" disabled={!file}>{busy ? 'Importing?' : 'Import and add image'}</button><button type="button" onClick={onCancel}>Cancel</button></fieldset>
    {error && <p role="alert">{error}</p>}
  </form>;
}
