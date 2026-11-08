import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import type { ProjectSession } from './session';
import { errorText, suggestCopyName } from './document';
import CompatibilityPanel from './CompatibilityPanel';
import SaveCopyForm from './SaveCopyForm';
import SceneCanvas from './SceneCanvas';
import NodeInspector from './NodeInspector';
import LayerOrderControls from './LayerOrderControls';
import { addNode, moveNode, updateNode, orderLayer } from './scene-edit';

export default function ProjectOverview({ session, onClose, onCopy, onReload, blocked = false }: { session: ProjectSession; onClose: () => void; onCopy: (name: string) => void; onReload: () => void; blocked?: boolean }) {
  const change = useSyncExternalStore(session.subscribe, session.snapshot, session.snapshot);
  const [sceneId, setSceneId] = useState(session.document.scenes[0].id);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [copyName, setCopyName] = useState<string | null>(null);
  const copyButton = useRef<HTMLButtonElement>(null);
  const [confirmReload, setConfirmReload] = useState(false);
  const document = session.document;
  const scene = document.scenes.find(s => s.id === sceneId) ?? document.scenes[0];
  const selected = scene.nodes.find(node => node.id === selectedId) ?? null;
  const editorDisabled = blocked || copyName !== null;
  const nodeCount = document.scenes.reduce((count, scene) => count + scene.nodes.length, 0);
  const add = (kind: 'text' | 'panel') => { const next = addNode(session.document, scene.id, kind); if (next.id) { session.edit(next.project, `Add ${kind}`); setSelectedId(next.id); } };
  useEffect(() => {
    if (!session.dirty || session.error || session.busy) return;
    const timer = window.setTimeout(() => { void session.flush().catch(() => undefined); }, 500);
    return () => window.clearTimeout(timer);
  }, [session, change]);
  const status = session.error ? session.conflict ? 'Conflict — local edits kept' : 'Save failed — local edits kept' : session.busy ? 'Saving…' : session.dirty ? 'Unsaved edits' : session.record.draftJson ? 'Autosaved draft' : 'Saved checkpoint';
  const metadata = (key: string, value: string) => session.edit({ ...document, metadata: { ...document.metadata, [key]: value } }, `Edit project ${key}`, `metadata:${key}`);
  return <section className="ts-editor" aria-label="Theme Studio editor" onKeyDown={event => {
    if (editorDisabled || event.altKey || !(event.ctrlKey || event.metaKey) || (event.target as Element).closest('input, textarea, select, [contenteditable="true"]')) return;
    if (event.key.toLowerCase() === 'z') { event.preventDefault(); if (event.shiftKey) session.redo(); else session.undo(); }
    else if (event.key.toLowerCase() === 'y') { event.preventDefault(); session.redo(); }
  }}>
    <div className="panel ts-toolbar"><h2>{document.metadata.name || 'Untitled theme'}</h2><span role="status" aria-live="polite">{status}</span><button className="primary" disabled={session.busy || !!session.error || copyName !== null} onClick={() => { void session.save().catch(() => undefined); }}>Save checkpoint</button><button ref={copyButton} aria-expanded={copyName !== null} aria-controls="ts-save-copy" disabled={session.busy} onClick={() => setCopyName(suggestCopyName(document.metadata.name))}>Save As…</button><button className="ghost" onClick={onClose}>Back to projects</button></div>
    {session.error != null && <div className="panel" role="alert"><p>{errorText(session.error)}</p>{!session.conflict && <button onClick={() => { void session.retry().catch(() => undefined); }}>Retry autosave</button>}<p>Save As keeps your current local edits in a separate project. Reload replaces this session with the database version.</p><button className="ghost" onClick={() => setConfirmReload(true)}>Reload from database…</button></div>}
    {confirmReload && <div className="panel" role="alert"><p>Discard this session's unsaved edits and reopen the database version? Save a copy first to keep these edits.</p><button onClick={onReload}>Discard local edits and reload</button><button className="ghost" onClick={() => setConfirmReload(false)}>Keep editing</button></div>}
    {copyName !== null && <SaveCopyForm name={copyName} sourceName={document.metadata.name} busy={session.busy} onName={setCopyName} onCreate={onCopy} onCancel={() => { setCopyName(null); copyButton.current?.focus(); }} />}
    <p>Edits autosave as a draft. Save checkpoint marks a version to return to. Target: {document.targets.length ? document.targets.map(t => `${t.frontendFamily}${t.frontendVersion ? ` ${t.frontendVersion}` : ' (version unknown)'}`).join(', ') : 'Universal / No Target Yet'}. Native support is unverified.</p>
    <div className="ts-toolbar ts-edit-actions"><button disabled={editorDisabled || !session.history.canUndo} title={session.history.undoLabel ?? 'Nothing to undo'} onClick={() => session.undo()}>Undo</button><button disabled={editorDisabled || !session.history.canRedo} onClick={() => session.redo()}>Redo</button><button disabled={editorDisabled || nodeCount >= 10_000} onClick={() => add('text')}>Add text</button><button disabled={editorDisabled || nodeCount >= 10_000} onClick={() => add('panel')}>Add panel</button></div>
    <div className="ts-editor-grid">
      <aside className="panel ts-scenes" aria-label="Scenes and layers"><h3>Scenes</h3><nav aria-label="Select scene">{document.scenes.map(s => <button key={s.id} aria-pressed={scene.id === s.id} onClick={() => { setSceneId(s.id); setSelectedId(null); session.endEditGroup(); }}>{s.name}</button>)}</nav><h3>Layers</h3><small>Back to front · nested layers are read-only</small>{scene.nodes.length ? <nav aria-label="Select layer">{scene.nodes.map(node => <button key={node.id} aria-pressed={selectedId === node.id} onClick={() => { setSelectedId(node.id); session.endEditGroup(); }}>{node.name || node.type} <small>({node.type})</small></button>)}</nav> : <p>No layers in this scene.</p>}</aside>
      <SceneCanvas scene={scene} selected={selectedId} disabled={editorDisabled} onSelect={setSelectedId} onMove={(id, x, y, group) => session.edit(moveNode(session.document, scene.id, id, x, y), 'Move layer', group)} onEndGroup={() => session.endEditGroup()}/>
      <aside className="panel ts-inspector" aria-label="Properties inspector"><h3>Properties</h3>
        <LayerOrderControls scene={scene} selected={selectedId} disabled={editorDisabled} onOrder={order => { if (selectedId) session.edit(orderLayer(session.document, scene.id, selectedId, order), 'Change layer order'); }}/>
        <NodeInspector scene={scene} node={selected} disabled={editorDisabled} onChange={(node, label, group) => session.edit(updateNode(session.document, scene.id, node.id, () => node), label, group)} onEnd={() => session.endEditGroup()}/>
        <h3>Project properties</h3>
        <fieldset className="ts-property-fields" disabled={editorDisabled} onBlur={() => session.endEditGroup()}><legend className="ts-sr-only">Edit current project</legend>
        <label>Name<input maxLength={120} value={document.metadata.name} onChange={e => metadata('name', e.target.value)} /></label>
        <label>Author<input value={document.metadata.author} onChange={e => metadata('author', e.target.value)} /></label>
        <label>Description<textarea value={document.metadata.description} onChange={e => metadata('description', e.target.value)} /></label>
        <label>Project version<input maxLength={64} value={document.metadata.version} onChange={e => metadata('version', e.target.value)} /></label>
        <label>License<input maxLength={256} value={document.metadata.license} onChange={e => metadata('license', e.target.value)} /></label>
        </fieldset>
        <p className="ts-field-help">These fields edit the current project. Use Save As to create another project.</p>
        <CompatibilityPanel document={document}/>
      </aside>
    </div>
    <details className="panel ts-timeline"><summary>Animation timeline</summary><p>This region is reserved for animation tracks, keyframes and media synchronization. Timeline editing is not implemented yet; existing source extension data is preserved.</p></details>
  </section>;
}
