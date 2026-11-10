// Controller and light gun wizard (owner 2026-11-09, ADR-102): one simple wizard that
// starts with "Controller or light gun?". Controllers: find it (confirm the style
// RetroDrive recognised), then set it up on your systems. Light guns: find it (plugged
// in, or choose it), set it up (the guide for each system, the Sinden helper), then
// the light gun games (a smart collection, and a hand-over to Copy games). Every action
// is an existing feature; optional, Exit any time.
import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { CheckCircle2, Crosshair, Gamepad2, RefreshCw, Wand2, X } from 'lucide-react';
import { ConfirmCard, SetupPanel, type Connected, type ControllerType } from './Controllers';
import { GunChoice, Guide, confirmedGun, type GunState } from './LightGuns';

type Kind = 'controller'|'gun';
const STEPS: Record<Kind, string[]> = {
  controller: ['Controller or light gun?', 'Find your controller', 'Set it up'],
  gun: ['Controller or light gun?', 'Find your light gun', 'Set it up', 'Light gun games'],
};
type Collection = { id: string; name: string; games: number; kind: string; rule: Record<string, unknown>|null };
/** A smart collection whose only filter is "light gun games". */
const gunCollection = (c: Collection) => c.kind === 'smart' && !!c.rule && c.rule.lightgun_only === true
  && Object.entries(c.rule).every(([k, v]) => k === 'lightgun_only' || v === null || v === false || v === '' || v === 0 || k === 'sort' || k === 'offset' || k === 'limit');
const GUN_COLLECTION = 'Light gun games';
const n = (x: number, one: string, many = `${one}s`) => `${x.toLocaleString()} ${x === 1 ? one : many}`;

