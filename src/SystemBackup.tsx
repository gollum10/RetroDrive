// Backups (FR-09, ADR-090): one click copies a system's saves, settings and game lists
// into a new folder on this PC. Reads only on the system; nothing is ever deleted.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { HardDriveDownload, FolderOpen, CheckCircle2, AlertTriangle } from 'lucide-react';
import { fmtSize } from './format';

type Group = { id: string; label: string; optional: boolean; files: number; bytes: number; note: string|null };
type Preview = { system: string; location: string; groups: Group[]; destination: string; free_bytes: number|null; blocked: string|null };
type Entry = { folder: string; name: string; created_at: string|null; installation_id: string|null; system: string|null; location: string|null; files: number; bytes: number; groups: string[]; complete: boolean; problems: number };
type Check = { files: number; ok: number; missing: string[]; changed: string[] };

const FOLDER_KEY = 'rd.backupFolder';
const EXTRAS_KEY = 'rd.backupExtras';
const load = (k: string) => { try { return localStorage.getItem(k); } catch { return null; } };
const save = (k: string, v: string|null) => { try { if (v === null) localStorage.removeItem(k); else localStorage.setItem(k, v); } catch { /* not remembered */ } };
const when = (t: string|null) => t ? t.replace('T', ' ').slice(0, 16) : 'unknown date';

