// RetroPie settings (RDS-023 step 4): the default emulator per system, chosen from the
// emulators installed on that Pi (configs\<system>\emulators.cfg). Reviewed, then saved;
// Undo is in the settings history.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Eye } from 'lucide-react';

type SystemEmulators = { system: string; emulators: string[]; default: string|null; sha: string };
type Options = { problem: string|null; configs_path: string|null; systems: SystemEmulators[] };
type Row = { system: string; key: string; before: string|null; after: string|null };
type Preview = { changes: Row[]; shas: Record<string,string> };

const ROWS = 12;

export default function RetroPieSettings({ installationId, disabled, notify, onSaved }: { installationId: string; disabled: boolean; notify: (text: string, error?: boolean) => void; onSaved: () => void }) {
  const [o, setO] = useState<Options|null>(null); const [pending, setPending] = useState<Record<string,string>>({});
  const [review, setReview] = useState<Preview|null>(null); const [working, setWorking] = useState(false); const [all, setAll] = useState(false);
  const load = async () => { try { setO(await invoke<Options>('retropie_options', { installationId })); } catch (e) { setO({ problem: String(e), configs_path: null, systems: [] }); } };
  useEffect(() => { setPending({}); setReview(null); void load(); }, [installationId]);

  if (!o) return <div className="backups"><h3>RetroPie settings</h3><p className="plan-notes">Reading RetroPie's settings…</p></div>;
  if (o.problem) return <div className="backups"><h3>RetroPie settings</h3><p className="plan-notes">{o.problem}</p></div>;

  const count = Object.keys(pending).length;
  const changes = () => Object.entries(pending).map(([system, emulator]) => ({ system, emulator }));
  const pick = (s: SystemEmulators, v: string) => { setReview(null); setPending(p => { const n = { ...p }; if (v === (s.default ?? '')) delete n[s.system]; else n[s.system] = v; return n; }); };
  const doReview = async () => { try { setReview(await invoke<Preview>('preview_retropie_emulators', { installationId, changes: changes() })); } catch (e) { notify(`Could not check the changes: ${String(e)}`, true); } };
  const save = async () => { if (!review) return; setWorking(true);
    try { notify(await invoke<string>('apply_retropie_emulators', { installationId, changes: changes(), shas: review.shas })); setPending({}); setReview(null); await load(); onSaved(); }
    catch (e) { notify(`Could not save: ${String(e)}`, true); } finally { setWorking(false); } };
  const shown = all ? o.systems : o.systems.slice(0, ROWS);

  return <div className="backups"><h3>RetroPie settings: emulators</h3>
    <p className="plan-notes">The emulator each system uses by default, from the ones installed on this RetroPie (saved in <span className="mono">configs\&lt;system&gt;\emulators.cfg</span>, the file RetroPie's own menu uses). RetroPie uses it the next time a game starts.</p>
    {o.systems.length === 0 ? <p className="plan-notes">No systems with games and installed emulators were found.</p> : <>
      <div className="option-grid">{shown.map(s => <label key={s.system}><span className="mono">{s.system}</span>
        <select className="choice" value={pending[s.system] ?? s.default ?? ''} disabled={disabled || working} onChange={e => pick(s, e.target.value)}>
          {!s.default && <option value="">Not set</option>}
          {s.emulators.map(id => <option key={id} value={id}>{id}{id === s.default ? ' (current)' : ''}</option>)}</select></label>)}</div>
      {o.systems.length > ROWS && <button className="link" onClick={() => setAll(v => !v)}>{all ? 'Show fewer' : `Show all ${o.systems.length} systems`}</button>}
      {count > 0 && !review && <div className="actions"><button className="primary" disabled={disabled || working} onClick={() => void doReview()}><Eye size={15}/> Review {count} change{count === 1 ? '' : 's'}</button><button className="ghost" onClick={() => setPending({})}>Discard</button></div>}
      {review && <div className="confirm"><p><b>Save these emulator choices?</b></p><ul>{review.changes.map(r => <li key={r.system}><span className="mono">{r.system}</span>: {r.before ?? 'not set'} → <b>{r.after}</b></li>)}
        <li className="plan-notes">Only the default line changes in each file; it's backed up first and Undo puts it back.</li></ul>
        <div className="actions"><button className="primary" disabled={working} onClick={() => void save()}>{working ? 'Saving…' : 'Save'}</button><button className="ghost" disabled={working} onClick={() => setReview(null)}>Not now</button></div></div>}
    </>}
  </div>;
}
