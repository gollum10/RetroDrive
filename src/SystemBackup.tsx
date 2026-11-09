// Backups (FR-09, ADR-090/091): one click copies a system's saves, settings and game
// lists into a new folder on this PC (reads only on the system). Restore puts files
// back after a preview: missing files are added; different ones keep the system's copy
// unless the user chooses the backup's (then the system's is moved aside, never
// deleted). Every restore can be undone.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { HardDriveDownload, FolderOpen, CheckCircle2, AlertTriangle, Undo2 } from 'lucide-react';
import { fmtSize } from './format';
import { getBackupFolder } from './backupFolder';

// Systems with backup and restore so far.
export const BACKUP_ADAPTERS = ['retrobat', 'esde'];

type Group = { id: string; label: string; optional: boolean; files: number; bytes: number; note: string|null };
type Preview = { system: string; location: string; groups: Group[]; destination: string; free_bytes: number|null; blocked: string|null };
type Entry = { folder: string; name: string; created_at: string|null; installation_id: string|null; system: string|null; location: string|null; files: number; bytes: number; groups: string[]; complete: boolean; problems: number };
type Check = { files: number; ok: number; missing: string[]; changed: string[] };
type RGroup = { id: string; label: string; add: number; same: number; different: number; unavailable: number };
type RPreview = { backup: string; created_at: string; system: string; from_location: string; target_location: string; same_system: boolean; groups: RGroup[]; add: number; same: number; different: number; unavailable: number; bytes_add: number; bytes_different: number; different_files: string[]; unavailable_note: string|null; blocked: string|null; fingerprint: string };
type RSummary = { id: string; added: number; replaced: number; kept: number; failed: string[]; message: string };
type Restore = { id: string; created_at: string; backup: string; added: number; replaced: number; state: string };

const EXTRAS_KEY = 'rd.backupExtras';
const GROUP_LABEL: Record<string, string> = { saves: 'Saves', settings: 'Settings', gamelists: 'Game lists', media: 'Music and boot videos', art: 'Downloaded art' };
const load = (k: string) => { try { return localStorage.getItem(k); } catch { return null; } };
const save = (k: string, v: string|null) => { try { if (v === null) localStorage.removeItem(k); else localStorage.setItem(k, v); } catch { /* not remembered */ } };
const when = (t: string|null) => t ? t.replace('T', ' ').slice(0, 16) : 'unknown date';
const files = (n: number) => `${n.toLocaleString()} file${n === 1 ? '' : 's'}`;

