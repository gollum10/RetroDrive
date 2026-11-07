// Controllers workspace, step 1 (RDS-028): name the controllers plugged into this PC,
// say what kind they are and which systems suit them, look any controller up, and
// credit the data sources. Reads only; setting controllers up on your systems comes next.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RefreshCw, Search } from 'lucide-react';

type ControllerType = { id: string; name: string; layout: string; systems: string };
type Info = { vid: string; pid: string; name: string; also_known_as: string[]; controller_type: ControllerType|null; type_basis: string|null; sources: string; sdl_windows: boolean; retroarch_files: string[]; shares_usb_id: number; mixed_names: boolean };
type Connected = { vid: string; pid: string; product: string|null; info: Info|null };
type Reference = { types: ControllerType[]; sources: [string, string, string, string][]; controllers: number; notices: string };

function Card({ info, product, vid, pid }: { info: Info|null; product?: string|null; vid: string; pid: string }) {
  const t = info?.controller_type;
  return <article className="controller-card">
    <div className="controller-head"><b>{info?.name ?? product ?? 'Unknown controller'}</b><small className="mono">{vid ? `USB ${vid}:${pid}` : "USB ID not recorded"}</small></div>
    {info?.also_known_as.length ? <small className="plan-notes">Also known as {info.also_known_as.join(', ')}</small> : null}
    {info && info.shares_usb_id > 0 && <small className="plan-notes">{info.shares_usb_id} other device{info.shares_usb_id === 1 ? '' : 's'} in the list use{info.shares_usb_id === 1 ? 's' : ''} the same USB ID, so this is the closest match.</small>}
    {t ? <p><span className="tag identical">{t.name}</span> {t.layout}. <span className="plan-notes">Suits {t.systems}.</span></p>
      : <p className="plan-notes">{info?.mixed_names ? "Its type isn't certain: this USB ID is listed under several different controllers." : info ? "Its type isn't clear from its name." : "This controller isn't in RetroDrive's list yet."}{product && !info ? ` Windows calls it "${product}".` : ''}</p>}
    {info && <ul className="controller-facts">
      <li>{info.sdl_windows ? 'SDL knows it on Windows, so RetroBat and ES-DE can use it as is.' : 'SDL has no Windows mapping for it.'}</li>
      <li>{info.retroarch_files.length ? `RetroArch has ${info.retroarch_files.length} ready-made setup file${info.retroarch_files.length === 1 ? '' : 's'} for it.` : 'RetroArch has no ready-made setup file for it.'}</li>
    </ul>}
  </article>;
}

export default function Controllers() {
  const [connected, setConnected] = useState<Connected[]|null>(null); const [looking, setLooking] = useState(false);
  const [query, setQuery] = useState(''); const [found, setFound] = useState<Info[]>([]);
  const [ref, setRef] = useState<Reference|null>(null); const [showTypes, setShowTypes] = useState(false); const [showNotices, setShowNotices] = useState(false);
  const detect = async () => { setLooking(true); try { setConnected(await invoke<Connected[]>('connected_controllers')); } catch { setConnected([]); } finally { setLooking(false); } };
  useEffect(() => { void detect(); void invoke<Reference>('controller_reference').then(setRef).catch(() => {}); }, []);
  useEffect(() => { const t = window.setTimeout(() => { void invoke<Info[]>('find_controllers', { query }).then(setFound).catch(() => setFound([])); }, 250); return () => window.clearTimeout(t); }, [query]);

  return <section className="panel controllers">
    <div className="section-heading"><div><h2>Controllers</h2><p>Plug a controller into this PC and RetroDrive tells you what it is, what kind of controller it is and which systems it suits. Setting controllers up on your systems (and light guns) comes next.</p></div>
      <button className="ghost" disabled={looking} onClick={() => void detect()}><RefreshCw size={16}/> {looking ? 'Looking…' : 'Look again'}</button></div>

    <h3>Connected to this PC</h3>
    {connected === null ? <p className="plan-notes">Looking for controllers…</p>
    : connected.length === 0 ? <p className="plan-notes">No game controllers found. Plug one in (USB, or pair it over Bluetooth), then choose <b>Look again</b>.</p>
    : <div className="controller-grid">{connected.map((c, i) => <Card key={`${c.vid}:${c.pid}:${i}`} info={c.info} product={c.product} vid={c.vid} pid={c.pid}/>)}</div>}

    <h3>Look up a controller</h3>
    <label className="controller-search"><Search size={15}/><input className="design-new" placeholder={`Search ${ref ? ref.controllers.toLocaleString() : ''} controllers by name (or vid:pid)`} value={query} onChange={e => setQuery(e.target.value)}/></label>
    {query.trim().length >= 2 && (found.length === 0 ? <p className="plan-notes">No match.</p> : <div className="controller-grid">{found.slice(0, 12).map(c => <Card key={c.vid + c.pid} info={c} vid={c.vid} pid={c.pid}/>)}</div>)}
    {found.length > 12 && <p className="plan-notes">Showing 12 of {found.length}; type more of the name to narrow it down.</p>}

    <h3><button className="link" onClick={() => setShowTypes(v => !v)}>{showTypes ? 'Hide' : 'Show'} the 12 controller types</button></h3>
    {showTypes && ref && <div className="table-wrap"><table><thead><tr><th>Type</th><th>Layout</th><th>Systems</th></tr></thead><tbody>{ref.types.map(t => <tr key={t.id}><td><b>{t.name}</b></td><td>{t.layout}</td><td>{t.systems}</td></tr>)}</tbody></table></div>}

    <div className="backups"><h3>Credits</h3>
      <p className="plan-notes">RetroDrive's controller list is compiled from these open projects. Thank you to their contributors.</p>
      <ul>{ref?.sources.map(([name, url, commit, entries]) => <li key={name}><b>{name}</b> ({Number(entries).toLocaleString()} entries): <span className="mono">{url}</span> <small className="plan-notes">version {commit.slice(0, 10)}</small></li>)}</ul>
      <button className="link" onClick={() => setShowNotices(v => !v)}>{showNotices ? 'Hide' : 'Show'} licences</button>
      {showNotices && ref && <pre className="notices">{ref.notices}</pre>}</div>
  </section>;
}