export default function SystemBackup({ installationId, sysName, disabled, notify }: { installationId: string; sysName: string; disabled: boolean; notify: (text: string, error?: boolean) => void }) {
  const [folder, setFolder] = useState<string|null>(() => load(FOLDER_KEY));
  const [extras, setExtras] = useState<string[]>(() => { try { return JSON.parse(load(EXTRAS_KEY) ?? '[]'); } catch { return []; } });
  const [preview, setPreview] = useState<Preview|null>(null);
  const [backups, setBackups] = useState<Entry[]>([]);
  const [details, setDetails] = useState(false);
  const [showList, setShowList] = useState(false);
  const [working, setWorking] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number }|null>(null);
  const [last, setLast] = useState<Entry|null>(null);
  const [checks, setChecks] = useState<Record<string, Check|string>>({});

  const refresh = async () => {
    try { setBackups((await invoke<Entry[]>('list_system_backups', { folder })).filter(b => b.installation_id === installationId)); } catch { setBackups([]); }
  };
  useEffect(() => { setPreview(null); setLast(null); setChecks({}); void refresh(); }, [installationId, folder]);
  useEffect(() => { if (details && !preview) void invoke<Preview>('system_backup_preview', { installationId, folder }).then(setPreview).catch(e => notify(String(e), true)); }, [details, preview, installationId, folder]);
  useEffect(() => { const un = listen<{ done: number; total: number }>('backup-progress', e => setProgress(e.payload)); return () => { void un.then(f => f()); }; }, []);

  const run = async () => {
    setWorking(true); setProgress(null); setLast(null);
    try {
      const b = await invoke<Entry>('run_system_backup', { installationId, optional: extras, folder });
      setLast(b); await refresh();
      if (!b.complete) notify(`The backup finished, but ${b.problems} file${b.problems === 1 ? '' : 's'} couldn't be copied. It's kept as "(incomplete)".`, true);
    } catch (e) { notify(String(e), true); } finally { setWorking(false); setProgress(null); }
  };
  const chooseFolder = async () => {
    const picked = await open({ directory: true, multiple: false, title: 'Choose where RetroDrive keeps backups' });
    if (typeof picked !== 'string') return;
    setFolder(picked); save(FOLDER_KEY, picked); setPreview(null);
  };
  const useDefault = () => { setFolder(null); save(FOLDER_KEY, null); setPreview(null); };
  const toggleExtra = (id: string) => { const next = extras.includes(id) ? extras.filter(x => x !== id) : [...extras, id]; setExtras(next); save(EXTRAS_KEY, JSON.stringify(next)); };
  const check = async (b: Entry) => {
    setChecks(c => ({ ...c, [b.folder]: 'Checking…' }));
    try { const r = await invoke<Check>('check_system_backup', { folder, backup: b.folder }); setChecks(c => ({ ...c, [b.folder]: r })); }
    catch (e) { setChecks(c => ({ ...c, [b.folder]: String(e) })); }
  };
  const openIt = (backup?: string) => void invoke('open_system_backups', { folder, backup: backup ?? null }).catch(e => notify(String(e), true));
  const newest = backups[0];
  const checkText = (c: Check|string|undefined) => c === undefined ? null : typeof c === 'string' ? c : c.missing.length + c.changed.length === 0 ? `All ${c.files.toLocaleString()} files match.` : `${c.missing.length} missing, ${c.changed.length} changed since the backup.`;

  return <div className="backups system-backup">
    <div className="designs-head"><h3><HardDriveDownload size={17}/> Backup</h3>
      <div className="actions"><button className="primary" disabled={disabled || working} onClick={() => void run()}>{working ? (progress ? `Backing up… ${progress.done.toLocaleString()} of ${progress.total.toLocaleString()}` : 'Backing up…') : 'Back up now'}</button></div></div>
    <p className="plan-notes">Copies {sysName}'s saves, settings and game lists to a new folder on this PC. Nothing on {sysName} is changed. {newest ? <>Last backup: <b>{when(newest.created_at)}</b>{newest.complete ? '' : ' (incomplete)'}.</> : 'No backup yet.'}{' '}
      <button className="link" onClick={() => setDetails(!details)}>{details ? 'Hide details' : 'What is backed up'}</button>{backups.length > 0 && <> · <button className="link" onClick={() => setShowList(!showList)}>{showList ? 'Hide backups' : `Show backups (${backups.length})`}</button></>}</p>
    {last && <p>{last.complete ? <CheckCircle2 size={15} className="ok-text"/> : <AlertTriangle size={15} className="warn-text"/>} Backed up {last.files.toLocaleString()} files ({fmtSize(last.bytes)}) to <span className="mono">{last.name}</span>. <button className="link" onClick={() => openIt(last.folder)}><FolderOpen size={14}/> Open</button></p>}
    {details && <div className="backup-details">
      {!preview ? <p className="plan-notes">Looking…</p> : <>
        {preview.blocked && <p className="warn-text"><AlertTriangle size={15}/> {preview.blocked}</p>}
        <ul>{preview.groups.map(g => <li key={g.id}>{g.optional
          ? <label><input type="checkbox" checked={extras.includes(g.id)} onChange={() => toggleExtra(g.id)}/> Also back up <b>{g.label.toLowerCase()}</b>: {g.files.toLocaleString()} files, {fmtSize(g.bytes)}</label>
          : <><b>{g.label}</b>: {g.files ? `${g.files.toLocaleString()} files, ${fmtSize(g.bytes)}` : g.note}</>}</li>)}</ul>
        <p className="plan-notes">Games, BIOS files, themes and emulators aren't included: they're in your library or come with {sysName}.</p>
        <p className="plan-notes">Backups go to <span className="mono">{preview.destination}</span>{preview.free_bytes !== null ? ` (${fmtSize(preview.free_bytes)} free)` : ''}. <button className="link" onClick={() => void chooseFolder()}>Change folder…</button>{folder && <> · <button className="link" onClick={useDefault}>Use Documents\RetroDrive Backups</button></>}</p>
      </>}</div>}
    {showList && <ul className="backup-list">{backups.map(b => <li key={b.folder}>
      <b>{when(b.created_at)}</b> · {b.files.toLocaleString()} files, {fmtSize(b.bytes)}{b.complete ? '' : <span className="warn-text"> · incomplete{b.problems ? ` (${b.problems} not copied)` : ''}</span>}
      {' '}<button className="link" onClick={() => openIt(b.folder)}>Open</button>{b.complete && <> · <button className="link" onClick={() => void check(b)}>Check</button></>}
      {checkText(checks[b.folder]) && <small className="plan-notes"> {checkText(checks[b.folder])}</small>}
    </li>)}</ul>}
  </div>;
}
