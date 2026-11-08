import { useEffect, useState, useSyncExternalStore } from 'react';
import type { ProjectSession } from './session';
import { errorText } from './document';

export default function ProjectOverview({ session, onClose, onCopy, onReload }: { session: ProjectSession; onClose: () => void; onCopy: (name: string) => void; onReload: () => void }) {
  const change = useSyncExternalStore(session.subscribe, session.snapshot, session.snapshot);
  const [sceneId, setSceneId] = useState(session.document.scenes[0].id);
  const [copyName, setCopyName] = useState<string | null>(null);
  const [confirmReload, setConfirmReload] = useState(false);
  const document = session.document;
  const scene = document.scenes.find(s => s.id === sceneId) ?? document.scenes[0];
  useEffect(() => {
    if (!session.dirty || session.error || session.busy) return;
    const timer = window.setTimeout(() => { void session.flush().catch(() => undefined); }, 500);
    return () => window.clearTimeout(timer);
  }, [session, change]);
  const status = session.error ? session.conflict ? 'Conflict — local edits kept' : 'Save failed — local edits kept' : session.busy ? 'Saving…' : session.dirty ? 'Unsaved edits' : session.record.draftJson ? 'Autosaved draft' : 'Saved checkpoint';
  const metadata = (key: string, value: string) => session.edit({ ...document, metadata: { ...document.metadata, [key]: value } });
  return <section className="ts-editor" aria-label="Theme Studio editor">
    <div className="panel ts-toolbar"><h2>{document.metadata.name || 'Untitled theme'}</h2><span role="status" aria-live="polite">{status}</span><button className="primary" disabled={session.busy || !!session.error} onClick={() => { void session.save().catch(() => undefined); }}>Save checkpoint</button><button disabled={session.busy} onClick={() => setCopyName(`${document.metadata.name} copy`)}>Save As…</button><button className="ghost" onClick={onClose}>Back to projects</button></div>
    {session.error != null && <div className="panel" role="alert"><p>{errorText(session.error)}</p>{!session.conflict && <button onClick={() => { void session.retry().catch(() => undefined); }}>Retry autosave</button>}<p>Save As keeps your current local edits in a separate project. Reload replaces this session with the database version.</p><button className="ghost" onClick={() => setConfirmReload(true)}>Reload from database…</button></div>}
    {confirmReload && <div className="panel" role="alert"><p>Discard this session's unsaved edits and reopen the database version? Save a copy first to keep these edits.</p><button onClick={onReload}>Discard local edits and reload</button><button className="ghost" onClick={() => setConfirmReload(false)}>Keep editing</button></div>}
    {copyName !== null && <form className="panel ts-toolbar" onSubmit={e => { e.preventDefault(); onCopy(copyName); }}><label>New project name<input autoFocus maxLength={120} required value={copyName} onChange={e => setCopyName(e.target.value)} /></label><button type="submit" disabled={session.busy || !copyName.trim()}>Create independent copy</button><button type="button" onClick={() => setCopyName(null)}>Cancel</button></form>}
    <p>Edits autosave as a draft. Save checkpoint marks a version to return to. Target: {document.targets.length ? document.targets.map(t => `${t.frontendFamily}${t.frontendVersion ? ` ${t.frontendVersion}` : ' (version unknown)'}`).join(', ') : 'Universal / No Target Yet'}. Native support is unverified.</p>
    <div className="ts-editor-grid">
      <aside className="panel ts-scenes" aria-label="Scenes and layers"><h3>Scenes</h3><nav aria-label="Select scene">{document.scenes.map(s => <button key={s.id} aria-pressed={scene.id === s.id} onClick={() => setSceneId(s.id)}>{s.name}</button>)}</nav><h3>Layers</h3>{scene.nodes.length ? <ul>{scene.nodes.map(node => <li key={node.id}>{node.name} <small>({node.type})</small></li>)}</ul> : <p>No layers in this scene.</p>}</aside>
      <section className="panel ts-canvas" aria-label="Scene canvas"><div className="ts-toolbar"><h3>{scene.name}</h3><small>{scene.resolution.width} × {scene.resolution.height}</small></div><div className="ts-artboard" style={{ aspectRatio: `${scene.resolution.width} / ${scene.resolution.height}` }}><strong>{scene.name}</strong>{scene.nodes.length ? scene.nodes.map(node => <span key={node.id}>{node.name}</span>) : <span>Blank scene</span>}</div><p>Scene overview. Canvas manipulation and native frontend preview will arrive in later increments.</p></section>
      <aside className="panel ts-inspector" aria-label="Properties inspector"><h3>Project properties</h3>
        <label>Name<input maxLength={120} value={document.metadata.name} onChange={e => metadata('name', e.target.value)} /></label>
        <label>Author<input value={document.metadata.author} onChange={e => metadata('author', e.target.value)} /></label>
        <label>Description<textarea value={document.metadata.description} onChange={e => metadata('description', e.target.value)} /></label>
        <label>Project version<input maxLength={64} value={document.metadata.version} onChange={e => metadata('version', e.target.value)} /></label>
        <label>License<input maxLength={256} value={document.metadata.license} onChange={e => metadata('license', e.target.value)} /></label>
        <h3>Compatibility</h3><p>Unverified. No native exporter is enabled in this workspace.</p>
      </aside>
    </div>
    <details className="panel ts-timeline"><summary>Animation timeline</summary><p>This region is reserved for animation tracks, keyframes and media synchronization. Timeline editing is not implemented yet; existing source extension data is preserved.</p></details>
  </section>;
}
