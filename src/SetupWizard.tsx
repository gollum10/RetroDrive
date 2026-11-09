// Setup wizard (owner 2026-11-09; ADR-100): walks a new user through setting up an
// emulation system one step at a time, reusing the existing features. Part 1: Your
// games (scan a folder) and Find your system (search drives and network names, read
// only; the user picks what to add). The later steps hand over to the existing screens
// until they're built into the wizard (part 2).
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { CheckCircle2, FolderOpen, Search, Wand2, X } from 'lucide-react';

type Found = { path: string; adapter_id: string; name: string; version: string|null; added: boolean };
type Install = { id: string; adapter_name: string; path: string };
type Inspection = { path: string; best: { adapter_id: string; display_name: string } | null; detections: { hint: string|null }[] };

export const TASK_TITLE: Record<number, string> = { 0: 'Add your games', 1: 'Find or add a system' };
export const STEPS = ['Your games', 'Find your system', 'What to copy', 'BIOS', 'Copy', 'Back up and check'];
const STEP_KEY = 'rd.wizardStep';
const SYSTEM_KEY = 'rd.wizardSystem';
const load = (k: string) => { try { return localStorage.getItem(k); } catch { return null; } };
const save = (k: string, v: string|null) => { try { if (v === null) localStorage.removeItem(k); else localStorage.setItem(k, v); } catch { /* not remembered */ } };
const same = (a: string, b: string) => a.replace(/[\\/]+$/, '').toLowerCase() === b.replace(/[\\/]+$/, '').toLowerCase();

