import { useState } from 'react';
import { displays, newDocument, type NewTheme } from './document';
import type { ThemeProject } from './types';

export default function NewProjectWizard({ busy, onCreate, onCancel }: { busy: boolean; onCreate: (project: ThemeProject) => void; onCancel: () => void }) {
  const [form, setForm] = useState<NewTheme>({ name: '', author: '', description: '', version: '1.0.0', license: 'unknown', target: '', targetVersion: '', display: '16:9', template: 'starter' });
  const change = (key: keyof NewTheme, value: string) => setForm(f => ({ ...f, [key]: value }));
  return <section className="panel ts-wizard" aria-labelledby="new-theme-title"><h2 id="new-theme-title">Create a theme</h2>
    <p>Build an original universal project. No emulator or connected installation is needed.</p>
    <form onSubmit={event => { event.preventDefault(); onCreate(newDocument(form)); }}>
      <fieldset disabled={busy}><legend>Project details</legend>
        <label>Name<input autoFocus required maxLength={120} value={form.name} onChange={e => change('name', e.target.value)} /></label>
        <label>Author<input value={form.author} onChange={e => change('author', e.target.value)} /></label>
        <label>Description<textarea value={form.description} onChange={e => change('description', e.target.value)} /></label>
        <label>Project version<input required maxLength={64} value={form.version} onChange={e => change('version', e.target.value)} /></label>
        <label>License<input required maxLength={256} value={form.license} onChange={e => change('license', e.target.value)} /></label>
        <label>Intended frontend<select value={form.target} onChange={e => change('target', e.target.value)}><option value="">Universal / No Target Yet</option><option value="retrobat">RetroBat</option><option value="batocera">Batocera (future exporter)</option><option value="esde">ES-DE (future exporter)</option></select></label>
        {form.target && <label>Frontend version (optional)<input maxLength={64} value={form.targetVersion} onChange={e => change('targetVersion', e.target.value)} /></label>}
        <p>Target choices record intent. Native compatibility and export are not verified yet.</p>
        <label>Display<select value={form.display} onChange={e => change('display', e.target.value)}>{Object.keys(displays).map(name => <option key={name}>{name}</option>)}</select></label>
        <label>Starting layout<select value={form.template} onChange={e => change('template', e.target.value)}><option value="starter">Original starter — scene titles</option><option value="blank">Blank scenes</option></select></label>
      </fieldset>
      <div className="actions"><button className="primary" disabled={busy || !form.name.trim() || !form.version.trim() || !form.license.trim()} type="submit">{busy ? 'Creating…' : 'Create project'}</button><button type="button" className="ghost" disabled={busy} onClick={onCancel}>Cancel</button></div>
    </form>
  </section>;
}
