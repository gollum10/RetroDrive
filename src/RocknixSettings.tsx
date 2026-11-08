// ROCKNIX settings (ADR-082): a compact grid per scope (all systems or one system),
// reviewed before saving to ROCKNIX's system.cfg on its config share. Undo is in the
// settings history.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Eye } from 'lucide-react';
import GamePicker, { gameScope } from './GamePicker';

type Choice = { label: string; value: string };
type Setting = { key: string; label: string; help: string; choices: Choice[] };
type Options = { settings: Setting[]; systems: string[]; values: Record<string,string>; conf_sha: string; conf_path: string };
type Row = { system: string; key: string; before: string|null; after: string|null };
type Preview = { changes: Row[]; file_sha: string };

export default function RocknixSettings({ installationId, disabled, notify, onSaved }: { installationId: string; disabled: boolean; notify: (text: string, error?: boolean) => void; onSaved: () => void }) {
  const [o, setO] = useState<Options|null>(null); const [problem, setProblem] = useState('');
  const [scope, setScope] = useState('global'); const [game, setGame] = useState(''); const [pending, setPending] = useState<Record<string, string|null>>({});
  const [review, setReview] = useState<Preview|null>(null); const [working, setWorking] = useState(false);
  const load = async () => { try { setO(await invoke<Options>('rocknix_options', { installationId })); setProblem(''); } catch (e) { setO(null); setProblem(String(e)); } };
  useEffect(() => { setPending({}); setReview(null); setScope('global'); setGame(''); void load(); }, [installationId]);

  if (problem) return <div className="backups"><h3>ROCKNIX settings</h3><p className="warn-text">{problem}</p></div>;
  if (!o) return <div className="backups"><h3>ROCKNIX settings</h3><p className="plan-notes">Reading ROCKNIX's settings…</p></div>;

  const full = (s: Setting) => `${gameScope(scope, game)}.${s.key}`;
  const stored = (k: string) => o.values[k] ?? '';
  const current = (k: string) => k in pending ? (pending[k] ?? '') : stored(k);
  const set = (s: Setting, v: string) => { const k = full(s); setReview(null); setPending(p => { const n = { ...p }; if (v === stored(k)) delete n[k]; else n[k] = v === '' ? null : v; return n; }); };
  const count = Object.keys(pending).length;
  const changes = () => Object.entries(pending).map(([key, value]) => ({ key, value }));
  const doReview = async () => { try { setReview(await invoke<Preview>('preview_rocknix_settings', { installationId, changes: changes() })); } catch (e) { notify(`Could not check the changes: ${String(e)}`, true); } };
  const save = async () => { if (!review) return; setWorking(true);
    try { notify(await invoke<string>('apply_rocknix_settings', { installationId, changes: changes(), fileSha: review.file_sha })); setPending({}); setReview(null); await load(); onSaved(); }
    catch (e) { notify(`Could not save: ${String(e)}`, true); } finally { setWorking(false); } };
  const name = (s: Setting|undefined, v: string) => s?.choices.find(c => c.value === v)?.label ?? v;
  // No line: ROCKNIX's default for all systems; a system follows all systems; a game follows its system.
  const unset = (s: Setting|undefined, system: string) => { if (system === 'all systems') return "ROCKNIX's default";
    const [sys, one] = system.split(': '); const from = one ? (o.values[`${sys}.${s?.key}`] ?? o.values[`global.${s?.key}`]) : o.values[`global.${s?.key}`];
    return `Same as ${one ? sys : 'all systems'}${s && from ? ` (${name(s, from)})` : ''}`; };
  const label = (s: Setting|undefined, v: string|null, system: string) => v === null || v === '' ? unset(s, system) : name(s, v);

  return <div className="backups"><h3>ROCKNIX settings</h3>
    <p className="plan-notes">Saved in ROCKNIX's <span className="mono">{o.conf_path}</span>, the same file its menus use; ROCKNIX uses them the next time a game starts. A system set to <b>Same as all systems</b> follows the all-systems setting; pick a system and then a <b>Game</b> to set things for just that game.</p>
    <div className="actions"><label>Settings for <select className="choice" value={scope} onChange={e => { setScope(e.target.value); setGame(''); setReview(null); }}>
      <option value="global">All systems</option>{o.systems.map(s => <option key={s} value={s}>{s}{Object.keys(pending).some(k => k.startsWith(`${s}.`) || k.startsWith(`${s}["`)) ? ' •' : ''}</option>)}</select></label>
      <GamePicker installationId={installationId} system={scope} command="rocknix_games" value={game} disabled={disabled || working} onChange={g => { setGame(g); setReview(null); }} changed={g => Object.keys({ ...o.values, ...pending }).some(k => k.startsWith(`${gameScope(scope, g)}.`))}/></div>
    <div className="option-grid">{o.settings.map(s => { const v = current(full(s)); const other = v !== '' && !s.choices.some(c => c.value === v);
      return <label key={s.key} title={s.help}><span>{s.label}<small>{s.help}</small></span>
        <select className="choice" value={v} disabled={disabled || working} onChange={e => set(s, e.target.value)}>
          <option value="">{unset(s, scope === 'global' ? 'all systems' : game ? `${scope}: ${game}` : scope)}</option>
          {other && <option value={v}>{v} (chosen on the device)</option>}
          {s.choices.map(c => <option key={c.value} value={c.value}>{c.label}</option>)}</select></label>; })}</div>
    {count > 0 && !review && <div className="actions"><button className="primary" disabled={disabled || working} onClick={() => void doReview()}><Eye size={15}/> Review {count} change{count === 1 ? '' : 's'}</button><button className="ghost" onClick={() => setPending({})}>Discard</button></div>}
    {review && <div className="confirm"><p><b>Save these to ROCKNIX's system.cfg?</b></p><ul>{review.changes.map(r => { const s = o.settings.find(x => x.key === r.key); return <li key={r.system + r.key}>{r.system}: {s?.label ?? r.key}: {label(s, r.before, r.system)} → <b>{label(s, r.after, r.system)}</b></li>; })}
      <li className="plan-notes">Only these lines change; the file is backed up on the device's config share first and Undo puts it back.</li></ul>
      <div className="actions"><button className="primary" disabled={working} onClick={() => void save()}>{working ? 'Saving…' : 'Save'}</button><button className="ghost" disabled={working} onClick={() => setReview(null)}>Not now</button></div></div>}
  </div>;
}
