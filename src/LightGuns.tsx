// Light guns (RDS-028 step 3, ADR-066): recognize a gun plugged into this PC (or let
// the user choose theirs), then a short guide for it on their systems, how far to
// sit, and a link to the light gun games in their library. Reads only, apart from
// remembering the user's gun.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { fmtSize } from './format';
import { Check, Crosshair } from 'lucide-react';
import ControllerOutline from './ControllerOutline';

type Gun = { id: string; name: string; tracking: string; distance: [number, number]|null; setup: string[]; retrobat: string[]; batocera: string[]; detectable: boolean };
type ConnectedGun = { vid: string; pid: string; product: string|null; gun: Gun|null; remembered: boolean };
type State = { connected: ConnectedGun[]; chosen: Gun|null; guns: Gun[]; gun_games: number; systems: string[]; other_steps: Record<string, Record<string, string[]>> };

const SYSTEM: Record<string, string> = { retrobat: 'RetroBat', batocera: 'Batocera', retropie: 'RetroPie', esde: 'ES-DE', recalbox: 'Recalbox' };
const COLLECTION: Record<string, string> = {
  retrobat: 'To see your light gun games together in RetroBat, turn on the light gun collection: MAIN MENU > GAME COLLECTION SETTINGS > AUTOMATIC GAME COLLECTIONS. Per game, the GUNS options include ENABLE GUN, USE ONE GUN ONLY and RELOAD WITH BUTTON.',
  batocera: 'Batocera puts light gun games in the light gun genre, and can show it as its own system.',
  retropie: 'RetroDrive can make a Light Gun Games collection on RetroPie with the light gun games it copied there: Customize → Game groups.',
  esde: 'RetroDrive can make a Light Gun Games collection in ES-DE with the light gun games it copied there: Customize → Game groups.',
};

const fmt = (n: number) => n.toFixed(1);

type SindenStatus = { installation_id: string; location: string; target: string; installed: boolean; problem: string|null };
type SindenPreview = { source: string; target: string; to_copy: number; bytes: number; already_there: number; kept: string[]; kept_count: number };

// Light guns D: copy the Sinden software the user downloaded into RetroBat's
// system/tools/sinden folder (new files only; nothing replaced).
function SindenStep({ notify }: { notify: (t: string, e?: boolean) => void }) {
  const [rows, setRows] = useState<SindenStatus[]|null>(null);
  const [p, setP] = useState<{ id: string; preview: SindenPreview }|null>(null); const [problem, setProblem] = useState('');
  const [confirm, setConfirm] = useState(false); const [working, setWorking] = useState(false); const [progress, setProgress] = useState<{ done: number; total: number }|null>(null);
  const load = async () => { try { setRows(await invoke<SindenStatus[]>('sinden_status')); } catch { setRows([]); } };
  useEffect(() => { void load(); }, []);
  useEffect(() => { const un = listen<{ done: number; total: number }>('art-progress', e => { if (working) setProgress(e.payload); }); return () => { void un.then(f => f()); }; }, [working]);
  const choose = async (id: string) => { setProblem(''); setConfirm(false);
    try { const source = await open({ directory: true, multiple: false, title: 'Select the folder with the Sinden software (Lightgun.exe)' }); if (typeof source !== 'string') return;
      setP({ id, preview: await invoke<SindenPreview>('preview_sinden_step', { installationId: id, source }) }); } catch (e) { setP(null); setProblem(String(e)); } };
  const copy = async () => { if (!p) return; if (!confirm) { setConfirm(true); return; } setWorking(true); setProgress(null);
    try { notify(await invoke<string>('apply_sinden_step', { installationId: p.id, source: p.preview.source })); setP(null); await load(); }
    catch (e) { notify(`Could not copy the Sinden software: ${String(e)}`, true); } finally { setWorking(false); setConfirm(false); setProgress(null); } };
  if (!rows || rows.length === 0) return null;
  return <div className="sinden-step"><b>Sinden software in RetroBat</b>
    <ul className="setup-list">{rows.map(r => <li key={r.installation_id} className={r.installed ? 'setup-ready' : r.problem ? 'setup-unavailable' : 'setup-add'}>
      <b>{r.installed ? '✓' : r.problem ? '–' : '＋'} {r.location}</b><br/>
      <span className="plan-notes">{r.installed ? "The Sinden software is in RetroBat's system\\tools\\sinden folder." : r.problem ?? 'Not there yet. Download the Sinden software from the Sinden website and unpack it, then choose its folder: RetroDrive copies it in for you.'}</span>
      {!r.installed && !r.problem && (!p || p.id !== r.installation_id) && <div className="actions"><button className="ghost" onClick={() => void choose(r.installation_id)}>Choose the Sinden folder…</button></div>}
      {p && p.id === r.installation_id && (p.preview.to_copy === 0
        ? <p className="plan-notes">Nothing to copy: it's already there. <button className="link" onClick={() => setP(null)}>close</button></p>
        : <div className="confirm"><p><b>Copy {p.preview.to_copy.toLocaleString()} file{p.preview.to_copy === 1 ? '' : 's'}</b> ({fmtSize(p.preview.bytes)}) to <span className="mono">{p.preview.target}</span>?</p>
          <ul>{p.preview.kept_count > 0 && <li>{p.preview.kept_count} different file(s) with the same name already there are kept{p.preview.kept.length ? ` (e.g. ${p.preview.kept.slice(0, 3).join(', ')})` : ''}.</li>}<li className="plan-notes">New files only; nothing is replaced or deleted.</li></ul>
          {working && progress && <p className="plan-notes">Copying: {progress.done.toLocaleString()} of {progress.total.toLocaleString()}…</p>}
          <div className="actions"><button className="primary" disabled={working} onClick={() => void copy()}>{working ? 'Copying…' : confirm ? 'Click again to copy' : 'Copy the Sinden software'}</button><button className="ghost" disabled={working} onClick={() => { setP(null); setConfirm(false); }}>Not now</button></div></div>)}
    </li>)}</ul>
    {problem && <p className="warn-text">{problem}</p>}
  </div>;
}

