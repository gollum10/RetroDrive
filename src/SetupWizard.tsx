// Setup wizards (owner 2026-11-09; ADR-100, ADR-101): walk a user through setting up an
// emulation system one step at a time, reusing the existing features. "Set up
// everything" runs every step; each task card on the Dashboard runs only its steps
// (e.g. Copy games = system, what to copy, copy). Always optional: Exit any time.
//
// Steps: Your games (scan a folder), Find your system (search drives and network names,
// read only; the user picks), What to copy (platforms or a collection; the usual plan),
// BIOS (the BIOS checker, and the BIOS step where a system keeps BIOS elsewhere), Copy
// (the usual checked copy, never overwriting), Back up and check (first backup and the
// health check).
import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { CheckCircle2, FolderOpen, Search, Wand2, X, AlertTriangle } from 'lucide-react';
import type { DeploymentPlan } from './App';
import BiosCheck from './BiosCheck';
import BiosStep from './BiosStep';
import SystemBackup, { BACKUP_ADAPTERS } from './SystemBackup';
import HealthPanel from './HealthPanel';
import { fmtSize } from './format';

type Found = { path: string; adapter_id: string; name: string; version: string|null; added: boolean };
export type WizardInstall = { id: string; adapter_id: string; adapter_name: string; path: string; presence: string; arcade_system: string|null };
type Inspection = { path: string; best: { adapter_id: string; display_name: string } | null; detections: { hint: string|null }[] };
type Collection = { id: string; name: string };
export type WizardCopy = { running: boolean; progress: { games_done: number; games_total: number; bytes_done: number; bytes_total: number; current: string } | null; result: { status: string; games_copied: number; games_already_there: number; files_failed: number } | null; error: string | null };

export const STEPS = ['Your games', 'Find your system', 'What to copy', 'BIOS', 'Copy', 'Back up and check'];
/** The task cards: which steps each runs. */
export const TASKS: Record<string, { title: string; steps: number[] }> = {
  everything: { title: 'Set up everything', steps: [0, 1, 2, 3, 4, 5] },
  games: { title: 'Add your games', steps: [0] },
  system: { title: 'Find or add a system', steps: [1] },
  copy: { title: 'Copy games to a system', steps: [1, 2, 4] },
  bios: { title: 'Check BIOS files', steps: [1, 3] },
  backup: { title: 'Back up and check a system', steps: [1, 5] },
};
const STEP_KEY = 'rd.wizardStep';
const SYSTEM_KEY = 'rd.wizardSystem';
const BIOS_ELSEWHERE = (i: WizardInstall) => (i.adapter_id === 'retropie' && i.path.startsWith('\\')) || i.adapter_id === 'esde' || i.adapter_id === 'lakka';
const load = (k: string) => { try { return localStorage.getItem(k); } catch { return null; } };
const save = (k: string, v: string|null) => { try { if (v === null) localStorage.removeItem(k); else localStorage.setItem(k, v); } catch { /* not remembered */ } };
const same = (a: string, b: string) => a.replace(/[\\/]+$/, '').toLowerCase() === b.replace(/[\\/]+$/, '').toLowerCase();
const n = (x: number, one: string, many = `${one}s`) => `${x.toLocaleString()} ${x === 1 ? one : many}`;

