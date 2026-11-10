import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import type { ProjectSession } from './session';
import { errorText, suggestCopyName } from './document';
import CompatibilityPanel from './CompatibilityPanel';
import ImageImportForm from './ImageImportForm';
import { attachImage } from './image-assets';
import { importPng } from './api';
import useImagePreviews from './use-image-previews';
import SaveCopyForm from './SaveCopyForm';
import SceneCanvas from './SceneCanvas';
import ImageControls from './ImageControls';
import NodeInspector from './NodeInspector';
import LayerList from './LayerList';
import LayerOrderControls from './LayerOrderControls';
import AlignmentControls from './AlignmentControls';
import { alignLayer } from './alignment';
import { resizeLayer } from './resize';
import { addNode, moveNode, updateNode, orderLayer, setLayerState, layerState, deleteNode, editableNode } from './scene-edit';
import { LEVELS, savedLevel, saveLevel, shows, type Level, type Tool } from './skill';

export default function ProjectOverview({ session, onClose, onCopy, onReload, blocked = false }: { session: ProjectSession; onClose: () => void; onCopy: (name: string) => void; onReload: () => void; blocked?: boolean }) {
  const change = useSyncExternalStore(session.subscribe, session.snapshot, session.snapshot);
  const [sceneId, setSceneId] = useState(session.document.scenes[0].id);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [copyName, setCopyName] = useState<string | null>(null);
  const [importScene,setImportScene]=useState<string|null>(null);
  const [importPlacement,setImportPlacement]=useState<'free'|'over'>('free');
  const alive=useRef(true);
  useEffect(()=>{alive.current=true;return()=>{alive.current=false;};},[]);
  const copyButton = useRef<HTMLButtonElement>(null);
  const [confirmReload, setConfirmReload] = useState(false);
  const [level, setLevelState] = useState<Level>(savedLevel);
  const setLevel = (next: Level) => { setLevelState(next); saveLevel(next); };
  const can = (tool: Tool) => shows(level, tool);
  const document = session.document;
  const scene = document.scenes.find(s => s.id === sceneId) ?? document.scenes[0];
  const selected = scene.nodes.find(node => node.id === selectedId) ?? null;
  const editorDisabled = blocked || copyName !== null || importScene !== null;
  const sceneAssetIds=new Set(scene.nodes.filter(node=>node.type==='image'&&!layerState(node).hidden).flatMap(node=>node.assets));
  const sceneAssets=document.assets.filter(asset=>sceneAssetIds.has(asset.id));
  const previews=useImagePreviews(session.record.id,sceneAssets,session.record.revision);
  const nodeCount = document.scenes.reduce((count, scene) => count + scene.nodes.length, 0);
  const removable = selected !== null && editableNode(scene, selected);
  const remove = () => { if (!editorDisabled && selected && removable) { session.edit(deleteNode(session.document, scene.id, selected.id), 'Delete layer'); setSelectedId(null); } };
  const add = (kind: 'text' | 'panel' | 'video') => { const next = addNode(session.document, scene.id, kind); if (next.id) { session.edit(next.project, kind === 'video' ? 'Add game video' : `Add ${kind}`); setSelectedId(next.id); } };
  useEffect(() => {
    if (!session.dirty || session.error || session.busy) return;
    const timer = window.setTimeout(() => { void session.flush().catch(() => undefined); }, 500);
    return () => window.clearTimeout(timer);
  }, [session, change]);
  const status = session.error ? session.conflict ? 'Changed elsewhere: your edits are kept here' : "Couldn't save: your edits are kept here" : session.busy ? 'Saving…' : session.dirty ? 'Saving soon…' : session.record.draftJson ? 'Changes kept' : 'Saved';
  const metadata = (key: string, value: string) => session.edit({ ...document, metadata: { ...document.metadata, [key]: value } }, `Edit project ${key}`, `metadata:${key}`);
  return <section className="ts-editor" aria-label="Theme Studio editor" onKeyDown={event => {
    if (editorDisabled || (event.target as Element).closest('input, textarea, select, [contenteditable="true"]')) return;
    if (event.key === 'Delete' && !event.ctrlKey && !event.metaKey && !event.altKey && removable) { event.preventDefault(); remove(); return; }
    if (event.altKey || !(event.ctrlKey || event.metaKey)) return;
    if (event.key.toLowerCase() === 'z') { event.preventDefault(); if (event.shiftKey) session.redo(); else session.undo(); }
    else if (event.key.toLowerCase() === 'y') { event.preventDefault(); session.redo(); }
  }}>
    <div className="panel ts-toolbar"><h2>{document.metadata.name || 'Untitled theme'}</h2><span role="status" aria-live="polite">{status}</span><button className="primary" disabled={session.busy || !!session.error || copyName !== null} onClick={() => { void session.save().catch(() => undefined); }} title="Save this as a version you can go back to">Save version</button><button ref={copyButton} aria-expanded={copyName !== null} aria-controls="ts-save-copy" disabled={session.busy || importScene !== null} onClick={() => setCopyName(suggestCopyName(document.metadata.name))}>Save a copy…</button><button className="ghost" onClick={onClose}>Back to my themes</button></div>
    {session.error != null && <div className="panel" role="alert"><p>{errorText(session.error)}</p>{!session.conflict && <button onClick={() => { void session.retry().catch(() => undefined); }}>Try saving again</button>}<p>Save a copy keeps your edits in a separate theme. Reload opens the last saved state instead.</p><button className="ghost" onClick={() => setConfirmReload(true)}>Reload the last saved state…</button></div>}
    {confirmReload && <div className="panel" role="alert"><p>Drop the edits you just made and reopen the last saved state? Save a copy first if you want to keep them.</p><button onClick={onReload}>Drop my edits and reload</button><button className="ghost" onClick={() => setConfirmReload(false)}>Keep editing</button></div>}
    {copyName !== null && <SaveCopyForm name={copyName} sourceName={document.metadata.name} busy={session.busy} onName={setCopyName} onCreate={onCopy} onCancel={() => { setCopyName(null); copyButton.current?.focus(); }} />}
    {importScene && <ImageImportForm sceneName={document.scenes.find(scene=>scene.id===importScene)?.name??'scene'} overName={removable && importScene === scene.id ? selected!.name : undefined} initialPlacement={importPlacement} onCancel={()=>setImportScene(null)} onImport={async(encoded,license,attribution,options)=>{await session.flush();const image=await importPng(session.record.id,encoded);if(!alive.current)return;const next=attachImage(session.document,importScene,image,license,attribution,{ ...options, overId: selectedId });session.edit(next.project,options?.placement==='background'?'Add background picture':options?.placement==='over'?'Add frame':'Add picture');setSelectedId(next.id);setImportScene(null);await session.flush();}}/>}
    {previews.error && <p role="status">{previews.error} <button onClick={previews.retry}>Retry previews</button></p>}
    <div className="ts-levels" role="radiogroup" aria-label="Tools"><span>Tools</span>{LEVELS.map(l => <button key={l.id} type="button" role="radio" aria-checked={level === l.id} className={level === l.id ? 'on' : ''} title={l.hint} onClick={() => setLevel(l.id)}>{l.name}</button>)}<small>{LEVELS.find(l => l.id === level)?.hint}</small></div>
    <p className="ts-field-help">Your changes are kept automatically; <b>Save version</b> marks a point you can go back to. Made for: {document.targets.length ? document.targets.map(t => `${t.frontendFamily}${t.frontendVersion ? ` ${t.frontendVersion}` : ''}`).join(', ') : 'any system (choose later)'}.</p>
    <div className="ts-toolbar ts-edit-actions"><button disabled={editorDisabled || nodeCount >= 10000} onClick={()=>{setImportPlacement('free');setImportScene(scene.id);}}>Add picture</button><button disabled={editorDisabled || !session.history.canUndo} title={session.history.undoLabel ?? 'Nothing to undo'} onClick={() => session.undo()}>Undo</button><button disabled={editorDisabled || !session.history.canRedo} onClick={() => session.redo()}>Redo</button><button disabled={editorDisabled || !removable} title="Delete the selected layer (Delete key). Undo brings it back." onClick={remove}>Delete layer</button><button disabled={editorDisabled || nodeCount >= 10_000} onClick={() => add('text')}>Add text</button><button disabled={editorDisabled || nodeCount >= 10_000} title="A space where the system plays the selected game's video" onClick={() => add('video')}>Add game video</button>{can('panel') && <button disabled={editorDisabled || nodeCount >= 10_000} onClick={() => add('panel')}>Add panel</button>}</div>
    <nav className="ts-screens" aria-label="Select screen"><span>Screens</span>{document.scenes.map(s => <button key={s.id} aria-pressed={scene.id === s.id} onClick={() => { setSceneId(s.id); setSelectedId(null); session.endEditGroup(); }}>{s.name}</button>)}</nav>
    <div className="ts-editor-grid">
      <aside className="panel ts-layers" aria-label="Layer list"><LayerList scene={scene} selected={selectedId} disabled={editorDisabled} onSelect={id => { setSelectedId(id); session.endEditGroup(); }} onState={(id,key,value) => session.edit(setLayerState(session.document,scene.id,id,key,value), `${value ? key === 'locked' ? 'Lock' : 'Hide' : key === 'locked' ? 'Unlock' : 'Show'} layer`)}/></aside>
      <SceneCanvas previewImages={previews.images} scene={scene} selected={selectedId} disabled={editorDisabled} onSelect={setSelectedId} onMove={(id, x, y, group) => session.edit(moveNode(session.document, scene.id, id, x, y), 'Move layer', group)} onResize={(id, width, height, group) => session.edit(resizeLayer(session.document, scene.id, id, width, height), 'Resize layer', group)} onEndGroup={() => session.endEditGroup()}/>
      <aside className="panel ts-inspector" aria-label="Properties inspector"><h3>Properties</h3>
        {can('order') && <LayerOrderControls scene={scene} selected={selectedId} disabled={editorDisabled} onOrder={order => { if (selectedId) session.edit(orderLayer(session.document, scene.id, selectedId, order), 'Change layer order'); }}/>}
        {selected?.type==='image'&&<ImageControls key={selected.id} scene={scene} node={selected} asset={document.assets.find(asset=>asset.id===selected.assets[0])} url={previews.images[selected.assets[0]]} showCrop={can('crop')} disabled={editorDisabled} onChange={(node,label,group)=>session.edit(updateNode(session.document,scene.id,node.id,()=>node),label,group)} onEnd={()=>session.endEditGroup()}/>}
        {selected?.type === 'video' && <section aria-label="Game video"><h3>Game video</h3><p className="ts-field-help">On the system this space plays the video of whichever game is selected. For an arcade-cabinet look, put a picture with a see-through screen over it.</p><button disabled={editorDisabled || !removable} onClick={() => { setImportPlacement('over'); setImportScene(scene.id); }}>Add a frame over it</button></section>}
        <NodeInspector scene={scene} node={selected} tools={{ numbers: can('numbers'), units: can('units'), transform: can('transform'), layerName: can('layerName') }} disabled={editorDisabled} onChange={(node, label, group) => session.edit(updateNode(session.document, scene.id, node.id, () => node), label, group)} onEnd={() => session.endEditGroup()}/>
        <AlignmentControls scene={scene} node={selected} disabled={editorDisabled} onAlign={alignment => { if (selectedId) session.edit(alignLayer(session.document, scene.id, selectedId, alignment), 'Align layer'); }}/>
        <h3>Theme details</h3>
        <fieldset className="ts-property-fields" disabled={editorDisabled} onBlur={() => session.endEditGroup()}><legend className="ts-sr-only">Edit theme details</legend>
        <label>Name<input maxLength={120} value={document.metadata.name} onChange={e => metadata('name', e.target.value)} /></label>
        {can('details') && <><label>Author<input value={document.metadata.author} onChange={e => metadata('author', e.target.value)} /></label>
        <label>Description<textarea value={document.metadata.description} onChange={e => metadata('description', e.target.value)} /></label>
        <label>Version<input maxLength={64} value={document.metadata.version} onChange={e => metadata('version', e.target.value)} /></label>
        <label>License<input maxLength={256} value={document.metadata.license} onChange={e => metadata('license', e.target.value)} /></label></>}
        </fieldset>
        {can('check') && <CompatibilityPanel document={document}/>}
      </aside>
    </div>
    {can('timeline') && <details className="panel ts-timeline"><summary>Animation</summary><p>Animation (moving and fading layers over time) is coming later. Nothing here changes your theme yet.</p></details>}
  </section>;
}
