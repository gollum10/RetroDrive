// BIOS checker (FR-07, ADR-088): for the systems in the library, which BIOS files
// RetroArch's cores look for, and whether they're in the library and on this system.
// Reads only; RetroDrive never ships or downloads BIOS files.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Eye, CheckCircle2, AlertTriangle, Info } from 'lucide-react';

type BiosFile = { file: string; level: string; group: string|null; what: string; core: string; library: string; on_system: boolean|null };
type SystemBios = { system: string; platform: string; games: number; status: string; files: BiosFile[] };
type Check = { systems: SystemBios[]; system_folder: string|null; system_note: string|null };

const STATUS: Record<string, string> = { ok: 'Ready', missing: 'BIOS missing', not_on_system: 'In your library, not on this system yet', recommended: 'Works; a BIOS is recommended' };

export default function BiosCheck({ installationId, sysName, notify }: { installationId: string; sysName: string; notify: (text: string, error?: boolean) => void }) {
  const [c, setC] = useState<Check|null>(null); const [working, setWorking] = useState(false); const [open, setOpen] = useState<string|null>(null);
  useEffect(() => { setC(null); setOpen(null); }, [installationId]);
  const run = async () => { setWorking(true); try { setC(await invoke<Check>('check_bios', { installationId })); } catch (e) { notify(String(e), true); } finally { setWorking(false); } };
  const problems = c ? c.systems.filter(s => s.status === 'missing' || s.status === 'not_on_system').length : 0;
  const icon = (s: string) => s === 'ok' ? <CheckCircle2 size={14} className="ok-text"/> : s === 'recommended' ? <Info size={14}/> : <AlertTriangle size={14} className="warn-text"/>;
  const where = (f: BiosFile) => [f.library === 'yes' ? 'in your library' : f.library === 'other_folder' ? 'in your library, but in another folder' : 'not in your library', f.on_system === null ? '' : f.on_system ? `on ${sysName}` : `not on ${sysName}`].filter(Boolean).join(' · ');
  return <div className="backups"><h3>BIOS checker</h3>
    <p className="plan-notes">Some systems need BIOS files (copies of the console's own chips) to start games. This checks the systems in your library against what RetroArch's emulators look for. RetroDrive never provides BIOS files; use your own.</p>
    {!c ? <div className="actions"><button className="ghost" disabled={working} onClick={() => void run()}><Eye size={15}/> {working ? 'Checking…' : 'Check BIOS files'}</button></div> : <>
      <p>{c.systems.length === 0 ? 'None of the systems in your library need BIOS files.' : problems === 0 ? <><CheckCircle2 size={15} className="ok-text"/> Every system that needs a BIOS has one.</> : <><AlertTriangle size={15} className="warn-text"/> {problems} system{problems === 1 ? '' : 's'} need attention.</>}
        {c.system_note && <small className="plan-notes"> {c.system_note}</small>}</p>
      <ul className="bios-list">{c.systems.map(s => <li key={s.system}>
        <button className="link" onClick={() => setOpen(open === s.system ? null : s.system)}>{icon(s.status)} <b>{s.platform}</b> ({s.games.toLocaleString()} game{s.games === 1 ? '' : 's'}): {STATUS[s.status] ?? s.status}</button>
        {open === s.system && <ul>{s.files.map(f => <li key={f.file}><span className="mono">{f.file}</span> · {f.what} · <i>{f.level}{f.group ? ' (any one of this group)' : ''}</i> · {where(f)}</li>)}
          {s.status === 'not_on_system' && <li className="plan-notes">Copy it with the BIOS row in Plan, or the BIOS files step below where shown.</li>}</ul>}
      </li>)}</ul>
      <div className="actions"><button className="ghost" disabled={working} onClick={() => void run()}>{working ? 'Checking…' : 'Check again'}</button></div></>}
  </div>;
}