export default function ControllerWizard({ systems, onCopyCollection, onShowGunGames, onClose, notify }: {
  /** How many systems are set up (Copy needs one). */ systems: number;
  /** Open Copy games with this collection chosen. */ onCopyCollection: (collectionId: string) => void;
  onShowGunGames: () => void; onClose: () => void; notify: (text: string, error?: boolean) => void;
}) {
  const [kind, setKind] = useState<Kind|null>(null);
  const [at, setAt] = useState(0);
  const top = useRef<HTMLElement>(null);
  useEffect(() => { top.current?.scrollIntoView({ behavior: 'smooth', block: 'start' }); }, [at]);
  // Controllers
  const [pads, setPads] = useState<Connected[]|null>(null); const [types, setTypes] = useState<ControllerType[]>([]); const [looking, setLooking] = useState(false);
  const findPads = async () => { setLooking(true); try { setPads(await invoke<Connected[]>('connected_controllers')); } catch { setPads([]); } finally { setLooking(false); } };
  // Light guns
  const [guns, setGuns] = useState<GunState|null>(null); const [busy, setBusy] = useState(false); const [changing, setChanging] = useState<string|null>(null);
  const findGuns = async () => { setLooking(true); try { setGuns(await invoke<GunState>('light_guns')); } catch (e) { notify(`Could not look for light guns: ${String(e)}`, true); } finally { setLooking(false); } };
  const remember = async (usbId: string, gunId: string|null) => { setBusy(true);
    try { await invoke('remember_light_gun', { usbId, gunId }); setChanging(null); await findGuns(); } catch (e) { notify(`Could not save that: ${String(e)}`, true); } finally { setBusy(false); } };
  // Light gun games
  const [inLibrary, setInLibrary] = useState<number|null>(null); const [collection, setCollection] = useState<Collection|null>(null); const [making, setMaking] = useState(false);
  const loadGames = async () => {
    try { const page = await invoke<{ total: number }>('query_games', { query: { lightgun_only: true, limit: 1 } }); setInLibrary(page.total); } catch { setInLibrary(0); }
    try { setCollection((await invoke<Collection[]>('list_collections')).find(gunCollection) ?? null); } catch { setCollection(null); }
  };

  useEffect(() => {
    if (kind === 'controller' && at === 1) { void findPads(); if (types.length === 0) void invoke<{ types: ControllerType[] }>('controller_reference').then(r => setTypes(r.types)).catch(() => {}); }
    if (kind === 'gun' && at === 1 && !guns) void findGuns();
    if (kind === 'gun' && at === 3) void loadGames();
  }, [kind, at]);

  const makeCollection = async () => { setMaking(true);
    try {
      const names = (await invoke<Collection[]>('list_collections')).map(c => c.name.toLowerCase());
      let name = GUN_COLLECTION; for (let i = 2; names.includes(name.toLowerCase()); i++) name = `${GUN_COLLECTION} ${i}`;
      await invoke('create_smart_collection', { name, query: { lightgun_only: true } });
      await loadGames();
    } catch (e) { notify(`Could not make the collection: ${String(e)}`, true); } finally { setMaking(false); } };

  const steps = kind ? STEPS[kind] : [STEPS.gun[0]];
  const last = at === steps.length - 1;
  const ready = pads?.filter(p => p.suggested_by === 'remembered') ?? [];
  const gun = guns ? confirmedGun(guns) : null;
  const canNext = at === 0 ? kind !== null : kind === 'controller' ? ready.length > 0 : !!gun;
  const blockedWhy = at === 0 ? undefined : kind === 'controller' ? 'Confirm your controller first' : 'Choose your light gun first';
  const choose = (k: Kind) => { setKind(k); setAt(1); };

  return <section className="panel wizard" ref={top}>
    <div className="designs-head"><h2><Wand2 size={18}/> Controllers and light guns</h2><button className="ghost" onClick={onClose} title="Leave the wizard"><X size={16}/> Exit</button></div>
    {kind && <ol className="wizard-steps">{steps.map((s, i) => <li key={s} className={i === at ? 'current' : i < at ? 'done' : ''}>{i < at ? <CheckCircle2 size={14}/> : <span>{i + 1}</span>} {s}</li>)}</ol>}

    {at === 0 && <div className="wizard-body">
      <p>What would you like to set up?</p>
      <div className="task-grid">
        <button className="task" onClick={() => choose('controller')}><Gamepad2 size={18}/><b>A controller</b><small>A gamepad or arcade stick: RetroDrive recognises it and sets it up on your systems</small></button>
        <button className="task" onClick={() => choose('gun')}><Crosshair size={18}/><b>A light gun</b><small>Find your gun, set it up, then play the light gun games</small></button>
      </div>
    </div>}

    {kind === 'controller' && at === 1 && <div className="wizard-body">
      {pads === null ? <p className="plan-notes">Looking for controllers…</p>
      : pads.length === 0 ? <p>No controller found. Plug it in (USB, or pair it over Bluetooth), then choose <b>Look again</b>.</p>
      : <><p>{pads.length === 1 ? 'Is this your controller? Confirm it once and RetroDrive remembers it.' : 'RetroDrive found these controllers. Confirm yours; RetroDrive remembers it.'}</p>
        <div className="controller-grid">{pads.map((c, i) => <ConfirmCard key={`${c.vid}:${c.pid}:${i}:${c.suggested_by}:${c.suggested?.id}`} c={c} types={types} setup={false} onSaved={() => void findPads()} notify={notify}/>)}</div></>}
      <div className="actions"><button className="ghost" disabled={looking} onClick={() => void findPads()}><RefreshCw size={16}/> {looking ? 'Looking…' : 'Look again'}</button></div>
    </div>}

    {kind === 'controller' && at === 2 && <div className="wizard-body">
      {ready.map((c, i) => <div key={`${c.vid}:${c.pid}:${i}`}><p><b>{c.info?.name ?? c.product ?? 'Your controller'}</b> ({c.suggested?.name})</p><SetupPanel c={c} notify={notify}/></div>)}
      {last && <p className="plan-notes">That's it. Change or set up another controller any time on the <b>Controllers</b> page.</p>}
    </div>}

    {kind === 'gun' && at === 1 && <div className="wizard-body">
      {!guns ? <p className="plan-notes">Looking for a light gun…</p> : <GunChoice state={guns} busy={busy} changing={changing} setChanging={setChanging} remember={(u, g) => void remember(u, g)}/>}
      <div className="actions"><button className="ghost" disabled={looking} onClick={() => void findGuns()}><RefreshCw size={16}/> {looking ? 'Looking…' : 'Look again'}</button></div>
    </div>}

    {kind === 'gun' && at === 2 && guns && gun && <div className="wizard-body">
      <Guide gun={gun} systems={guns.systems} gunGames={guns.gun_games} notify={notify} other={guns.other_steps[gun.id] ?? {}}/>
    </div>}

    {kind === 'gun' && at === 3 && <div className="wizard-body">
      {inLibrary === null ? <p className="plan-notes">Looking for light gun games in your library…</p>
      : inLibrary === 0 ? <p>No light gun games in your library yet. RetroDrive recognises {guns ? n(guns.gun_games, 'light gun game') : 'them'} by name (from Batocera's game list); add the folder with yours under <b>Add your games</b>.</p>
      : <>
        <p>Your library has <b>{n(inLibrary, 'light gun game')}</b>.</p>
        {collection
          ? <p><CheckCircle2 size={15} className="ok-text"/> Your collection <b>{collection.name}</b> has {n(collection.games, 'game')}. It fills itself: light gun games you add later join it.</p>
          : <p>Put them together in a collection: <button className="primary" disabled={making} onClick={() => void makeCollection()}>{making ? 'Making it…' : `Make a "${GUN_COLLECTION}" collection`}</button> <small className="plan-notes">It fills itself, so light gun games you add later join it. Nothing is copied or changed.</small></p>}
        <div className="actions">
          {collection && <button className={systems > 0 ? 'primary' : 'ghost'} disabled={systems === 0} title={systems === 0 ? 'Add a system first' : undefined} onClick={() => onCopyCollection(collection.id)}>Copy them to a system</button>}
          <button className="ghost" onClick={onShowGunGames}><Crosshair size={16}/> See them in the library</button>
        </div>
      </>}
    </div>}

    <div className="actions wizard-nav">
      {at > 0 && <button className="ghost" onClick={() => setAt(at - 1)}>Back</button>}
      {at > 0 && !last && <button className="primary" disabled={!canNext} title={canNext ? undefined : blockedWhy} onClick={() => setAt(at + 1)}>Next: {steps[at + 1]}</button>}
      {last && at > 0 && <button className="primary" onClick={onClose}>Done</button>}
    </div>
  </section>;
}