export default function SystemBackup({ installationId, sysName, disabled, notify }: { installationId: string; sysName: string; disabled: boolean; notify: (text: string, error?: boolean) => void }) {
  const [folder] = useState<string|null>(() => getBackupFolder());
  const [extras, setExtras] = useState<string[]>(() => { try { return JSON.parse(load(EXTRAS_KEY) ?? '[]'); } catch { return []; } });
  const [preview, setPreview] = useState<Preview|null>(null);
  const [backups, setBackups] = useState<Entry[]>([]);
  const [restores, setRestores] = useState<Restore[]>([]);
  const [details, setDetails] = useState(false);
  const [showList, setShowList] = useState(false);
  const [working, setWorking] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number }|null>(null);
  const [last, setLast] = useState<Entry|null>(null);
  const [checks, setChecks] = useState<Record<string, Check|string>>({});
  // Restore
  const [restoreFor, setRestoreFor] = useState<Entry|null>(null);
  const [rGroups, setRGroups] = useState<string[]>([]);
  const [rPreview, setRPreview] = useState<RPreview|null>(null);
  const [useBackup, setUseBackup] = useState(false);
  const [showDifferent, setShowDifferent] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [rDone, setRDone] = useState<string|null>(null);

  const refresh = async () => {
    try { setBackups((await invoke<Entry[]>('list_system_backups', { folder })).filter(b => b.installation_id === installationId || b.system === sysName)); } catch { setBackups([]); }
    try { setRestores(await invoke<Restore[]>('list_restores', { installationId })); } catch { setRestores([]); }
  };
  useEffect(() => { setPreview(null); setLast(null); setChecks({}); setRestoreFor(null); setRDone(null); void refresh(); }, [installationId, folder]);
  useEffect(() => { if (details && !preview) void invoke<Preview>('system_backup_preview', { installationId, folder }).then(setPreview).catch(e => notify(String(e), true)); }, [details, preview, installationId, folder]);
  useEffect(() => { const un = listen<{ done: number; total: number }>('backup-progress', e => setProgress(e.payload)); return () => { void un.then(f => f()); }; }, []);
  useEffect(() => {
    if (!restoreFor) return;
    setRPreview(null); setConfirm(false);
    if (rGroups.length === 0) return;
    void invoke<RPreview>('restore_preview', { installationId, backup: restoreFor.folder, groups: rGroups, folder }).then(setRPreview).catch(e => { notify(String(e), true); setRestoreFor(null); });
  }, [restoreFor, rGroups]);

  const run = async () => {
    setWorking(true); setProgress(null); setLast(null);
    try {
      const b = await invoke<Entry>('run_system_backup', { installationId, optional: extras, folder });
      setLast(b); await refresh();
      if (!b.complete) notify(`The backup finished, but ${b.problems} file${b.problems === 1 ? '' : 's'} couldn't be copied. It's kept as "(incomplete)".`, true);
    } catch (e) { notify(String(e), true); } finally { setWorking(false); setProgress(null); }
  };
  const toggleExtra = (id: string) => { const next = extras.includes(id) ? extras.filter(x => x !== id) : [...extras, id]; setExtras(next); save(EXTRAS_KEY, JSON.stringify(next)); };
  const check = async (b: Entry) => {
    setChecks(c => ({ ...c, [b.folder]: 'Checking…' }));
    try { const r = await invoke<Check>('check_system_backup', { folder, backup: b.folder }); setChecks(c => ({ ...c, [b.folder]: r })); }
    catch (e) { setChecks(c => ({ ...c, [b.folder]: String(e) })); }
  };
  const openIt = (backup?: string) => void invoke('open_system_backups', { folder, backup: backup ?? null }).catch(e => notify(String(e), true));
  const startRestore = (b: Entry) => { setRestoreFor(b); setRGroups(b.groups); setUseBackup(false); setShowDifferent(false); setRDone(null); };
  const doRestore = async () => {
    if (!restoreFor || !rPreview) return;
    if (!confirm) { setConfirm(true); return; }
    setWorking(true);
    try {
      const r = await invoke<RSummary>('apply_restore', { installationId, backup: restoreFor.folder, groups: rGroups, useBackup, fingerprint: rPreview.fingerprint, folder });
      setRDone(r.message); setRestoreFor(null); setShowList(true); await refresh();
      if (r.failed.length) notify(`${r.failed.length} file${r.failed.length === 1 ? '' : 's'} couldn't be restored: ${r.failed.slice(0, 3).join('; ')}`, true);
    } catch (e) { notify(String(e), true); setConfirm(false); } finally { setWorking(false); }
  };
  const undo = async (r: Restore) => {
    setWorking(true);
    try { setRDone(await invoke<string>('undo_restore', { installationId, restoreId: r.id })); await refresh(); }
    catch (e) { notify(String(e), true); } finally { setWorking(false); }
  };
  const newest = backups.find(b => b.installation_id === installationId);
  const checkText = (c: Check|string|undefined) => c === undefined ? null : typeof c === 'string' ? c : c.missing.length + c.changed.length === 0 ? `All ${c.files.toLocaleString()} files match.` : `${c.missing.length} missing, ${c.changed.length} changed since the backup.`;
  const p = rPreview;
  const nothing = !!p && p.add === 0 && (p.different === 0 || !useBackup);

  return <div className="backups system-backup">
    <div className="designs-head"><h3><HardDriveDownload size={17}/> Backup and restore</h3>
      <div className="actions"><button className="primary" disabled={disabled || working} onClick={() => void run()}>{working && !restoreFor ? (progress ? `Backing up… ${progress.done.toLocaleString()} of ${progress.total.toLocaleString()}` : 'Working…') : 'Back up now'}</button></div></div>
    <p className="plan-notes">Copies {sysName}'s saves, settings and game lists to a new folder on this PC. Nothing on {sysName} is changed. {newest ? <>Last backup: <b>{when(newest.created_at)}</b>{newest.complete ? '' : ' (incomplete)'}.</> : 'No backup yet.'}{' '}
      <button className="link" onClick={() => setDetails(!details)}>{details ? 'Hide details' : 'What is backed up'}</button>{(backups.length > 0 || restores.length > 0) && <> · <button className="link" onClick={() => setShowList(!showList)}>{showList ? 'Hide backups' : `Show backups and restore (${backups.length})`}</button></>}</p>
    {last && <p>{last.complete ? <CheckCircle2 size={15} className="ok-text"/> : <AlertTriangle size={15} className="warn-text"/>} Backed up {files(last.files)} ({fmtSize(last.bytes)}) to <span className="mono">{last.name}</span>. <button className="link" onClick={() => openIt(last.folder)}><FolderOpen size={14}/> Open</button></p>}
    {rDone && <p><CheckCircle2 size={15} className="ok-text"/> {rDone}</p>}
    {details && <div className="backup-details">
      {!preview ? <p className="plan-notes">Looking…</p> : <>
        {preview.blocked && <p className="warn-text"><AlertTriangle size={15}/> {preview.blocked}</p>}
        <ul>{preview.groups.map(g => <li key={g.id}>{g.optional
          ? <label><input type="checkbox" checked={extras.includes(g.id)} onChange={() => toggleExtra(g.id)}/> Also back up <b>{g.label.toLowerCase()}</b>: {files(g.files)}, {fmtSize(g.bytes)}</label>
          : <><b>{g.label}</b>: {g.files ? `${files(g.files)}, ${fmtSize(g.bytes)}` : g.note}</>}</li>)}</ul>
        <p className="plan-notes">Games, BIOS files, themes and emulators aren't included: they're in your library or come with {sysName}.</p>
        <p className="plan-notes">Backups go to <span className="mono">{preview.destination}</span>{preview.free_bytes !== null ? ` (${fmtSize(preview.free_bytes)} free)` : ''}. You can change this in <b>Settings</b>.</p>
      </>}</div>}

    {restoreFor && <div className="confirm">
      <p><b>Restore from the backup of {when(restoreFor.created_at)}</b>{p && !p.same_system && <> (made from {p.from_location})</>} onto {sysName} at <span className="mono">{p?.target_location ?? ''}</span></p>
      <p>{restoreFor.groups.map(g => <label key={g} className="inline-check"><input type="checkbox" checked={rGroups.includes(g)} onChange={() => setRGroups(rGroups.includes(g) ? rGroups.filter(x => x !== g) : [...rGroups, g])}/> {GROUP_LABEL[g] ?? g} </label>)}</p>
      {rGroups.length === 0 ? <p className="plan-notes">Tick what to restore.</p> : !p ? <p className="plan-notes">Comparing the backup with {sysName}…</p> : <>
        {p.blocked && <p className="warn-text"><AlertTriangle size={15}/> {p.blocked}</p>}
        <ul>
          <li><b>{files(p.add)}</b> missing on {sysName} will be added{p.add ? ` (${fmtSize(p.bytes_add)})` : ''}.</li>
          <li>{files(p.same)} already the same: skipped.</li>
          {p.different > 0 && <li><b>{files(p.different)}</b> different on {sysName} ({p.groups.filter(g => g.different).map(g => `${g.label.toLowerCase()}: ${g.different}`).join(', ')}). <button className="link" onClick={() => setShowDifferent(!showDifferent)}>{showDifferent ? 'Hide' : 'Show them'}</button>
            {showDifferent && <ul className="mono">{p.different_files.map(f => <li key={f}>{f}</li>)}{p.different > p.different_files.length && <li>…and {p.different - p.different_files.length} more</li>}</ul>}
            <div><label><input type="radio" checked={!useBackup} onChange={() => setUseBackup(false)}/> Keep {sysName}'s copies (they may be newer)</label></div>
            <div><label><input type="radio" checked={useBackup} onChange={() => setUseBackup(true)}/> Use the backup's copies ({sysName}'s copies are moved to <span className="mono">.retrodrive\backup</span>, not deleted)</label></div></li>}
          {p.unavailable > 0 && <li className="plan-notes">{files(p.unavailable)} left out: {p.unavailable_note}</li>}
          <li className="plan-notes">Nothing is deleted, and Undo puts everything back as it was.</li>
        </ul></>}
      <div className="actions"><button className="primary" disabled={working || !p || !!p.blocked || nothing} onClick={() => void doRestore()}>{working ? 'Restoring…' : nothing ? 'Nothing to restore' : confirm ? 'Click again to restore' : 'Restore'}</button><button className="ghost" disabled={working} onClick={() => setRestoreFor(null)}>Not now</button></div>
    </div>}

    {showList && <>
      <ul className="backup-list">{backups.map(b => <li key={b.folder}>
        <b>{when(b.created_at)}</b> · {files(b.files)}, {fmtSize(b.bytes)}{b.installation_id !== installationId && b.location ? <span className="plan-notes"> · from {b.location}</span> : ''}{b.complete ? '' : <span className="warn-text"> · incomplete{b.problems ? ` (${b.problems} not copied)` : ''}</span>}
        {' '}{b.complete && <><button className="link" disabled={working} onClick={() => startRestore(b)}>Restore…</button> · </>}<button className="link" onClick={() => openIt(b.folder)}>Open</button>{b.complete && <> · <button className="link" onClick={() => void check(b)}>Check</button></>}
        {checkText(checks[b.folder]) && <small className="plan-notes"> {checkText(checks[b.folder])}</small>}
      </li>)}</ul>
      {restores.length > 0 && <><h4>Restores</h4><ul className="backup-list">{restores.map(r => <li key={r.id}>
        <b>{when(r.created_at)}</b> from {r.backup} · {files(r.added)} added, {r.replaced} replaced · {r.state === 'applied' ? 'done' : r.state}
        {r.state !== 'undone' && <> <button className="link" disabled={working} onClick={() => void undo(r)}><Undo2 size={14}/> Undo</button></>}
      </li>)}</ul></>}
    </>}
  </div>;
}
