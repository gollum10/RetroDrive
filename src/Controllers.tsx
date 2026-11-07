// Controllers workspace (RDS-028): find the controllers plugged into this PC, show an
// outline of the style RetroDrive thinks each one is, and let the user confirm it or
// pick another style (remembered per USB ID). Look-up, the type list and credits are
// opt-in below. Setting controllers up on your systems comes next.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Check, RefreshCw, Search } from 'lucide-react';
import ControllerOutline from './ControllerOutline';

type ControllerType = { id: string; name: string; layout: string; systems: string };
type Info = { vid: string; pid: string; name: string; also_known_as: string[]; controller_type: ControllerType|null; type_basis: string|null; sources: string; sdl_windows: boolean; retroarch_files: string[]; shares_usb_id: number; mixed_names: boolean };
type Connected = { vid: string; pid: string; product: string|null; info: Info|null; xinput: boolean; suggested: ControllerType|null; suggested_by: string|null };
const like = (t: ControllerType) => /^many/i.test(t.systems) ? 'works with many systems' : `like ${t.systems}`;
type Reference = { types: ControllerType[]; sources: [string, string, string, string][]; controllers: number; notices: string };

function Facts({ info }: { info: Info }) {
  return <ul className="controller-facts">
    {info.also_known_as.length > 0 && <li>Also known as {info.also_known_as.join(', ')}.</li>}
    {info.shares_usb_id > 0 && <li>{info.shares_usb_id} other device{info.shares_usb_id === 1 ? '' : 's'} in the list use{info.shares_usb_id === 1 ? 's' : ''} the same USB ID.</li>}
    <li>{info.sdl_windows ? 'SDL knows it on Windows, so RetroBat and ES-DE can use it as is.' : 'SDL has no Windows mapping for it.'}</li>
    <li>{info.retroarch_files.length ? `RetroArch has ${info.retroarch_files.length} ready-made setup file${info.retroarch_files.length === 1 ? '' : 's'} for it.` : 'RetroArch has no ready-made setup file for it.'}</li>
  </ul>;
}

// One connected controller: confirm the suggested style, or choose one from the list.
function ConfirmCard({ c, types, onSaved, notify }: { c: Connected; types: ControllerType[]; onSaved: () => void; notify: (t: string, e?: boolean) => void }) {
  const confirmed = c.suggested_by === 'remembered';
  const [choosing, setChoosing] = useState(!c.suggested);
  const [pick, setPick] = useState(c.suggested?.id ?? '');
  const [details, setDetails] = useState(false);
  const [saving, setSaving] = useState(false);
  const name = c.info?.name ?? c.product ?? 'Unknown controller';
  const shown = choosing ? types.find(t => t.id === pick) ?? null : c.suggested;
  const save = async (typeId: string|null) => { setSaving(true);
    try { await invoke('remember_controller_type', { vid: c.vid, pid: c.pid, typeId, xinput: c.xinput }); setChoosing(false); onSaved(); }
    catch (e) { notify(`Could not save that: ${String(e)}`, true); } finally { setSaving(false); } };
  const why = c.suggested_by === 'xinput' ? 'Windows uses it as an Xbox controller.' : c.suggested_by === 'name' ? 'Going by its name.' : c.suggested_by === 'list' ? 'From RetroDrive\'s controller list.' : '';

  return <article className="controller-card confirm-card">
    <div className="controller-head"><b>{name}</b><small className="mono">USB {c.vid}:{c.pid}</small></div>
    <div className="confirm-body">
      <div className="outline-box">{shown ? <ControllerOutline typeId={shown.id} label={shown.name}/> : <span className="plan-notes">Choose a style to see it</span>}</div>
      <div>
        {confirmed && !choosing ? <>
          <p><Check size={15}/> <b>{c.suggested!.name}</b>. {c.suggested!.layout}.</p>
          <p className="plan-notes">Suits {c.suggested!.systems}. <button className="link" onClick={() => { setPick(c.suggested!.id); setChoosing(true); }}>Change</button></p>
          <SetupPanel c={c} notify={notify}/>
        </> : !choosing && c.suggested ? <>
          <p>Does yours look like this? <b>{c.suggested.name}</b>: {c.suggested.layout}.</p>
          <p className="plan-notes">{why} Suits {c.suggested.systems}.</p>
          <div className="actions"><button className="primary" disabled={saving} onClick={() => void save(c.suggested!.id)}>Yes, that's it</button>
            <button className="ghost" disabled={saving} onClick={() => setChoosing(true)}>No, choose another</button></div>
        </> : <>
          <p>{c.suggested ? 'Which one looks like yours?' : "RetroDrive can't tell what kind of controller this is. Which one looks like yours?"}</p>
          <select value={pick} onChange={e => setPick(e.target.value)}>
            <option value="">Choose a style…</option>
            {types.map(t => <option key={t.id} value={t.id}>{t.name} ({like(t)})</option>)}
          </select>
          {shown && <p className="plan-notes">{shown.layout}.</p>}
          <div className="actions"><button className="primary" disabled={saving || !pick} onClick={() => void save(pick)}>This is mine</button>
            {(c.suggested || confirmed) && <button className="ghost" disabled={saving} onClick={() => setChoosing(false)}>Cancel</button>}</div>
        </>}
      </div>
    </div>
    {c.info && <><button className="link" onClick={() => setDetails(v => !v)}>{details ? 'Hide details' : 'Details'}</button>{details && <Facts info={c.info}/>}</>}
  </article>;
}