function Distance({ gun }: { gun: Gun }) {
  const [inches, setInches] = useState(55);
  if (!gun.distance) return null;
  const [lo, hi] = gun.distance;
  const m = (x: number) => inches * x * 0.0254;
  const range = (f: (x: number) => number, unit: string) => lo === hi ? `${fmt(f(lo))} ${unit}` : `${fmt(f(lo))}–${fmt(f(hi))} ${unit}`;
  return <p className="gun-distance">How far to sit from a <input type="number" min={10} max={150} value={inches} onChange={e => setInches(Math.max(10, Math.min(150, Number(e.target.value) || 55)))}/>-inch screen: <b>about {range(m, 'm')}</b> ({range(x => m(x) / 0.3048, 'ft')}). <small className="plan-notes">From Batocera's light gun guide.</small></p>;
}

function Guide({ gun, systems, gunGames, onShowGunGames, notify, other }: { gun: Gun; systems: string[]; gunGames: number; onShowGunGames: () => void; notify: (t: string, e?: boolean) => void; other: Record<string, string[]> }) {
  const known = systems.filter(s => s === 'retrobat' || s === 'batocera');
  const others = systems.filter(s => s !== 'retrobat' && s !== 'batocera' && other[s]);
  return <div className="gun-guide">
    <h4>Setting up your {gun.name}</h4>
    <Distance gun={gun}/>
    <ol>{gun.setup.map(s => <li key={s}>{s}</li>)}</ol>
    {known.map(s => <div key={s}><b>On {SYSTEM[s]}</b><ul>{(s === 'retrobat' ? gun.retrobat : gun.batocera).map(x => <li key={x}>{x}</li>)}<li>{COLLECTION[s]}</li></ul>{s === 'retrobat' && gun.id === 'sinden' && <SindenStep notify={notify}/>}</div>)}
    {others.map(s => <div key={s}><b>On {SYSTEM[s] ?? s}</b><ul>{other[s].map(x => <li key={x}>{x}</li>)}{COLLECTION[s] && <li>{COLLECTION[s]}</li>}</ul></div>)}
    {known.length === 0 && others.length === 0 && <p className="plan-notes">Add a system under Emulation Systems to see the steps for it. RetroBat and Batocera find most light guns by themselves.</p>}
    <p><button className="link" onClick={onShowGunGames}><Crosshair size={14}/> See the light gun games in your library</button> <small className="plan-notes">RetroDrive knows {gunGames.toLocaleString()} light gun games (from Batocera's game list). Pick them in the Game Library, add them to a collection, and copy that collection with Plan.</small></p>
  </div>;
}

function GunPicker({ guns, initial, onPick, onCancel, busy }: { guns: Gun[]; initial: string; onPick: (id: string) => void; onCancel?: () => void; busy: boolean }) {
  const [pick, setPick] = useState(initial);
  return <div className="gun-picker">
    <select value={pick} onChange={e => setPick(e.target.value)}>
      <option value="">Choose your light gun…</option>
      {guns.map(g => <option key={g.id} value={g.id}>{g.name}</option>)}
    </select>
    <button className="primary" disabled={busy || !pick} onClick={() => onPick(pick)}>This is mine</button>
    {onCancel && <button className="ghost" disabled={busy} onClick={onCancel}>Cancel</button>}
  </div>;
}

export default function LightGuns({ notify, onShowGunGames }: { notify: (t: string, e?: boolean) => void; onShowGunGames: () => void }) {
  const [state, setState] = useState<State|null>(null); const [busy, setBusy] = useState(false); const [changing, setChanging] = useState<string|null>(null);
  const load = async () => { try { setState(await invoke<State>('light_guns')); } catch (e) { notify(`Could not look for light guns: ${String(e)}`, true); } };
  useEffect(() => { void load(); }, []);
  const remember = async (usbId: string, gunId: string|null) => { setBusy(true);
    try { await invoke('remember_light_gun', { usbId, gunId }); setChanging(null); await load(); } catch (e) { notify(`Could not save that: ${String(e)}`, true); } finally { setBusy(false); } };
  if (!state) return null;
  const confirmed = state.connected.find(c => c.remembered)?.gun ?? state.chosen;

  return <div className="backups light-guns">
    <h3><Crosshair size={17}/> Light guns</h3>
    {state.connected.length > 0 ? <div className="controller-grid">{state.connected.map(c => { const key = `${c.vid}:${c.pid}`; return <article key={key} className="controller-card confirm-card">
      <div className="controller-head"><b>{c.product ?? 'Light gun'}</b><small className="mono">USB {key}</small></div>
      <div className="confirm-body"><div className="outline-box"><ControllerOutline typeId="CT11" label="Light gun"/></div><div>
        {changing === key ? <><p>Which light gun is it?</p><GunPicker guns={state.guns} initial={c.gun?.id ?? ''} busy={busy} onPick={id => void remember(key, id)} onCancel={() => setChanging(null)}/></>
        : c.remembered ? <p><Check size={15}/> <b>{c.gun?.name}</b> <button className="link" onClick={() => setChanging(key)}>Change</button></p>
        : <><p>We found a <b>{c.gun?.name}</b>. Is that right?</p>
          <div className="actions"><button className="primary" disabled={busy} onClick={() => c.gun && void remember(key, c.gun.id)}>Yes, that's it</button><button className="ghost" disabled={busy} onClick={() => setChanging(key)}>No, choose another</button></div></>}
      </div></div></article>; })}</div>
    : state.chosen && changing !== 'manual' ? <p><Check size={15}/> Your light gun: <b>{state.chosen.name}</b> (not plugged in right now). <button className="link" onClick={() => setChanging('manual')}>Change</button> <button className="link" onClick={() => void remember('manual', null)}>I don't have one</button></p>
    : <><p className="plan-notes">No light gun plugged in. Have one? Plug it in and choose <b>Look again</b>, or choose it here to see how to set it up.</p>
      <GunPicker guns={state.guns} initial={state.chosen?.id ?? ''} busy={busy} onPick={id => void remember('manual', id)} onCancel={state.chosen ? () => setChanging(null) : undefined}/></>}
    {confirmed && <Guide gun={confirmed} systems={state.systems} gunGames={state.gun_games} onShowGunGames={onShowGunGames} notify={notify} other={state.other_steps[confirmed.id] ?? {}}/>}
    {!confirmed && <p><button className="link" onClick={onShowGunGames}><Crosshair size={14}/> See the light gun games in your library</button></p>}
  </div>;
}
