// Game groups on RetroBat/Batocera (ADR-070): switch on their built-in "lightgun
// games" and "vertical arcade" collections, reviewed before saving; plus, for a
// vertical screen, where RetroBat's orientation is set (Windows) and Batocera's
// (its settings, ADR-071).
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Crosshair, Eye, RectangleVertical } from 'lucide-react';

type Group = { id: string; label: string; on: boolean; in_library: number };
type State = { groups: Group[]; file_sha: string; network: boolean };
type Row = { system: string; key: string; before: string|null; after: string|null };
type Preview = { changes: Row[]; file_sha: string };

export default function GameGroups({ installationId, adapterId, disabled, notify, onSaved }: { installationId: string; adapterId: string; disabled: boolean; notify: (t: string, e?: boolean) => void; onSaved: () => void }) {
  const [s, setS] = useState<State|null>(null); const [problem, setProblem] = useState('');
  const [want, setWant] = useState<Record<string, boolean>>({}); const [review, setReview] = useState<Preview|null>(null); const [working, setWorking] = useState(false);
  const name = adapterId === 'batocera' ? 'Batocera' : 'RetroBat';
  const load = async () => { try { const x = await invoke<State>('game_groups_state', { installationId }); setS(x); setWant(Object.fromEntries(x.groups.map(g => [g.id, g.on]))); setProblem(''); } catch (e) { setS(null); setProblem(String(e)); } };
  useEffect(() => { setReview(null); void load(); }, [installationId]);
  if (problem) return <div className="backups"><h3>Game groups</h3><p className="warn-text">{problem}</p></div>;
  if (!s) return null;
  const changed = s.groups.filter(g => want[g.id] !== g.on);
  const wanted = () => Object.fromEntries(changed.map(g => [g.id, want[g.id]]));
  const doReview = async () => { try { setReview(await invoke<Preview>('preview_game_groups', { installationId, wanted: wanted() })); } catch (e) { notify(`Could not check the change: ${String(e)}`, true); } };
  const save = async () => { if (!review) return; setWorking(true);
    try { notify(await invoke<string>('apply_game_groups', { installationId, wanted: wanted(), fileSha: review.file_sha })); setReview(null); await load(); onSaved(); }
    catch (e) { notify(`Could not save: ${String(e)}`, true); } finally { setWorking(false); } };

  return <div className="backups"><h3>Game groups</h3>
    <p className="plan-notes">{name} can show these as their own groups in its menu, filled in by {name} from the same game lists RetroDrive uses. Copy the games first (Plan, or a collection of them from the Game Library).</p>
    <div className="group-list">{s.groups.map(g => <label key={g.id} className="check"><input type="checkbox" checked={!!want[g.id]} disabled={disabled || working} onChange={e => { setReview(null); setWant(w => ({ ...w, [g.id]: e.target.checked })); }}/>
      {g.id === 'lightgun' ? <Crosshair size={14}/> : <RectangleVertical size={14}/>} {g.label} <small className="plan-notes">({g.in_library.toLocaleString()} in your library{g.on ? '; on now' : ''})</small></label>)}</div>
    {changed.length > 0 && !review && <div className="actions"><button className="primary" disabled={disabled || working} onClick={() => void doReview()}><Eye size={15}/> Review</button><button className="ghost" onClick={() => setWant(Object.fromEntries(s.groups.map(g => [g.id, g.on])))}>Discard</button></div>}
    {review && <div className="confirm"><p><b>Change {name}'s game groups?</b></p><ul>{review.changes.map(r => <li key={r.key}>{r.before} → <b>{r.after}</b></li>)}
      <li className="plan-notes">Only this setting changes; the file is backed up first and Undo puts it back.{adapterId === 'retrobat' ? ' RetroBat must be closed.' : ''}</li></ul>
      <div className="actions"><button className="primary" disabled={working} onClick={() => void save()}>{working ? 'Saving…' : 'Save'}</button><button className="ghost" disabled={working} onClick={() => setReview(null)}>Not now</button></div></div>}
    <p className="plan-notes"><RectangleVertical size={13}/> <b>Vertical screen?</b> {adapterId === 'batocera'
      ? <>Set <b>Screen rotation</b> in Batocera settings above.</>
      : <>RetroBat uses Windows' own screen orientation: Windows Settings → System → Display → <b>Display orientation</b> (Portrait).</>}</p>
  </div>;
}
