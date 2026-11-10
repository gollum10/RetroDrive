// Save for RetroBat (ADR-113): shows what will be saved, then writes the theme to a new folder.
import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { ProjectSession } from './session';
import { previewPng } from './api';
import { errorText } from './document';
import { bakeScreens, exportDocument, loadPictures, planExport, type Baked } from './export-retrobat';

type Saved = { folder: string; name: string; files: number; bytes: number };

export default function SaveForRetroBat({ session, onClose }: { session: ProjectSession; onClose: () => void }) {
  const plan = useMemo(() => planExport(session.document), [session.document]);
  const [folder, setFolder] = useState<string | null>(null);
  const [destination, setDestination] = useState('');
  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState('');
  const [error, setError] = useState('');
  const [saved, setSaved] = useState<Saved | null>(null);
  const ref = useRef<HTMLElement>(null);
  useEffect(() => { ref.current?.scrollIntoView?.({ block: 'start', behavior: 'smooth' }); }, []);
  useEffect(() => { void invoke<string>('theme_save_destination', { folder }).then(setDestination).catch(e => setError(errorText(e))); }, [folder]);
  const choose = async () => { const picked = await open({ directory: true, multiple: false, title: 'Choose where to save the theme' }); if (typeof picked === 'string') setFolder(picked); };
  const save = async () => {
    setBusy(true); setError(''); setSaved(null);
    try {
      setStep('Saving your latest changes…'); await session.flush();
      const project = session.document; const current = planExport(project);
      setStep('Loading your pictures…');
      const pictures = await loadPictures(project, current.screens, id => previewPng(session.record.id, id));
      setStep('Making pictures from your design…');
      const baked = await bakeScreens(current, pictures, encoded => invoke<Baked['info']>('prepare_theme_bake', { pngBase64: encoded }));
      const exportJson = JSON.stringify(exportDocument(project, current, baked));
      const encoded = [...new Set([...baked.values()].map(b => b.encoded))];
      setStep('Writing the theme…');
      setSaved(await invoke<Saved>('save_theme_for_retrobat', { projectId: session.record.id, revision: session.record.revision, exportJson, baked: encoded, folder }));
    } catch (e) { setError(errorText(e)); } finally { setBusy(false); setStep(''); }
  };
  const live = plan.live.length ? plan.live.join(', ') : 'nothing that changes per game yet';
  return <section ref={ref} className="panel ts-save-retrobat" aria-label="Save for RetroBat">
    <h3>Save for RetroBat</h3>
    {!saved ? <>
      <p>RetroDrive makes a RetroBat theme from your design and saves it in a new folder. Nothing on RetroBat changes.</p>
      <ul>
        <li><b>Screens:</b> {plan.screens.map(s => s.scene.name).join(' and ') || 'none'}.</li>
        <li><b>Your design:</b> {plan.bakeCount ? `drawn into ${plan.bakeCount} ${plan.bakeCount === 1 ? 'picture' : 'pictures'}, exactly as you see it here (panels, pictures, text, turning and fading included).` : 'nothing to draw yet.'}</li>
        <li><b>Changes with each game:</b> {live}.</li>
      </ul>
      {plan.notes.length > 0 && <details open={plan.notes.length < 4}><summary>Good to know ({plan.notes.length})</summary><ul>{plan.notes.map(note => <li key={note}>{note}</li>)}</ul></details>}
      <p className="ts-field-help">Saves to <span className="mono">{destination || '…'}</span> <button type="button" className="link" disabled={busy} onClick={() => void choose()}>Choose another folder…</button></p>
      <div className="actions"><button className="primary" disabled={busy || !plan.screens.length} onClick={() => void save()}>{busy ? step || 'Saving…' : 'Save theme'}</button><button className="ghost" disabled={busy} onClick={onClose}>Cancel</button></div>
    </> : <>
      <p role="status">Saved <b>{saved.name}</b> ({saved.files} files, {(saved.bytes / 1024 / 1024).toFixed(1)} MB) to <span className="mono">{saved.folder}</span>.</p>
      <p className="ts-field-help">Next, RetroDrive will be able to try it in RetroBat and put it on your RetroBat for you. Until then you can copy this folder into RetroBat's themes folder yourself.</p>
      <div className="actions"><button onClick={() => { void invoke('open_saved_theme', { folder: saved.folder }).catch(e => setError(errorText(e))); }}>Open folder</button><button className="ghost" onClick={onClose}>Close</button></div>
    </>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
