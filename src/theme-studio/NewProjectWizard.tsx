import { useState } from 'react';
import { displays, newDocument, type NewTheme } from './document';
import type { ThemeProject } from './types';

export default function NewProjectWizard({ busy, onCreate, onCancel }: { busy: boolean; onCreate: (project: ThemeProject) => void; onCancel: () => void }) {
  const [form, setForm] = useState<NewTheme>({ name: '', author: '', description: '', version: '1.0.0', license: 'unknown', target: '', targetVersion: '', display: '16:9', template: 'ready' });
  const change = (key: keyof NewTheme, value: string) => setForm(f => ({ ...f, [key]: value }));
  return <section className="panel ts-wizard" aria-labelledby="new-theme-title"><h2 id="new-theme-title">Create a theme</h2>
    <p>Design your own theme: your art, colours and layout on every screen. You don't need a system connected; choose which one it's for now or later.</p>
    <form onSubmit={event => { event.preventDefault(); onCreate(newDocument(form)); }}>
      <fieldset disabled={busy}><legend>About your theme</legend>
        <label>Name<input autoFocus required maxLength={120} value={form.name} onChange={e => change('name', e.target.value)} /></label>
        <label>Author<input value={form.author} onChange={e => change('author', e.target.value)} /></label>
        <label>Description<textarea value={form.description} onChange={e => change('description', e.target.value)} /></label>
        <label>Version<input required maxLength={64} value={form.version} onChange={e => change('version', e.target.value)} /></label>
        <label>License (if you share it)<small className="ts-field-help">How others may use your theme, for example CC BY 4.0. Leave “unknown” if it's just for you.</small><input required maxLength={256} value={form.license} onChange={e => change('license', e.target.value)} /></label>
        <label>Made for<select value={form.target} onChange={e => change('target', e.target.value)}><option value="">Any system (choose later)</option><option value="retrobat">RetroBat</option><option value="batocera">Batocera (coming later)</option><option value="esde">ES-DE (coming later)</option></select></label>
        {form.target && <label>System version (optional)<input maxLength={64} value={form.targetVersion} onChange={e => change('targetVersion', e.target.value)} /></label>}
        <p className="ts-field-help">You can change this later. Your design is kept exactly as you make it; saving it as a theme for a system is coming soon.</p>
        <label>Screen shape<select value={form.display} onChange={e => change('display', e.target.value)}>{Object.keys(displays).map(name => <option key={name}>{name}</option>)}</select></label>
        <label>Starting layout<select value={form.template} onChange={e => change('template', e.target.value)}><option value="ready">Ready layout: background, game title and description to change</option><option value="starter">Starter: each screen labelled</option><option value="blank">Blank screens</option></select></label>
      </fieldset>
      <div className="actions"><button className="primary" disabled={busy || !form.name.trim() || !form.version.trim() || !form.license.trim()} type="submit">{busy ? 'Creating…' : 'Create theme'}</button><button type="button" className="ghost" disabled={busy} onClick={onCancel}>Cancel</button></div>
    </form>
  </section>;
}