export default function SetupWizard({ task, collection: startCollection, firstRun, games, scanning, busy, installs, copy, onScan, onAdded, onSetArcade, onStartCopy, onOpenPlan, onHealthAction, onInstallHelp, onClose, notify }: {
  task: string; /** Copy games: start with this collection chosen. */ collection?: string; firstRun: boolean; games: number; scanning: boolean; busy: boolean; installs: WizardInstall[]; copy: WizardCopy;
  onScan: () => void; onAdded: () => Promise<void>; onSetArcade: (installationId: string, system: string) => Promise<void>;
  onStartCopy: (plan: DeploymentPlan, collectionId: string|null) => Promise<void>; onOpenPlan: (installationId: string) => void;
  /** A health-check link other than Back up (Sync, Plan, BIOS, backup folder). */ onHealthAction: (installationId: string, action: string) => void;
  onInstallHelp: () => void; onClose: () => void; notify: (text: string, error?: boolean) => void;
}) {
  const t = TASKS[task] ?? TASKS.everything;
  const full = task === 'everything';
  const [at, setAtRaw] = useState(() => full ? Math.min(Number(load(STEP_KEY) ?? 0) || 0, t.steps.length - 1) : 0);
  const setAt = (i: number) => { setAtRaw(i); if (full) save(STEP_KEY, String(i)); };
  const step = t.steps[at];
  const [system, setSystemRaw] = useState<string|null>(() => load(SYSTEM_KEY) ?? (installs.length === 1 ? installs[0].id : null));
  const setSystem = (id: string|null) => { setSystemRaw(id); save(SYSTEM_KEY, id); };
  const chosen = installs.find(i => i.id === system);
  // Find your system
  const [onDrives, setOnDrives] = useState<Found[]|null>(null);
  const [onNetwork, setOnNetwork] = useState<Found[]|null>(null);
  const [adding, setAdding] = useState<string|null>(null);
  const [hint, setHint] = useState<string|null>(null);
  // What to copy
  const [plan, setPlan] = useState<DeploymentPlan|null>(null);
  const [planning, setPlanning] = useState(false);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [collection, setCollection] = useState(startCollection ?? '');
  const [confirm, setConfirm] = useState(false);
  // Only a copy started from this wizard is shown here.
  const [started, setStarted] = useState(false);
  const top = useRef<HTMLElement>(null);
  const backupBox = useRef<HTMLDivElement>(null);
  // Opened (or moved on) from further down the page: bring the wizard into view.
  useEffect(() => { top.current?.scrollIntoView({ behavior: 'smooth', block: 'start' }); }, [at]);

  // A task that needs a system skips "Find your system" when there's only one.
  useEffect(() => { if (!full && step === 1 && t.steps.length > 1 && installs.length === 1) { setSystem(installs[0].id); setAt(at + 1); } }, []);
  useEffect(() => { if (system && !installs.some(i => i.id === system)) setSystem(null); }, [installs]);
  useEffect(() => { if (step === 1 && onDrives === null) search(); }, [step]);
  useEffect(() => { if (step === 2 && chosen) { void invoke<Collection[]>('list_collections').then(setCollections).catch(() => setCollections([])); if (!plan || plan.installation_id !== chosen.id) void replan([], collection); } }, [step, system]);

  const search = () => {
    setOnDrives(null); setOnNetwork(null); setHint(null);
    void invoke<Found[]>('find_systems_on_drives').then(setOnDrives).catch(e => { notify(String(e), true); setOnDrives([]); });
    void invoke<Found[]>('find_systems_on_network').then(setOnNetwork).catch(() => setOnNetwork([]));
  };
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
  const replan = async (systems: string[], collectionId: string) => {
    if (!chosen) return;
    setPlanning(true); setConfirm(false);
    try {
      const choices = await invoke('saved_choices', { installationId: chosen.id }).catch(() => ({}));
      setPlan(await invoke<DeploymentPlan>('deployment_plan', { installationId: chosen.id, systems: collectionId ? null : systems, choices, collectionId: collectionId || null }));
    } catch (e) { notify(`Couldn't work out what to copy: ${String(e)}`, true); } finally { setPlanning(false); }
  };
  const selected = plan ? plan.systems.filter(x => x.selected).map(x => x.system_folder) : [];
  const toggle = (folder: string) => void replan(selected.includes(folder) ? selected.filter(x => x !== folder) : [...selected, folder], collection);
  const startCopy = async () => {
    if (!plan) return;
    if (!confirm) { setConfirm(true); return; }
    setConfirm(false);
    try { await onStartCopy(plan, collection || null); setStarted(true); } catch (e) { notify(String(e), true); }
  };
  const close = () => { if (full) save(STEP_KEY, null); onClose(); };

  const toCopy = plan ? plan.games.copy ?? 0 : 0;
  const arcadeWaiting = !!plan && !!chosen && !chosen.arcade_system && plan.unsupported.some(u => /arcade/i.test(u.platform));
  const fits = plan?.capacity.fits !== false;
  const running = started && copy.running;
  const copied = started && !!copy.result && !copy.running;
  const canNext = step === 0 ? games > 0 : step === 1 ? !!chosen : step === 2 ? toCopy > 0 && fits : step === 4 ? copied || (plan !== null && toCopy === 0) : true;
  const blockedWhy = step === 0 ? 'Add your games first' : step === 1 ? 'Choose a system first' : step === 2 ? (fits ? 'Tick at least one platform with games to copy' : 'Not enough space on the drive') : step === 4 ? 'Copy first (or go Back)' : undefined;
  const last = at === t.steps.length - 1;
  const found = [...(onDrives ?? []), ...(onNetwork ?? [])];

  const row = (f: Found) => {
    const id = idFor(f.path);
    return <li key={f.path} className={id && id === system ? 'chosen' : ''}>
      <b>{f.name}</b>{f.version ? ` ${f.version}` : ''} <span className="mono">{f.path}</span>{' '}
      {id ? (id === system ? <span className="ok-text"><CheckCircle2 size={14}/> Chosen</span> : <button className="link" onClick={() => setSystem(id)}>Use this one</button>)
        : <button className="link" disabled={adding !== null} onClick={() => void add(f)}>{adding === f.path ? 'Adding…' : 'Add and use'}</button>}
    </li>;
  };

  return <section className="panel wizard" ref={top}>
    <div className="designs-head"><h2><Wand2 size={18}/> {t.title}{chosen && step > 1 ? `: ${chosen.adapter_name}` : ''}</h2><button className="ghost" onClick={close} title="Leave the wizard; your choices are kept"><X size={16}/> Exit</button></div>
    {firstRun && at === 0 && <p className="plan-notes">New to RetroDrive? This walks you through setting up your first system, step by step. It's optional: you can leave any time, do everything yourself instead, and come back any time from <b>Quick actions</b> on the Dashboard. <button className="link" onClick={close}>No thanks, I'll do it myself</button></p>}
    {t.steps.length > 1 && <ol className="wizard-steps">{t.steps.map((s, i) => <li key={s} className={i === at ? 'current' : i < at ? 'done' : ''}>{i < at ? <CheckCircle2 size={14}/> : <span>{i + 1}</span>} {STEPS[s]}</li>)}</ol>}

    {step === 0 && <div className="wizard-body">
      {games > 0
        ? <p><CheckCircle2 size={15} className="ok-text"/> Your library has <b>{games.toLocaleString()}</b> game files. You can add another folder, or go on.</p>
        : <p>First, show RetroDrive where your games are: a folder on this PC, a drive or a network share. RetroDrive lists them; it doesn't copy, move or change anything.</p>}
      <div className="actions"><button className={games > 0 ? 'ghost' : 'primary'} disabled={scanning} onClick={onScan}><FolderOpen size={16}/> {scanning ? 'Scanning… (you can wait here)' : games > 0 ? 'Add another folder' : 'Choose your games folder'}</button></div>
    </div>}

    {step === 1 && <div className="wizard-body">
      <p className="plan-notes">{t.steps.length > 1 && !full ? 'Which system? ' : ''}RetroDrive looks on your drives and for systems on your network (Batocera, Recalbox, RetroPie, ROCKNIX, Lakka). It only looks; nothing is changed.</p>
      {onDrives === null && <p className="plan-notes"><Search size={14}/> Looking on your drives…</p>}
      {found.length > 0 && <ul className="backup-list">{found.map(row)}</ul>}
      {onDrives !== null && onNetwork === null && <p className="plan-notes"><Search size={14}/> Still checking the network…</p>}
      {onDrives !== null && onNetwork !== null && found.length === 0 && <p>No system found. If it's on another machine, switch it on and connect it to your network, then <button className="link" onClick={search}>look again</button>.</p>}
      {hint && <p className="warn-text">{hint}</p>}
      <div className="actions"><button className="ghost" onClick={() => void browse()}><FolderOpen size={16}/> Browse…</button>
        {onDrives !== null && onNetwork !== null && found.length > 0 && <button className="ghost" onClick={search}><Search size={16}/> Look again</button>}
        <button className="link" onClick={onInstallHelp}>No system yet? Install Batocera or RetroPie</button></div>
      {installs.some(i => !found.some(f => same(f.path, i.path))) && <><p className="plan-notes">Added before:</p>
        <ul className="backup-list">{installs.filter(i => !found.some(f => same(f.path, i.path))).map(i => <li key={i.id}><b>{i.adapter_name}</b> <span className="mono">{i.path}</span> {i.id === system ? <span className="ok-text"><CheckCircle2 size={14}/> Chosen</span> : <button className="link" onClick={() => setSystem(i.id)}>Use this one</button>}</li>)}</ul></>}
    </div>}

    {step === 2 && chosen && <div className="wizard-body">
      {chosen.presence !== 'connected' ? <p className="warn-text"><AlertTriangle size={15}/> {chosen.adapter_name} at <span className="mono">{chosen.path}</span> isn't connected. Connect it, then go Back and Next.</p> : !plan ? <p className="plan-notes">Looking at your library and {chosen.adapter_name}…</p> : <>
        <p>Tick the platforms to put on {chosen.adapter_name}{collections.length > 0 ? ', or pick one of your collections' : ''}. Games already there are skipped.</p>
        {collections.length > 0 && <p><label>Collection: <select value={collection} onChange={e => { setCollection(e.target.value); void replan(e.target.value ? [] : selected, e.target.value); }}><option value="">None: choose platforms</option>{collections.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}</select></label></p>}
        {!collection && <ul className="wizard-systems">{plan.systems.map(x => <li key={x.system_folder}><label><input type="checkbox" checked={x.selected} disabled={planning} onChange={() => toggle(x.system_folder)}/> <b>{x.platforms.join(', ') || x.system_folder}</b> <small>{n(x.games, 'game')}, {fmtSize(x.bytes)}</small></label></li>)}</ul>}
        {arcadeWaiting && <p className="warn-text"><AlertTriangle size={15}/> Arcade games need a choice first: which emulator should they go to? <button className="link" onClick={() => void onSetArcade(chosen.id, 'mame').then(() => replan(selected, collection))}>MAME</button> · <button className="link" onClick={() => void onSetArcade(chosen.id, 'fbneo').then(() => replan(selected, collection))}>FinalBurn Neo</button> <small>(not sure? MAME)</small></p>}
        {(selected.length > 0 || collection) && <div className="confirm">
          {planning ? <p className="plan-notes">Working it out…</p> : <>
            <p><b>{n(toCopy, 'game')}</b> to copy ({fmtSize(plan.bytes_to_copy)}){(plan.games.already_there ?? 0) > 0 ? `, ${n(plan.games.already_there ?? 0, 'game')} already there` : ''}.</p>
            {plan.capacity.measured && <p className={fits ? 'plan-notes' : 'warn-text'}>{fits ? `Fits: ${fmtSize(plan.capacity.free_bytes)} free on the drive.` : `Doesn't fit: ${fmtSize(plan.capacity.required_bytes)} needed, ${fmtSize(plan.capacity.free_bytes)} free. Untick a platform.`}</p>}
            {plan.attention.length > 0 && <p className="plan-notes">{n(plan.attention.length, 'game')} on the drive {plan.attention.length === 1 ? 'is' : 'are'} different from your library: the drive's {plan.attention.length === 1 ? 'copy is' : 'copies are'} kept. <button className="link" onClick={() => { close(); onOpenPlan(chosen.id); }}>Review in the full plan</button></p>}
            {!plan.copy_supported && <p className="warn-text">RetroDrive can only preview copies to {chosen.adapter_name} so far.</p>}
          </>}</div>}
      </>}
    </div>}

    {step === 3 && chosen && <div className="wizard-body">
      <BiosCheck installationId={chosen.id} sysName={chosen.adapter_name} notify={notify}/>
      {BIOS_ELSEWHERE(chosen) && chosen.presence === 'connected' && <BiosStep installationId={chosen.id} adapterId={chosen.adapter_id} disabled={busy} notify={notify}/>}
    </div>}

    {step === 4 && chosen && <div className="wizard-body">
      {running ? <>
        <p>Copying to {chosen.adapter_name}… each file is checked before it gets its real name.</p>
        {copy.progress && <><div className="bar"><div style={{ width: `${copy.progress.bytes_total ? 100 * copy.progress.bytes_done / copy.progress.bytes_total : 0}%` }}/></div>
          <p className="plan-notes">{copy.progress.games_done.toLocaleString()} of {n(copy.progress.games_total, 'game')} · {fmtSize(copy.progress.bytes_done)} of {fmtSize(copy.progress.bytes_total)}<br/><span className="mono">{copy.progress.current}</span></p></>}
        <p className="plan-notes">You can keep using RetroDrive; cancel from the Emulation Systems page.</p>
      </> : copied && copy.result ? <>
        <p>{copy.result.status === 'completed' ? <CheckCircle2 size={15} className="ok-text"/> : <AlertTriangle size={15} className="warn-text"/>} Copied {n(copy.result.games_copied, 'game')}{copy.result.games_already_there ? `; ${n(copy.result.games_already_there, 'game')} already there` : ''}.{copy.result.files_failed ? ` ${n(copy.result.files_failed, 'file')} couldn't be copied (see Emulation Systems).` : ''}</p>
      </> : started && copy.error ? <p className="warn-text">{copy.error}</p> : !plan ? <p>Choose what to copy first. <button className="link" onClick={() => setAt(t.steps.indexOf(2))}>Back to What to copy</button></p> : toCopy === 0 ? <p><CheckCircle2 size={15} className="ok-text"/> Nothing to copy: those games are already on {chosen.adapter_name}.</p> : <>
        <p>Ready to copy <b>{n(toCopy, 'game')}</b> ({fmtSize(plan.bytes_to_copy)}) to <span className="mono">{plan.installation_path}</span>. Existing files are never overwritten or deleted.</p>
        <div className="actions"><button className="primary" disabled={busy || !plan.copy_supported} onClick={() => void startCopy()} onBlur={() => setConfirm(false)}>{confirm ? 'Click again to start copying' : `Copy ${n(toCopy, 'game')}`}</button></div>
      </>}
    </div>}

    {step === 5 && chosen && <div className="wizard-body">
      <div ref={backupBox}>{BACKUP_ADAPTERS.includes(chosen.adapter_id) ? <SystemBackup installationId={chosen.id} sysName={chosen.adapter_name} disabled={busy || chosen.presence !== 'connected'} notify={notify}/> : <p className="plan-notes">Backups aren't available for {chosen.adapter_name} yet.</p>}</div>
      <HealthPanel installationId={chosen.id} disabled={busy || chosen.presence !== 'connected'} notify={notify} onAction={a => { if (a === 'backup') backupBox.current?.scrollIntoView({ behavior: 'smooth', block: 'center' }); else { close(); onHealthAction(chosen.id, a); } }}/>
      {last && full && <p><CheckCircle2 size={15} className="ok-text"/> <b>{chosen.adapter_name} is set up.</b> Use <b>Sync</b> on the Emulation Systems page to keep it up to date as you add games.</p>}
    </div>}

    <div className="actions wizard-nav">
      {at > 0 && <button className="ghost" disabled={running} onClick={() => setAt(at - 1)}>Back</button>}
      {!last && <button className="primary" disabled={!canNext} title={canNext ? undefined : blockedWhy} onClick={() => setAt(at + 1)}>Next: {STEPS[t.steps[at + 1]]}</button>}
      {last && <>{step === 1 && chosen && <button className="ghost" onClick={() => { close(); onOpenPlan(chosen.id); }}>Copy games to {chosen.adapter_name}</button>}<button className="primary" disabled={running} onClick={close}>Done</button></>}
    </div>
  </section>;
}