type SetupTarget = { installation_id: string; adapter_id: string; location: string; status: 'ready'|'add'|'manual'|'unavailable'; message: string; files: string[] };
const SYSTEM: Record<string, string> = { retrobat: 'RetroBat', batocera: 'Batocera', retropie: 'RetroPie', esde: 'ES-DE', recalbox: 'Recalbox' };
// The last two folders of a path (the full path is in the tooltip).
const shortPath = (p: string) => p.split(/[\\/]/).filter(Boolean).slice(-2).join('\\');
const MARK: Record<string, string> ={ ready: '✓', add: '＋', manual: '•', unavailable: '–' };

// Step 2: where this controller stands on each installation, and one button to set it up.
function SetupPanel({ c, notify }: { c: Connected; notify: (t: string, e?: boolean) => void }) {
  const [targets, setTargets] = useState<SetupTarget[]|null>(null); const [problem, setProblem] = useState('');
  const [confirm, setConfirm] = useState(false); const [working, setWorking] = useState(false);
  const check = async () => { setProblem(''); setConfirm(false);
    try { setTargets(await invoke<SetupTarget[]>('controller_setup_preview', { vid: c.vid, pid: c.pid, xinput: c.xinput })); } catch (e) { setProblem(String(e)); } };
  useEffect(() => { void check(); }, [c.vid, c.pid]);
  const adds = targets?.filter(t => t.status === 'add') ?? [];
  const files = adds.reduce((n, t) => n + t.files.length, 0);
  const apply = async () => { if (!confirm) { setConfirm(true); return; } setWorking(true);
    try { notify(await invoke<string>('apply_controller_setup', { vid: c.vid, pid: c.pid, xinput: c.xinput, installationIds: adds.map(t => t.installation_id) })); }
    catch (e) { notify(`Could not set the controller up: ${String(e)}`, true); } finally { setWorking(false); setConfirm(false); void check(); } };
  if (problem) return <p className="warn-text">{problem}</p>;
  if (!targets) return <p className="plan-notes">Checking your systems…</p>;
  if (targets.length === 0) return <p className="plan-notes">Add a system under Emulation Systems and RetroDrive can set this controller up there.</p>;
  return <div className="setup-panel">
    {adds.length > 0 && <div className="actions"><button className="primary" disabled={working} onClick={() => void apply()}>{working ? 'Setting up…' : confirm ? `Click again to add ${files} file${files === 1 ? '' : 's'}` : `Set up on ${adds.map(t => SYSTEM[t.adapter_id] ?? t.adapter_id).join(' and ')}`}</button>
      {confirm && <button className="ghost" onClick={() => setConfirm(false)}>Not now</button>}</div>}
    {confirm && <p className="plan-notes">Adds only: {adds.flatMap(t => t.files).join(', ')}. Nothing already there is changed. Undo is under Customize → Show every settings change.</p>}
    <ul className="setup-list">{targets.map(t => <li key={t.installation_id} className={`setup-${t.status}`}><b>{MARK[t.status]} {SYSTEM[t.adapter_id] ?? t.adapter_id}</b> <small className="mono" title={t.location}>{shortPath(t.location)}</small><br/><span className="plan-notes">{t.status === 'add' ? `RetroDrive can set it up. ${t.message}` : t.message}</span></li>)}</ul>
    {adds.length === 0 && <button className="link" onClick={() => void check()}>Check again</button>}
  </div>;
}

