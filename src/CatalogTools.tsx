// Settings → Library catalog (ADR-097): export the game list to a CSV file, forget a
// library folder (e.g. a drive you no longer use) or entries whose files are gone.
// Catalog records only: files on drives are never touched. Start over (ADR-098):
// reset the library after saving a full copy of the catalog, which can be put back.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Database, Download, FolderOpen, CheckCircle2, RotateCcw } from 'lucide-react';

type Folder = { id: string; drive: string; path: string; files: number; missing: number; last_scan: string|null; connected: boolean };
type Removal = { files: number; games: number; in_collections: number; kept: number };
type Exported = { path: string; name: string; rows: number };
type Counts = { games: number; files: number; folders: number; collections: number };
type ResetPreview = { clears: Counts; systems: number; reference_entries: number; theme_projects: number; designs: number };
type Saved = { path: string; name: string; before: string; saved_at: string; counts: Counts|null };
type Done = { saved: Saved; counts: Counts };

const n = (x: number, one: string, many = `${one}s`) => `${x.toLocaleString()} ${x === 1 ? one : many}`;

export default function CatalogTools({ busy, notify, onChanged }: { busy: boolean; notify: (text: string, error?: boolean) => void; onChanged: () => void }) {
  const [folders, setFolders] = useState<Folder[]|null>(null);
  const [missing, setMissing] = useState<Removal|null>(null);
  const [exporting, setExporting] = useState(false);
  const [exported, setExported] = useState<Exported|null>(null);
  // The removal being reviewed: a folder id, or 'missing'.
  const [reviewing, setReviewing] = useState<string|null>(null);
  const [preview, setPreview] = useState<Removal|null>(null);
  const [confirm, setConfirm] = useState(false);
  const [working, setWorking] = useState(false);
  const [done, setDone] = useState<string|null>(null);
  // Start over
  const [resetPreview, setResetPreview] = useState<ResetPreview|null>(null);
  const [resetOpen, setResetOpen] = useState(false);
  const [saved, setSaved] = useState<Saved[]>([]);
  // What's waiting for the second click: 'reset' or a saved catalog's path.
  const [armed, setArmed] = useState<string|null>(null);

  const refresh = async () => {
    try { setFolders(await invoke<Folder[]>('library_folders')); } catch (e) { notify(String(e), true); setFolders([]); }
    try { setMissing(await invoke<Removal>('catalog_removal_preview', { folder: null })); } catch { setMissing(null); }
    try { setSaved(await invoke<Saved[]>('saved_catalogs')); } catch { setSaved([]); }
  };
  const openReset = async () => {
    if (resetOpen) { setResetOpen(false); return; }
    setResetOpen(true); setResetPreview(null); setArmed(null); setDone(null);
    try { setResetPreview(await invoke<ResetPreview>('catalog_reset_preview')); } catch (e) { notify(String(e), true); setResetOpen(false); }
  };
  const doReset = async () => {
    if (!resetPreview) return;
    if (armed !== 'reset') { setArmed('reset'); return; }
    setWorking(true);
    try {
      await invoke<Done>('reset_catalog', { expected: resetPreview.clears });
      setDone(`The library catalog is empty. The old one was saved first; use Put back below to return to it. Scan your game folders again from the Dashboard to start over.`);
      setResetOpen(false); onChanged(); await refresh();
    } catch (e) { notify(String(e), true); } finally { setWorking(false); setArmed(null); }
  };
  const putBack = async (c: Saved) => {
    if (armed !== c.path) { setArmed(c.path); return; }
    setWorking(true);
    try {
      const d = await invoke<Done>('put_back_catalog', { path: c.path });
      setDone(`Put back the catalog saved ${c.saved_at}: ${n(d.counts.games, 'game')}, ${n(d.counts.folders, 'folder')}, ${n(d.counts.collections, 'collection')}. The catalog from just before was saved too.`);
      onChanged(); await refresh();
    } catch (e) { notify(String(e), true); } finally { setWorking(false); setArmed(null); }
  };
  useEffect(() => { void refresh(); }, []);

  const exportList = async () => {
    setExporting(true); setExported(null);
    try { setExported(await invoke<Exported>('export_library')); } catch (e) { notify(String(e), true); } finally { setExporting(false); }
  };
  const review = async (what: string) => {
    if (reviewing === what) { setReviewing(null); return; }
    setReviewing(what); setPreview(null); setConfirm(false); setDone(null);
    try { setPreview(await invoke<Removal>('catalog_removal_preview', { folder: what === 'missing' ? null : what })); }
    catch (e) { notify(String(e), true); setReviewing(null); }
  };
  const remove = async () => {
    if (!reviewing || !preview) return;
    if (!confirm) { setConfirm(true); return; }
    setWorking(true);
    try {
      const r = await invoke<Removal>('remove_catalog_entries', { folder: reviewing === 'missing' ? null : reviewing, expected: preview });
      setDone(`Removed ${n(r.games, 'game')} (${n(r.files, 'file entry', 'file entries')}) from the catalog. No files were touched.`);
      setReviewing(null); setPreview(null); onChanged(); await refresh();
    } catch (e) { notify(String(e), true); setConfirm(false); } finally { setWorking(false); }
  };

  const reviewBox = (what: string, label: string) => reviewing === what && <div className="confirm">
    {!preview ? <p className="plan-notes">Counting…</p> : <>
      <p>Remove {label} from the catalog: <b>{n(preview.games, 'game')}</b> ({n(preview.files, 'file entry', 'file entries')}) disappear from your library.
        {preview.in_collections > 0 && <> {n(preview.in_collections, 'of them is', 'of them are')} in your collections and will leave them.</>}
        {preview.kept > 0 && <> {n(preview.kept, 'file stays', 'files stay')}: another library folder holds {preview.kept === 1 ? 'it' : 'them'}.</>}</p>
      <p className="plan-notes">Only RetroDrive's list changes: no file on any drive is touched, and games already copied to your systems stay there.{what !== 'missing' && ' Scan the folder again any time to bring its games back.'}</p>
      <div className="actions">
        <button className="primary" disabled={working || busy || (what === 'missing' && preview.files === 0)} onClick={() => void remove()} onBlur={() => setConfirm(false)}>{working ? 'Removing… (up to a minute on a big library)' : confirm ? 'Click again to remove' : 'Remove from catalog'}</button>
        <button className="ghost" disabled={working} onClick={() => setReviewing(null)}>Not now</button></div>
    </>}</div>;

  return <section className="panel"><h2><Database size={18}/> Library catalog</h2>
    <div className="designs-head"><p className="plan-notes">Your game list, kept on this PC. Save a copy to open in Excel, or tidy up entries you don't need.</p>
      <div className="actions"><button className="primary" disabled={exporting} onClick={() => void exportList()}><Download size={16}/>{exporting ? 'Saving…' : 'Export game list'}</button></div></div>
    {exported && <p><CheckCircle2 size={15} className="ok-text"/> Saved {n(exported.rows, 'game')} to <span className="mono">Documents\{exported.name}</span>. <button className="link" onClick={() => void invoke('show_exported_library', { path: exported.path }).catch(e => notify(String(e), true))}><FolderOpen size={14}/> Show</button></p>}
    {done && <p><CheckCircle2 size={15} className="ok-text"/> {done}</p>}
    {busy && <p className="plan-notes warn-text">A scan or copy is running. You can remove entries when it finishes.</p>}

    <h3>Library folders</h3>
    {folders === null ? <p className="plan-notes">Looking…</p> : folders.length === 0 ? <div className="empty">No folders scanned yet.</div> :
      <ul className="backup-list">{folders.map(f => <li key={f.id}>
        <span className="mono">{f.path}</span> · {n(f.files, 'file')}{f.missing > 0 && <span className="warn-text"> · {f.missing.toLocaleString()} missing</span>}{!f.connected && <span className="plan-notes"> · drive not connected</span>}
        {' '}<button className="link" disabled={working} onClick={() => void review(f.id)}>{reviewing === f.id ? 'Hide' : 'Remove from catalog…'}</button>
        {reviewBox(f.id, 'this folder')}
      </li>)}</ul>}
    <p className="plan-notes">To hide a game that's still on a drive, use <b>Leave out</b> on it in the Game Library instead; it stays hidden after rescans.</p>

    {missing && missing.files > 0 && <>
      <h3>Missing files</h3>
      <p>{n(missing.files, 'file')} {missing.files === 1 ? 'was' : 'were'} gone the last time {missing.files === 1 ? 'its' : 'their'} folder was scanned. <button className="link" disabled={working} onClick={() => void review('missing')}>{reviewing === 'missing' ? 'Hide' : 'Clean up missing entries…'}</button></p>
      {reviewBox('missing', 'the missing entries')}
    </>}

    <h3>Start over</h3>
    <p className="plan-notes">Empties your library so you can scan again from scratch. A full copy of the catalog is saved first, and you can put it back. <button className="link" disabled={working} onClick={() => void openReset()}>{resetOpen ? 'Hide' : 'Reset the catalog…'}</button></p>
    {resetOpen && <div className="confirm">
      {!resetPreview ? <p className="plan-notes">Counting…</p> : <>
        <p><b>Clears</b> {n(resetPreview.clears.games, 'game')} from {n(resetPreview.clears.folders, 'scanned folder')}, {n(resetPreview.clears.collections, 'collection')}, art picks and Leave-out choices.</p>
        <p><b>Keeps</b> your {n(resetPreview.systems, 'emulation system')} and their choices, copy history, {n(resetPreview.designs, 'design')}, {n(resetPreview.theme_projects, 'Theme Studio project')}{resetPreview.reference_entries > 0 ? ' and the reference catalog' : ''}. No file on any drive is touched.</p>
        <div className="actions"><button className="primary" disabled={working || busy} onClick={() => void doReset()} onBlur={() => setArmed(null)}>{working ? 'Saving a copy, then resetting…' : armed === 'reset' ? 'Click again to reset' : <><RotateCcw size={16}/> Reset the catalog</>}</button>
          <button className="ghost" disabled={working} onClick={() => setResetOpen(false)}>Not now</button></div>
      </>}</div>}
    {saved.length > 0 && <>
      <p className="plan-notes">Saved catalogs (kept next to the catalog; RetroDrive never deletes them). <button className="link" onClick={() => void invoke('open_catalog_folder').catch(e => notify(String(e), true))}><FolderOpen size={14}/> Open folder</button></p>
      <ul className="backup-list">{saved.map(c => <li key={c.path}>
        <b>{c.saved_at.slice(0, 16)}</b> · before {c.before === 'reset' ? 'a reset' : 'putting one back'}{c.counts ? ` · ${n(c.counts.games, 'game')}, ${n(c.counts.collections, 'collection')}` : " · can't be read"}
        {c.counts && <> <button className="link" disabled={working || busy} onClick={() => void putBack(c)} onBlur={() => setArmed(null)}>{working && armed === c.path ? 'Putting back…' : armed === c.path ? 'Click again to put it back' : 'Put back'}</button></>}
      </li>)}</ul>
      {armed && armed !== 'reset' && <p className="plan-notes">Putting a catalog back replaces the library you have now; that one is saved first, so you can return to it.</p>}
    </>}
  </section>;
}
