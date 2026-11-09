// Maintenance checks (FR-10, ADR-093): one look at a system's health. Reads only; each
// finding points to the feature that fixes it (Sync, Backup, BIOS checker…).
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Activity, AlertTriangle, CheckCircle2, Info, XCircle } from 'lucide-react';
import { getBackupFolder } from './backupFolder';

type Item = { id: string; label: string; status: 'ok'|'info'|'warning'|'problem'; summary: string; action: string|null };
type Health = { system: string; location: string; items: Item[] };

const ACTION_LABEL: Record<string, string> = { sync: 'Open Sync', plan: 'Plan deployment', backup: 'Back up', bios: 'Open Customize', open_backup_folder: 'Open backup folder' };
const ICON = { ok: <CheckCircle2 size={16} className="ok-text"/>, info: <Info size={16}/>, warning: <AlertTriangle size={16} className="warn-text"/>, problem: <XCircle size={16} className="warn-text"/> };

export default function HealthPanel({ installationId, disabled, onAction, notify }: { installationId: string; disabled: boolean; onAction: (action: string) => void; notify: (text: string, error?: boolean) => void }) {
  const [health, setHealth] = useState<Health|null>(null);
  const [working, setWorking] = useState(false);
  const run = async () => {
    setWorking(true);
    try { setHealth(await invoke<Health>('check_health', { installationId, folder: getBackupFolder() })); }
    catch (e) { notify(String(e), true); } finally { setWorking(false); }
  };
  useEffect(() => { setHealth(null); if (!disabled) void run(); }, [installationId]);
  const attention = health ? health.items.filter(i => i.status === 'warning' || i.status === 'problem').length : 0;
  return <div className="backups health">
    <div className="designs-head"><h3><Activity size={17}/> Health check</h3>
      <div className="actions"><button className="ghost" disabled={disabled || working} onClick={() => void run()}>{working ? 'Checking…' : 'Check again'}</button></div></div>
    {!health ? <p className="plan-notes">{working ? 'Checking space, games, folders, BIOS, backups and leftovers…' : disabled ? 'Connect the drive to check it.' : ''}</p> : <>
      <p>{attention === 0 ? <><CheckCircle2 size={15} className="ok-text"/> Everything looks good.</> : <><AlertTriangle size={15} className="warn-text"/> {attention} thing{attention === 1 ? '' : 's'} to look at.</>} <small className="plan-notes">Nothing was changed.</small></p>
      <ul className="health-list">{health.items.map(i => <li key={i.id}>{ICON[i.status]} <b>{i.label}</b>: {i.summary}
        {i.action && <> <button className="link" disabled={disabled} onClick={() => onAction(i.action!)}>{ACTION_LABEL[i.action] ?? i.action}</button></>}</li>)}</ul>
    </>}
  </div>;
}
