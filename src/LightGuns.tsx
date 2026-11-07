// Light guns (RDS-028 step 3, ADR-066): recognize a gun plugged into this PC (or let
// the user choose theirs), then a short guide for it on their systems, how far to
// sit, and a link to the light gun games in their library. Reads only, apart from
// remembering the user's gun.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Check, Crosshair } from 'lucide-react';
import ControllerOutline from './ControllerOutline';

type Gun = { id: string; name: string; tracking: string; distance: [number, number]|null; setup: string[]; retrobat: string[]; batocera: string[]; detectable: boolean };
type ConnectedGun = { vid: string; pid: string; product: string|null; gun: Gun|null; remembered: boolean };
type State = { connected: ConnectedGun[]; chosen: Gun|null; guns: Gun[]; gun_games: number; systems: string[] };

const SYSTEM: Record<string, string> = { retrobat: 'RetroBat', batocera: 'Batocera', retropie: 'RetroPie', esde: 'ES-DE', recalbox: 'Recalbox' };
const COLLECTION: Record<string, string> = {
  retrobat: 'To see your light gun games together in RetroBat, turn on the light gun collection: MAIN MENU > GAME COLLECTION SETTINGS > AUTOMATIC GAME COLLECTIONS. Per game, the GUNS options include ENABLE GUN, USE ONE GUN ONLY and RELOAD WITH BUTTON.',
  batocera: 'Batocera puts light gun games in the light gun genre, and can show it as its own system.',
};

const fmt = (n: number) => n.toFixed(1);

function Distance({ gun }: { gun: Gun }) {
  const [inches, setInches] = useState(55);
  if (!gun.distance) return null;
  const [lo, hi] = gun.distance;
  const m = (x: number) => inches * x * 0.0254;
  const range = (f: (x: number) => number, unit: string) => lo === hi ? `${fmt(f(lo))} ${unit}` : `${fmt(f(lo))}–${fmt(f(hi))} ${unit}`;
  return <p className="gun-distance">How far to sit from a <input type="number" min={10} max={150} value={inches} onChange={e => setInches(Math.max(10, Math.min(150, Number(e.target.value) || 55)))}/>-inch screen: <b>about {range(m, 'm')}</b> ({range(x => m(x) / 0.3048, 'ft')}). <small className="plan-notes">From Batocera's light gun guide.</small></p>;
}

function Guide({ gun, systems, gunGames, onShowGunGames }: { gun: Gun; systems: string[]; gunGames: number; onShowGunGames: () => void }) {
  const known = systems.filter(s => s === 'retrobat' || s === 'batocera');
  const others = systems.filter(s => s !== 'retrobat' && s !== 'batocera');
  return <div className="gun-guide">
    <h4>Setting up your {gun.name}</h4>
    <Distance gun={gun}/>
    <ol>{gun.setup.map(s => <li key={s}>{s}</li>)}</ol>
    {known.map(s => <div key={s}><b>On {SYSTEM[s]}</b><ul>{(s === 'retrobat' ? gun.retrobat : gun.batocera).map(x => <li key={x}>{x}</li>)}<li>{COLLECTION[s]}</li></ul></div>)}
    {known.length === 0 && <p className="plan-notes">Add RetroBat or Batocera under Emulation Systems to see the steps for them. Both find most light guns by themselves.</p>}
    {others.length > 0 && <p className="plan-notes">RetroDrive doesn't have light gun steps for {others.map(s => SYSTEM[s] ?? s).join(', ')} yet; check that system's own guide.</p>}
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
    {confirmed && <Guide gun={confirmed} systems={state.systems} gunGames={state.gun_games} onShowGunGames={onShowGunGames}/>}
    {!confirmed && <p><button className="link" onClick={onShowGunGames}><Crosshair size={14}/> See the light gun games in your library</button></p>}
  </div>;
}
