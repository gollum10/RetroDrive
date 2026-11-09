// Settings → Library catalog (ADR-097): export the game list to a CSV file, forget a
// library folder (e.g. a drive you no longer use) or entries whose files are gone.
// Catalog records only: files on drives are never touched.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Database, Download, FolderOpen, CheckCircle2 } from 'lucide-react';

type Folder = { id: string; drive: string; path: string; files: number; missing: number; last_scan: string|null; connected: boolean };
type Removal = { files: number; games: number; in_collections: number; kept: number };
type Exported = { path: string; name: string; rows: number };

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

  const refresh = async () => {
    try { setFolders(await invoke<Folder[]>('library_folders')); } catch (e) { notify(String(e), true); setFolders([]); }
    try { setMissing(await invoke<Removal>('catalog_removal_preview', { folder: null })); } catch { setMissing(null); }
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
  </section>;
}
