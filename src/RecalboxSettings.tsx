// Recalbox settings and theme (ADR-068): a compact grid per scope (all systems or one
// system), reviewed before saving to recalbox.conf; the theme is in the same file.
// Undo is in the settings history.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Eye, Palette } from 'lucide-react';
import GamePicker from './GamePicker';

type Choice = { label: string; value: string };
type Setting = { key: string; label: string; help: string; per_system: boolean; default: string; choices: Choice[] };
type Options = { settings: Setting[]; systems: string[]; values: Record<string,string>; conf_sha: string; themes: string[]; theme: string };
type Row = { system: string; key: string; before: string|null; after: string|null };
type Preview = { changes: Row[]; file_sha: string };
type GameOptions = { values: Record<string,string>; exists: boolean; file_sha: string };

const THEME_KEY = 'emulationstation.theme.folder';
const BUILTIN = 'recalbox-next';
const themeName = (t: string) => t === BUILTIN ? 'Recalbox Next (built in)' : t;

export default function RecalboxSettings({ installationId, disabled, notify, onSaved }: { installationId: string; disabled: boolean; notify: (text: string, error?: boolean) => void; onSaved: () => void }) {
  const [o, setO] = useState<Options|null>(null); const [problem, setProblem] = useState('');
  const [scope, setScope] = useState('global'); const [game, setGame] = useState(''); const [gameOpts, setGameOpts] = useState<GameOptions|null>(null); const [own, setOwn] = useState<string[]>([]); const [pending, setPending] = useState<Record<string, string|null>>({});
  const [review, setReview] = useState<Preview|null>(null); const [working, setWorking] = useState(false);
  const load = async () => { try { setO(await invoke<Options>('recalbox_options', { installationId })); setProblem(''); } catch (e) { setO(null); setProblem(String(e)); } };
  useEffect(() => { setPending({}); setReview(null); setScope('global'); setGame(''); setGameOpts(null); void load(); }, [installationId]);
  // One game (ADR-085): its own settings come from <game>.recalbox.conf next to it.
  const loadGame = async (sys: string, g: string) => { setGameOpts(null); if (!g) return; try { setGameOpts(await invoke<GameOptions>('recalbox_game_options', { installationId, system: sys, game: g })); } catch (e) { notify(String(e), true); } };
  const loadOwn = async (sys: string) => { setOwn([]); if (sys === 'global') return; try { setOwn(await invoke<string[]>('recalbox_games_with_settings', { installationId, system: sys })); } catch { /* the dots are a hint only */ } };
  useEffect(() => { void loadOwn(scope); }, [installationId, scope]);
  const pickGame = (g: string) => { setGame(g); setPending({}); setReview(null); void loadGame(scope, g); };

  if (problem) return <div className="backups"><h3>Recalbox settings</h3><p className="warn-text">{problem}</p></div>;
  if (!o) return <div className="backups"><h3>Recalbox settings</h3><p className="plan-notes">Reading Recalbox's settings…</p></div>;

  const full = (s: Setting) => s.per_system ? `${scope}.${s.key}` : `global.${s.key}`;
  const shown = o.settings.filter(s => s.per_system || (scope === 'global' && !game));
  const source = game ? (gameOpts?.values ?? {}) : o.values;
  const current = (k: string) => k in pending ? (pending[k] ?? '') : (source[k] ?? '');
  const value = (s: Setting) => { const v = current(full(s)); return v === '' && full(s).startsWith('global.') && !game ? s.default : v; };
  const setKey = (k: string, v: string, now: string) => { setReview(null); setPending(p => { const n = { ...p }; if (v === now) delete n[k]; else n[k] = v === '' ? null : v; return n; }); };
  const set = (s: Setting, v: string) => { const k = full(s); const stored = source[k] ?? ''; setKey(k, v, stored === '' && k.startsWith('global.') && !game ? s.default : stored); };
  const count = Object.keys(pending).length;
  const changes = () => Object.entries(pending).map(([key, value]) => ({ key, value }));
  const doReview = async () => { try { setReview(await invoke<Preview>(game ? 'preview_recalbox_game' : 'preview_recalbox_settings', game ? { installationId, system: scope, game, changes: changes() } : { installationId, changes: changes() })); } catch (e) { notify(`Could not check the changes: ${String(e)}`, true); } };
  const save = async () => { if (!review) return; setWorking(true);
    try { notify(await invoke<string>(game ? 'apply_recalbox_game' : 'apply_recalbox_settings', game ? { installationId, system: scope, game, changes: changes(), fileSha: review.file_sha } : { installationId, changes: changes(), fileSha: review.file_sha })); setPending({}); setReview(null); if (game) { await loadGame(scope, game); await loadOwn(scope); } else await load(); onSaved(); }
    catch (e) { notify(`Could not save: ${String(e)}`, true); } finally { setWorking(false); } };
  const byKey = (k: string) => o.settings.find(s => s.key === k);
  const label = (s: Setting|undefined, v: string|null, system: string) => {
    if (v === null || v === '') return system === 'all systems' || system === 'interface' ? (s ? `${s.choices.find(c => c.value === s.default)?.label ?? s.default} (Recalbox's default)` : 'default') : system.includes(': ') ? `Same as ${system.split(': ')[0]}` : 'Same as all systems';
    return s?.choices.find(c => c.value === v)?.label ?? themeName(v);
  };
  const theme = current(THEME_KEY) || o.theme;

  return <>
    <div className="backups"><h3>Recalbox settings</h3>
      <p className="plan-notes">Saved in Recalbox's <span className="mono">system\recalbox.conf</span>, the same file its menus use; Recalbox uses them the next time a game starts. A system set to <b>Same as all systems</b> follows the all-systems setting; pick a system and then a <b>Game</b> to set things for just that game.</p>
      <div className="actions"><label>Settings for <select className="choice" value={scope} onChange={e => { setScope(e.target.value); setGame(''); setGameOpts(null); setPending(p => game ? {} : p); setReview(null); }}>
        <option value="global">All systems</option>{o.systems.map(s => <option key={s} value={s}>{s}{Object.keys(pending).some(k => k.startsWith(`${s}.`)) ? ' •' : ''}</option>)}</select></label>
        <GamePicker installationId={installationId} system={scope} command="recalbox_games" value={game} disabled={disabled || working} onChange={pickGame} changed={g => own.includes(g)}/></div>
      {game && <p className="plan-notes">{gameOpts?.exists ? <>This game's own settings are in <span className="mono">{game}.recalbox.conf</span> next to it.</> : <>The first change creates <span className="mono">{game}.recalbox.conf</span> next to the game; Undo moves it into RetroDrive's backup folder.</>}</p>}
      {(!game || gameOpts) && <div className="option-grid">{shown.map(s => <label key={s.key} title={s.help}><span>{s.label}<small>{s.help}</small></span>
        <select className="choice" value={value(s)} disabled={disabled || working} onChange={e => set(s, e.target.value)}>
          {game ? <option value="">{(() => { const v = o.values[`${scope}.${s.key}`] ?? o.values[`global.${s.key}`] ?? s.default; return `Same as ${scope} (${s.choices.find(c => c.value === v)?.label ?? v})`; })()}</option>
          : scope !== 'global' && s.per_system && <option value="">{`Same as all systems (${s.choices.find(c => c.value === (o.values[`global.${s.key}`] ?? s.default))?.label ?? o.values[`global.${s.key}`] ?? s.default})`}</option>}
          {s.choices.map(c => <option key={c.value} value={c.value}>{c.label}</option>)}</select></label>)}
        {scope === 'global' && !game && <label title="Recalbox's interface theme"><span>Theme<small>Themes in Recalbox's themes folder, plus the built-in one. Shows the next time Recalbox starts.</small></span>
          <select className="choice" value={theme} disabled={disabled || working} onChange={e => setKey(THEME_KEY, e.target.value, o.theme)}>
            {o.themes.map(t => <option key={t} value={t}>{themeName(t)}{t === o.theme ? ' (current)' : ''}</option>)}</select></label>}</div>}
      {count > 0 && !review && <div className="actions"><button className="primary" disabled={disabled || working} onClick={() => void doReview()}><Eye size={15}/> Review {count} change{count === 1 ? '' : 's'}</button><button className="ghost" onClick={() => setPending({})}>Discard</button></div>}
      {review && <div className="confirm"><p><b>{game ? `Save these to ${game}.recalbox.conf?` : 'Save these to recalbox.conf?'}</b></p><ul>{review.changes.map(r => { const s = byKey(r.key); return <li key={r.system + r.key}>{r.system}: {r.key === 'theme' ? <><Palette size={13}/> Theme</> : s?.label ?? r.key}: {label(s, r.before, r.system)} → <b>{label(s, r.after, r.system)}</b></li>; })}
        <li className="plan-notes">{game && !gameOpts?.exists ? "This creates the file next to the game; Undo moves it into RetroDrive's backup folder." : 'Only these lines change; the file is backed up first and Undo puts it back.'}</li></ul>
        <div className="actions"><button className="primary" disabled={working} onClick={() => void save()}>{working ? 'Saving…' : 'Save'}</button><button className="ghost" disabled={working} onClick={() => setReview(null)}>Not now</button></div></div>}
    </div>
  </>;
}
