// Batocera settings and theme (RDS-024): a compact grid per scope (all systems or one
// system), reviewed before saving; theme switch. Undo is in the settings history.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Eye, Palette } from 'lucide-react';

type Choice = { label: string; value: string };
type Setting = { key: string; label: string; help: string; per_system: boolean; choices: Choice[] };
type Options = { settings: Setting[]; systems: string[]; values: Record<string,string>; conf_sha: string; themes: string[]; theme: string|null; es_sha: string|null; network: boolean };
type Row = { system: string; key: string; before: string|null; after: string|null };
type Preview = { changes: Row[]; file_sha: string };

const BUILTIN = 'es-theme-carbon';
const themeName = (t: string) => t === BUILTIN ? 'Carbon (built in)' : t;

export default function BatoceraSettings({ installationId, disabled, notify, onSaved }: { installationId: string; disabled: boolean; notify: (text: string, error?: boolean) => void; onSaved: () => void }) {
  const [o, setO] = useState<Options|null>(null); const [problem, setProblem] = useState('');
  const [scope, setScope] = useState('global'); const [pending, setPending] = useState<Record<string, string|null>>({});
  const [review, setReview] = useState<Preview|null>(null); const [theme, setTheme] = useState(''); const [themeReview, setThemeReview] = useState<Preview|null>(null);
  const [working, setWorking] = useState(false);
  const load = async () => { try { const x = await invoke<Options>('batocera_options', { installationId }); setO(x); setTheme(x.theme ?? BUILTIN); setProblem(''); } catch (e) { setO(null); setProblem(String(e)); } };
  useEffect(() => { setPending({}); setReview(null); setThemeReview(null); setScope('global'); void load(); }, [installationId]);

  if (problem) return <div className="backups"><h3>Batocera settings</h3><p className="warn-text">{problem}</p></div>;
  if (!o) return <div className="backups"><h3>Batocera settings</h3><p className="plan-notes">Reading Batocera's settings…</p></div>;

  const full = (s: Setting) => s.per_system ? `${scope}.${s.key}` : `audio.${s.key}`;
  const shown = o.settings.filter(s => s.per_system || scope === 'global');
  const value = (s: Setting) => { const k = full(s); return k in pending ? (pending[k] ?? '') : (o.values[k] ?? ''); };
  const set = (s: Setting, v: string) => { const k = full(s); const now = o.values[k] ?? ''; setReview(null);
    setPending(p => { const n = { ...p }; if (v === now) delete n[k]; else n[k] = v === '' ? null : v; return n; }); };
  const count = Object.keys(pending).length;
  const changes = () => Object.entries(pending).map(([key, value]) => ({ key, value }));
  const doReview = async () => { try { setReview(await invoke<Preview>('preview_batocera_settings', { installationId, changes: changes() })); } catch (e) { notify(`Could not check the changes: ${String(e)}`, true); } };
  const save = async () => { if (!review) return; setWorking(true);
    try { notify(await invoke<string>('apply_batocera_settings', { installationId, changes: changes(), fileSha: review.file_sha })); setPending({}); setReview(null); await load(); onSaved(); }
    catch (e) { notify(`Could not save: ${String(e)}`, true); } finally { setWorking(false); } };
  const reviewTheme = async () => { try { setThemeReview(await invoke<Preview>('preview_batocera_theme', { installationId, theme })); } catch (e) { notify(String(e), true); } };
  const saveTheme = async () => { if (!themeReview) return; setWorking(true);
    try { notify(await invoke<string>('apply_batocera_theme', { installationId, theme, fileSha: themeReview.file_sha })); setThemeReview(null); await load(); onSaved(); }
    catch (e) { notify(`Could not switch the theme: ${String(e)}`, true); } finally { setWorking(false); } };
  const label = (s: Setting, v: string|null) => v === null || v === '' ? (scope === 'global' || !s.per_system ? 'Auto' : 'Same as all systems') : (s.choices.find(c => c.value === v)?.label ?? v);
  const byKey = (k: string) => o.settings.find(s => s.key === k);

  return <>
    <div className="backups"><h3>Batocera settings</h3>
      <p className="plan-notes">Saved in Batocera's <span className="mono">system\batocera.conf</span>, the same file its menus use; Batocera uses them the next time a game starts. Choose <b>Auto</b> to let Batocera decide. Emulator and core choices aren't offered yet (Batocera keeps that list where Windows can't read it).</p>
      <div className="actions"><label>Settings for <select className="choice" value={scope} onChange={e => { setScope(e.target.value); setReview(null); }}>
        <option value="global">All systems</option>{o.systems.map(s => <option key={s} value={s}>{s}{Object.keys(pending).some(k => k.startsWith(`${s}.`)) ? ' •' : ''}</option>)}</select></label></div>
      <div className="option-grid">{shown.map(s => <label key={s.key} title={s.help}><span>{s.label}<small>{s.help}</small></span>
        <select className="choice" value={value(s)} disabled={disabled || working} onChange={e => set(s, e.target.value)}>
          <option value="">{scope === 'global' || !s.per_system ? 'Auto (Batocera decides)' : `Same as all systems${o.values[`global.${s.key}`] ? ` (${label(s, o.values[`global.${s.key}`])})` : ''}`}</option>
          {s.choices.map(c => <option key={c.value} value={c.value}>{c.label}</option>)}</select></label>)}</div>
      {count > 0 && !review && <div className="actions"><button className="primary" disabled={disabled || working} onClick={() => void doReview()}><Eye size={15}/> Review {count} change{count === 1 ? '' : 's'}</button><button className="ghost" onClick={() => setPending({})}>Discard</button></div>}
      {review && <div className="confirm"><p><b>Save these to batocera.conf?</b></p><ul>{review.changes.map(r => { const s = byKey(r.key); return <li key={r.system + r.key}>{r.system}: {s?.label ?? r.key}: {s ? label(s, r.before) : r.before ?? 'Auto'} → <b>{s ? label(s, r.after) : r.after ?? 'Auto'}</b></li>; })}
        <li className="plan-notes">Only these lines change; the file is backed up first and Undo puts it back.</li></ul>
        <div className="actions"><button className="primary" disabled={working} onClick={() => void save()}>{working ? 'Saving…' : 'Save'}</button><button className="ghost" disabled={working} onClick={() => setReview(null)}>Not now</button></div></div>}
    </div>

    <div className="backups"><h3>Theme</h3>
      {o.es_sha === null ? <p className="plan-notes">Start Batocera once so it creates its interface settings; then the theme can be changed here.</p> : <>
        <p className="plan-notes">Themes you've added to Batocera's <span className="mono">themes</span> folder, plus its built-in Carbon.{o.network ? ' Over the network, restart Batocera right after switching, before changing other settings in its menu.' : ''}</p>
        <div className="actions"><select className="choice" value={theme} disabled={disabled || working} onChange={e => { setTheme(e.target.value); setThemeReview(null); }}>
          {o.themes.map(t => <option key={t} value={t}>{themeName(t)}{t === (o.theme ?? BUILTIN) ? ' (current)' : ''}</option>)}</select>
          {!themeReview ? <button className="ghost" disabled={disabled || working || theme === (o.theme ?? BUILTIN)} onClick={() => void reviewTheme()}><Palette size={15}/> Use this theme</button>
          : <><button className="primary" disabled={working} onClick={() => void saveTheme()}>{working ? 'Switching…' : `Switch to ${themeName(theme)}`}</button><button className="ghost" disabled={working} onClick={() => setThemeReview(null)}>Not now</button></>}</div></>}
    </div>
  </>;
}
