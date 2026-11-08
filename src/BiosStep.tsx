// BIOS files outside the installation folder (RetroPie's bios share, ES-DE's RetroArch
// system folder): check, then copy the missing ones. New files only; nothing replaced.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Eye } from 'lucide-react';
import { fmtSize } from './format';

type Preview = { target: string; to_copy: number; bytes: number; already_there: number; kept: string[]; kept_count: number; two_versions: string[]; two_versions_count: number };
type Progress = { phase: string; done: number; total: number };

export default function BiosStep({ installationId, adapterId, disabled, notify }: { installationId: string; adapterId: string; disabled: boolean; notify: (text: string, error?: boolean) => void }) {
  const [p, setP] = useState<Preview|null>(null); const [problem, setProblem] = useState(''); const [checking, setChecking] = useState(false);
  const [working, setWorking] = useState(false); const [progress, setProgress] = useState<Progress|null>(null); const [confirm, setConfirm] = useState(false);
  useEffect(() => { setP(null); setProblem(''); setConfirm(false); }, [installationId]);
  useEffect(() => { const un = listen<Progress>('art-progress', e => { if (working) setProgress(e.payload); }); return () => { void un.then(f => f()); }; }, [working]);
  const where = adapterId === 'esde' ? "RetroArch's system folder (where ES-DE's RetroArch looks for BIOS files)" : adapterId === 'lakka' ? "Lakka's System share (where its RetroArch looks for BIOS files)" : "RetroPie's bios share";
  const check = async () => { setChecking(true); setProblem(''); setConfirm(false);
    try { setP(await invoke<Preview>('preview_bios_step', { installationId })); } catch (e) { setP(null); setProblem(String(e)); } finally { setChecking(false); } };
  const copy = async () => { if (!confirm) { setConfirm(true); return; } setWorking(true); setProgress(null);
    try { notify(await invoke<string>('apply_bios_step', { installationId })); setP(null); } catch (e) { notify(`Could not copy BIOS files: ${String(e)}`, true); } finally { setWorking(false); setConfirm(false); setProgress(null); } };

  return <div className="backups"><h3>BIOS files</h3>
    <p className="plan-notes">Many systems need BIOS files to start games. This copies the ones in your library's <span className="mono">bios</span> folders into {where}, keeping their subfolders. Only missing files are copied; files already there are never replaced.</p>
    {!p ? <><div className="actions"><button className="ghost" disabled={disabled || checking} onClick={() => void check()}><Eye size={15}/> {checking ? 'Checking…' : 'Check BIOS files'}</button></div>
      {problem && <p className="warn-text">{problem}</p>}</>
    : p.to_copy === 0 ? <p className="plan-notes">Nothing to copy: {p.already_there.toLocaleString()} already there{p.kept_count ? `, ${p.kept_count.toLocaleString()} different file(s) kept` : ''}. <button className="link" onClick={() => setP(null)}>close</button></p>
    : <div className="confirm"><p><b>Copy {p.to_copy.toLocaleString()} BIOS file{p.to_copy === 1 ? '' : 's'}</b> ({fmtSize(p.bytes)}) to <span className="mono">{p.target}</span>?</p><ul>
        <li>{p.already_there.toLocaleString()} already there (same file).</li>
        {p.kept_count > 0 && <li>{p.kept_count.toLocaleString()} different file(s) with the same name already there are kept{p.kept.length ? ` (e.g. ${p.kept.slice(0, 3).join(', ')})` : ''}.</li>}
        {p.two_versions_count > 0 && <li>{p.two_versions_count.toLocaleString()} file(s) exist in two different versions in your library and aren't copied{p.two_versions.length ? ` (e.g. ${p.two_versions.slice(0, 3).join(', ')})` : ''}.</li>}
        <li className="plan-notes">New files only; nothing is replaced or deleted.</li></ul>
      {working && progress && <p className="plan-notes">Copying: {progress.done.toLocaleString()} of {progress.total.toLocaleString()}…</p>}
      <div className="actions"><button className="primary" disabled={working || disabled} onClick={() => void copy()}>{working ? 'Copying…' : confirm ? 'Click again to copy' : 'Copy BIOS files'}</button><button className="ghost" disabled={working} onClick={() => { setP(null); setConfirm(false); }}>Not now</button></div></div>}
  </div>;
}
