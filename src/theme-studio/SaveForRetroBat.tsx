// Save for RetroBat (ADR-113): shows what will be saved, then writes the theme to a new folder.
import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { ProjectSession } from './session';
import { previewPng } from './api';
import { errorText } from './document';
import { bakeScreens, exportDocument, loadPictures, planExport, type Baked } from './export-retrobat';

type Saved = { folder: string; name: string; files: number; bytes: number };
type Install = { id: string; adapter_id: string; path: string; presence: string };
type Preview = { changes: unknown[]; file_sha: string };

export default function SaveForRetroBat({ session, onClose }: { session: ProjectSession; onClose: () => void }) {
  const plan = useMemo(() => planExport(session.document), [session.document]);
  const [folder, setFolder] = useState<string | null>(null);
  const [destination, setDestination] = useState('');
  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState('');
  const [error, setError] = useState('');
  const [saved, setSaved] = useState<Saved | null>(null);
  const [installs, setInstalls] = useState<Install[]>([]);
  const [target, setTarget] = useState('');
  const [switchNow, setSwitchNow] = useState(true);
  const [placed, setPlaced] = useState<{ name: string; switched: boolean; note: string } | null>(null);
  const ref = useRef<HTMLElement>(null);
  useEffect(() => { if (!saved) return; void invoke<Install[]>('list_installations').then(rows => { const bats = rows.filter(r => r.adapter_id === 'retrobat'); setInstalls(bats); setTarget(t => t || bats.find(b => b.presence === 'connected')?.id || bats[0]?.id || ''); }).catch(() => setInstalls([])); }, [saved]);
  // Put on RetroBat (ADR-115): copy as a new theme folder, then switch through Customize's
  // reviewed, backed-up and undoable theme switch.
  const putOn = async () => {
    if (!saved || !target) return;
    setBusy(true); setError(''); setPlaced(null);
    try {
      setStep('Putting it on RetroBat…');
      const put = await invoke<Saved>('put_theme_on_retrobat', { installationId: target, folder: saved.folder });
      if (!switchNow) { setPlaced({ name: put.name, switched: false, note: '' }); return; }
      setStep('Switching RetroBat to it…');
      try {
        const preview = await invoke<Preview>('preview_theme_switch', { installationId: target, theme: put.name });
        await invoke<string>('apply_theme_switch', { installationId: target, theme: put.name, fileSha: preview.file_sha });
        setPlaced({ name: put.name, switched: true, note: '' });
      } catch (e) { setPlaced({ name: put.name, switched: false, note: errorText(e) }); }
    } catch (e) { setError(errorText(e)); } finally { setBusy(false); setStep(''); }
  };
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
      <p role="status">Saved <b>{saved.name}</b> ({saved.files} files, {saved.bytes >= 1024 * 1024 ? `${(saved.bytes / 1024 / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(saved.bytes / 1024))} KB`}) to <span className="mono">{saved.folder}</span>.</p>
      {!placed ? (installs.length ? <div className="ts-put-on">
        <h4>Put it on RetroBat</h4>
        {installs.length > 1 && <label>RetroBat<select value={target} onChange={e => setTarget(e.target.value)}>{installs.map(i => <option key={i.id} value={i.id}>{i.path}{i.presence !== 'connected' ? ' (not connected)' : ''}</option>)}</select></label>}
        <p className="ts-field-help">Adds it to {installs.length === 1 ? <span className="mono">{installs[0].path}</span> : 'that RetroBat'} as a new theme. No theme there is replaced.</p>
        <label className="ts-check"><input type="checkbox" checked={switchNow} onChange={e => setSwitchNow(e.target.checked)}/> Switch RetroBat to it now (you can undo this in Customize; close RetroBat first)</label>
        <div className="actions"><button className="primary" disabled={busy || !target} onClick={() => void putOn()}>{busy ? step || 'Working…' : 'Put on RetroBat'}</button></div>
      </div> : <p className="ts-field-help">To put themes on RetroBat for you, add your RetroBat in Emulation Systems first.</p>)
      : <p role="status">{placed.switched ? <>Added <b>{placed.name}</b> to RetroBat and made it RetroBat's theme. Start RetroBat to see it. To go back, use Undo in Customize.</> : <>Added <b>{placed.name}</b> to RetroBat's themes.{placed.note ? <> It isn't RetroBat's theme yet: {placed.note}</> : <> Choose it in RetroBat (Start → UI Settings → Theme) or in Customize.</>}</>}</p>}
      <div className="actions"><button onClick={() => { void invoke('open_saved_theme', { folder: saved.folder }).catch(e => setError(errorText(e))); }}>Open folder</button><button className="ghost" onClick={onClose}>Close</button></div>
    </>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
