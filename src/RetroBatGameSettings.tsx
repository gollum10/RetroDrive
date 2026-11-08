// RetroBat settings for one game (ADR-084): pick a system and a game, then its own
// emulator, shader, bezel, aspect ratio and integer scaling. Written to RetroBat's
// es_settings.cfg the way its game menu does; reviewed, backed up, undoable.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Eye } from 'lucide-react';

type Choice = { label: string; value: string };
type SystemSettings = { system: string; name: string; has_games: boolean; emulators: { emulator: string; cores: string[] }[]; current: Record<string,string> };
type SettingsOptions = { systems: SystemSettings[]; shaders: Choice[]; bezels: Choice[]; ratios: Choice[]; integer_scaling: Choice[]; blocked: string|null };
type GameSettings = { games: string[]; current: Record<string, Record<string,string>> };
type Row = { system: string; key: string; before: string|null; after: string|null };
type Preview = { changes: Row[]; file_sha: string };

const LABELS: Record<string,string> = { emulator: 'Emulator', core: 'Core', shaderset: 'Shader', bezel: 'Bezel', ratio: 'Aspect ratio', integerscale: 'Integer scaling' };

export default function RetroBatGameSettings({ installationId, emu, disabled, notify, onSaved }: { installationId: string; emu: SettingsOptions; disabled: boolean; notify: (text: string, error?: boolean) => void; onSaved: () => void }) {
  const systems = emu.systems.filter(s => s.has_games);
  const [system, setSystem] = useState(''); const [game, setGame] = useState('');
  const [data, setData] = useState<GameSettings|null>(null); const [pending, setPending] = useState<Record<string, string|null>>({});
  const [review, setReview] = useState<Preview|null>(null); const [working, setWorking] = useState(false);
  const load = async (sys: string) => { setData(null); if (!sys) return; try { setData(await invoke<GameSettings>('retrobat_game_settings', { installationId, system: sys })); } catch (e) { notify(String(e), true); } };
  useEffect(() => { setSystem(''); setGame(''); setPending({}); setReview(null); setData(null); }, [installationId]);

  const sys = systems.find(s => s.system === system);
  const own = (data && game ? data.current[game] : undefined) ?? {};
  const value = (k: string) => k in pending ? (pending[k] ?? '') : (own[k] ?? '');
  const set = (patch: Record<string, string>) => { setReview(null); setPending(p => { const n = { ...p }; for (const [k, v] of Object.entries(patch)) { if (v === (own[k] ?? '')) delete n[k]; else n[k] = v === '' ? null : v; } return n; }); };
  const show = (k: string, v: string, choices: Choice[]) => choices.find(c => c.value === v)?.label ?? v;
  const sameAs = (k: string, choices: Choice[]) => { const v = sys?.current[k]; return `Same as ${system}${v ? ` (${show(k, v, choices)})` : ''}`; };
  const changes = () => Object.entries(pending).map(([key, v]) => ({ system, key, value: v, game }));
  const count = Object.keys(pending).length;
  const doReview = async () => { try { setReview(await invoke<Preview>('preview_emulator_settings', { installationId, changes: changes() })); } catch (e) { notify(`Could not check the changes: ${String(e)}`, true); } };
  const save = async () => { if (!review) return; setWorking(true);
    try { notify(await invoke<string>('apply_emulator_settings', { installationId, changes: changes(), fileSha: review.file_sha })); setPending({}); setReview(null); await load(system); onSaved(); }
    catch (e) { notify(`Could not save: ${String(e)}`, true); } finally { setWorking(false); } };
  const pick = (k: string, choices: Choice[]) => <label key={k}><span>{LABELS[k]}</span>
    <select className="choice" value={value(k)} disabled={disabled || working} onChange={e => set({ [k]: e.target.value })}>
      <option value="">{sameAs(k, choices)}</option>{choices.map(c => <option key={c.value} value={c.value}>{c.label}</option>)}</select></label>;
  const ev = value('emulator'); const emuValue = ev ? `${ev}|${value('core')}` : '';
  const before = (r: Row, v: string|null) => v === null || v === '' ? `Same as ${r.system.split(': ')[0]}` : v;

  return <div className="backups"><h3>Settings for one game</h3>
    <p className="plan-notes">Give one game its own emulator or display settings; every other game keeps {'the system\'s'}. Saved in RetroBat's settings the same way its game menu does it. Close RetroBat first.</p>
    <div className="actions"><label>System <select className="choice" value={system} disabled={working} onChange={e => { setSystem(e.target.value); setGame(''); setPending({}); setReview(null); void load(e.target.value); }}>
      <option value="">Choose a system…</option>{systems.map(s => <option key={s.system} value={s.system}>{s.name}</option>)}</select></label>
      {system && <label>Game <select className="choice" value={game} disabled={working || !data} onChange={e => { setGame(e.target.value); setPending({}); setReview(null); }}>
        <option value="">{data ? 'Choose a game…' : 'Reading…'}</option>{data?.games.map(g => <option key={g} value={g}>{g}{data.current[g] ? ' •' : ''}</option>)}</select></label>}</div>
    {sys && game && <>
      <div className="option-grid">
        <label><span>Emulator</span><select className="choice" value={emuValue} disabled={disabled || working} onChange={e => { const v = e.target.value; if (!v) set({ emulator: '', core: '' }); else { const [em, co] = v.split('|'); set({ emulator: em, core: co ?? '' }); } }}>
          <option value="">{`Same as ${system}${sys.current.emulator ? ` (${sys.current.emulator}${sys.current.core ? `: ${sys.current.core}` : ''})` : ''}`}</option>
          {sys.emulators.flatMap(em => em.cores.length ? em.cores.map(co => <option key={`${em.emulator}|${co}`} value={`${em.emulator}|${co}`}>{em.emulator}: {co}</option>) : [<option key={`${em.emulator}|`} value={`${em.emulator}|`}>{em.emulator}</option>])}</select></label>
        {pick('shaderset', emu.shaders)}{pick('bezel', emu.bezels)}{pick('ratio', emu.ratios)}{pick('integerscale', emu.integer_scaling)}</div>
      {emu.blocked && <p className="warn-text">{emu.blocked}</p>}
      {count > 0 && !review && <div className="actions"><button className="primary" disabled={disabled || working} onClick={() => void doReview()}><Eye size={15}/> Review {count} change{count === 1 ? '' : 's'}</button><button className="ghost" onClick={() => setPending({})}>Discard</button></div>}
      {review && <div className="confirm"><p><b>Save these to RetroBat's settings?</b></p><ul>{review.changes.map(r => <li key={r.system + r.key}>{r.system}: {LABELS[r.key] ?? r.key}: {before(r, r.before)} → <b>{before(r, r.after)}</b></li>)}
        <li className="plan-notes">Only these lines change; the file is backed up first and Undo (under Show every settings change) puts it back.</li></ul>
        <div className="actions"><button className="primary" disabled={working} onClick={() => void save()}>{working ? 'Saving…' : 'Save'}</button><button className="ghost" disabled={working} onClick={() => setReview(null)}>Not now</button></div></div>}
    </>}
  </div>;
}