function LookupCard({ info }: { info: Info }) {
  const t = info.controller_type;
  return <article className="controller-card">
    <div className="controller-head"><b>{info.name}</b><small className="mono">{info.vid ? `USB ${info.vid}:${info.pid}` : 'USB ID not recorded'}</small></div>
    {t ? <p><span className="tag identical">{t.name}</span> {t.layout}. <span className="plan-notes">Suits {t.systems}.</span></p>
      : <p className="plan-notes">{info.mixed_names ? "Its type isn't certain: this USB ID is listed under several different controllers." : "Its type isn't clear from its name."}</p>}
    <Facts info={info}/>
  </article>;
}

export default function Controllers({ notify }: { notify: (text: string, error?: boolean) => void }) {
  const [connected, setConnected] = useState<Connected[]|null>(null); const [looking, setLooking] = useState(false);
  const [showLookup, setShowLookup] = useState(false); const [query, setQuery] = useState(''); const [found, setFound] = useState<Info[]>([]);
  const [ref, setRef] = useState<Reference|null>(null); const [showTypes, setShowTypes] = useState(false); const [showNotices, setShowNotices] = useState(false);
  const detect = async () => { setLooking(true); try { setConnected(await invoke<Connected[]>('connected_controllers')); } catch { setConnected([]); } finally { setLooking(false); } };
  useEffect(() => { void detect(); void invoke<Reference>('controller_reference').then(setRef).catch(() => {}); }, []);
  useEffect(() => { if (!showLookup) return; const t = window.setTimeout(() => { void invoke<Info[]>('find_controllers', { query }).then(setFound).catch(() => setFound([])); }, 250); return () => window.clearTimeout(t); }, [query, showLookup]);

  return <section className="panel controllers">
    <div className="section-heading"><div><h2>Controllers</h2><p>Plug in your controller and RetroDrive works out what kind it is. Confirm it once and RetroDrive remembers it, then sets it up on your systems. Light guns come next.</p></div>
      <button className="ghost" disabled={looking} onClick={() => void detect()}><RefreshCw size={16}/> {looking ? 'Looking…' : 'Look again'}</button></div>

    {connected === null ? <p className="plan-notes">Looking for controllers…</p>
    : connected.length === 0 ? <p className="plan-notes">No game controllers found. Plug one in (USB, or pair it over Bluetooth), then choose <b>Look again</b>.</p>
    : <div className="controller-grid">{connected.map((c, i) => <ConfirmCard key={`${c.vid}:${c.pid}:${i}:${c.suggested_by}:${c.suggested?.id}`} c={c} types={ref?.types ?? []} onSaved={() => void detect()} notify={notify}/>)}</div>}

    <p className="controller-more">
      <button className="link" onClick={() => setShowLookup(v => !v)}><Search size={14}/> {showLookup ? 'Hide look-up' : 'Look up a controller'}</button>
      <button className="link" onClick={() => setShowTypes(v => !v)}>{showTypes ? 'Hide' : 'Show'} the 12 controller styles</button>
    </p>
    {showLookup && <>
      <label className="controller-search"><Search size={15}/><input className="design-new" autoFocus placeholder={`Search ${ref ? ref.controllers.toLocaleString() : ''} controllers by name (or vid:pid)`} value={query} onChange={e => setQuery(e.target.value)}/></label>
      {query.trim().length >= 2 && (found.length === 0 ? <p className="plan-notes">No match.</p> : <div className="controller-grid">{found.slice(0, 12).map(c => <LookupCard key={c.vid + c.pid + c.name} info={c}/>)}</div>)}
      {found.length > 12 && <p className="plan-notes">Showing 12 of {found.length}; type more of the name to narrow it down.</p>}
    </>}
    {showTypes && ref && <div className="style-grid">{ref.types.map(t => <div key={t.id} className="style-tile"><ControllerOutline typeId={t.id} size={120} label={t.name}/><b>{t.name}</b><small className="plan-notes">{t.layout}. {like(t).replace(/^./, ch => ch.toUpperCase())}.</small></div>)}</div>}

    <div className="backups"><h3>Credits</h3>
      <p className="plan-notes">RetroDrive's controller list is compiled from these open projects. Thank you to their contributors.</p>
      <ul>{ref?.sources.map(([name, url, commit, entries]) => <li key={name}><b>{name}</b> ({Number(entries).toLocaleString()} entries): <span className="mono">{url}</span> <small className="plan-notes">version {commit.slice(0, 10)}</small></li>)}</ul>
      <button className="link" onClick={() => setShowNotices(v => !v)}>{showNotices ? 'Hide' : 'Show'} licences</button>
      {showNotices && ref && <pre className="notices">{ref.notices}</pre>}</div>
  </section>;
}