export default function SetupWizard({ firstRun, only, games, scanning, installs, onScan, onAdded, onPlan, onInstallHelp, onClose, notify }: {
  firstRun: boolean; /** One task on its own (a step number), or the complete wizard. */ only?: number; games: number; scanning: boolean; installs: Install[];
  onScan: () => void; onAdded: () => Promise<void>; onPlan: (installationId: string) => void; onInstallHelp: () => void; onClose: () => void;
  notify: (text: string, error?: boolean) => void;
}) {
  const [step, setStepRaw] = useState(() => only ?? Math.min(Number(load(STEP_KEY) ?? 0) || 0, 1));
  const setStep = (n: number) => { setStepRaw(n); if (only === undefined) save(STEP_KEY, String(n)); };
  const [system, setSystemRaw] = useState<string|null>(() => load(SYSTEM_KEY));
  const setSystem = (id: string|null) => { setSystemRaw(id); save(SYSTEM_KEY, id); };
  const [onDrives, setOnDrives] = useState<Found[]|null>(null);
  const [onNetwork, setOnNetwork] = useState<Found[]|null>(null);
  const [adding, setAdding] = useState<string|null>(null);
  const [hint, setHint] = useState<string|null>(null);

  const search = () => {
    setOnDrives(null); setOnNetwork(null); setHint(null);
    void invoke<Found[]>('find_systems_on_drives').then(setOnDrives).catch(e => { notify(String(e), true); setOnDrives([]); });
    void invoke<Found[]>('find_systems_on_network').then(setOnNetwork).catch(() => setOnNetwork([]));
  };
  useEffect(() => { if (step === 1 && onDrives === null) search(); }, [step]);
  // A system chosen earlier that has since been forgotten isn't chosen any more.
  useEffect(() => { if (system && !installs.some(i => i.id === system)) setSystem(null); }, [installs]);

  const idFor = (path: string) => installs.find(i => same(i.path, path))?.id ?? null;
  const add = async (f: { path: string; adapter_id: string }) => {
    const existing = idFor(f.path);
    if (existing) { setSystem(existing); return; }
    setAdding(f.path);
    try { const id = await invoke<string>('add_installation', { folder: f.path, adapterId: f.adapter_id }); await onAdded(); setSystem(id); }
    catch (e) { notify(`Couldn't add it: ${String(e)}`, true); } finally { setAdding(null); }
  };
  const browse = async () => {
    const folder = await open({ directory: true, multiple: false, title: 'Choose your system: a RetroBat folder, an ES-DE ROMs folder, or a game share such as \\\\BATOCERA\\share' });
    if (typeof folder !== 'string') return;
    setHint(null);
    try {
      const i = await invoke<Inspection>('inspect_installation', { folder });
      if (i.best) await add({ path: i.path, adapter_id: i.best.adapter_id });
      else setHint(i.detections.find(d => d.hint)?.hint ?? "RetroDrive didn't recognise a system in that folder. Choose the folder that holds RetroBat.exe, ES-DE's ROMs folder, or a system's game share.");
    } catch (e) { notify(String(e), true); }
  };
  const close = () => { if (only === undefined) save(STEP_KEY, null); onClose(); };
  const chosen = installs.find(i => i.id === system);
  const found = [...(onDrives ?? []), ...(onNetwork ?? [])];
  const canNext = step === 0 ? games > 0 : !!chosen;

  const row = (f: Found) => {
    const id = idFor(f.path);
    return <li key={f.path} className={id && id === system ? 'chosen' : ''}>
      <b>{f.name}</b>{f.version ? ` ${f.version}` : ''} <span className="mono">{f.path}</span>{' '}
      {id ? (id === system ? <span className="ok-text"><CheckCircle2 size={14}/> Chosen</span> : <button className="link" onClick={() => setSystem(id)}>Use this one</button>)
        : <button className="link" disabled={adding !== null} onClick={() => void add(f)}>{adding === f.path ? 'Adding…' : 'Add and use'}</button>}
    </li>;
  };

  return <section className="panel wizard">
    <div className="designs-head"><h2><Wand2 size={18}/> {only === undefined ? 'Set up everything' : TASK_TITLE[only] ?? STEPS[only]}</h2><button className="ghost" onClick={close} title="Leave the wizard; your choices are kept"><X size={16}/> Exit</button></div>
    {firstRun && step === 0 && <p className="plan-notes">New to RetroDrive? This walks you through setting up your first system, step by step. It's optional: you can leave any time, do everything yourself instead, and come back any time from <b>Quick actions</b> on the Dashboard. <button className="link" onClick={close}>No thanks, I'll do it myself</button></p>}
    {only === undefined && <ol className="wizard-steps">{STEPS.map((s, i) => <li key={s} className={i === step ? 'current' : i < step ? 'done' : ''}>{i < step ? <CheckCircle2 size={14}/> : <span>{i + 1}</span>} {s}</li>)}</ol>}

    {step === 0 && <div className="wizard-body">
      {only === undefined && <h3>Your games</h3>}
      {games > 0
        ? <p><CheckCircle2 size={15} className="ok-text"/> Your library has <b>{games.toLocaleString()}</b> game files. You can add another folder, or go on.</p>
        : <p>First, show RetroDrive where your games are: a folder on this PC, a drive or a network share. RetroDrive lists them; it doesn't copy, move or change anything.</p>}
      <div className="actions"><button className={games > 0 ? 'ghost' : 'primary'} disabled={scanning} onClick={onScan}><FolderOpen size={16}/> {scanning ? 'Scanning… (you can wait here)' : games > 0 ? 'Add another folder' : 'Choose your games folder'}</button></div>
    </div>}

    {step === 1 && <div className="wizard-body">
      {only === undefined && <h3>Find your system</h3>}
      <p className="plan-notes">RetroDrive looks on your drives and for systems on your network (Batocera, Recalbox, RetroPie, ROCKNIX, Lakka). It only looks; nothing is changed.</p>
      {onDrives === null && <p className="plan-notes"><Search size={14}/> Looking on your drives…</p>}
      {found.length > 0 && <ul className="backup-list">{found.map(row)}</ul>}
      {onDrives !== null && onNetwork === null && <p className="plan-notes"><Search size={14}/> Still checking the network…</p>}
      {onDrives !== null && onNetwork !== null && found.length === 0 && <p>No system found. If it's on another machine, switch it on and connect it to your network, then <button className="link" onClick={search}>look again</button>.</p>}
      {hint && <p className="warn-text">{hint}</p>}
      <div className="actions"><button className="ghost" onClick={() => void browse()}><FolderOpen size={16}/> Browse…</button>
        {onDrives !== null && onNetwork !== null && found.length > 0 && <button className="ghost" onClick={search}><Search size={16}/> Look again</button>}
        <button className="link" onClick={onInstallHelp}>No system yet? Install Batocera or RetroPie</button></div>
      {installs.length > 0 && !found.some(f => idFor(f.path)) && <><p className="plan-notes">Or one you've added before:</p>
        <ul className="backup-list">{installs.map(i => <li key={i.id}><b>{i.adapter_name}</b> <span className="mono">{i.path}</span> {i.id === system ? <span className="ok-text"><CheckCircle2 size={14}/> Chosen</span> : <button className="link" onClick={() => setSystem(i.id)}>Use this one</button>}</li>)}</ul></>}
    </div>}

    {only !== undefined ? <div className="actions wizard-nav">
      {only === 1 && chosen && <button className="ghost" onClick={() => { close(); onPlan(chosen.id); }}>Copy games to {chosen.adapter_name}</button>}
      <button className="primary" onClick={close}>Done</button>
    </div> : <div className="actions wizard-nav">
      {step > 0 && <button className="ghost" onClick={() => setStep(step - 1)}>Back</button>}
      {step < 1 && <button className="primary" disabled={!canNext} onClick={() => setStep(step + 1)} title={canNext ? undefined : 'Add your games first'}>Next</button>}
      {step === 1 && <button className="primary" disabled={!canNext} onClick={() => { if (chosen) { close(); onPlan(chosen.id); } }} title={canNext ? undefined : 'Choose a system first'}>Next: choose what to copy to {chosen?.adapter_name ?? 'it'}</button>}
    </div>}
  </section>;
}
