import { useEffect, useRef, useState, type MutableRefObject } from 'react';
import * as api from './theme-studio/api';
import type { ThemeProject } from './theme-studio/types';
import { ProjectSession } from './theme-studio/session';
import { errorText, suggestCopyName } from './theme-studio/document';
import { guardedWindowClose } from './theme-studio/window-close';
import NewProjectWizard from './theme-studio/NewProjectWizard';
import ProjectBrowser from './theme-studio/ProjectBrowser';
import ProjectOverview from './theme-studio/ProjectOverview';
import './theme-studio/theme-studio.css';

export type StudioLeaveGuard = MutableRefObject<null | (() => Promise<boolean>)>;

export default function ThemeStudio({ native, leaveGuard }: { native: boolean; leaveGuard: StudioLeaveGuard }) {
  const [projects, setProjects] = useState<api.ProjectSummary[]>([]);
  const [offset, setOffset] = useState(0);
  const [wizard, setWizard] = useState(false);
  const [session, setSessionState] = useState<ProjectSession | null>(null);
  const [recovery, setRecovery] = useState<api.ProjectRecord | null>(null);
  const [uneditable, setUneditable] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const sessionRef = useRef(session); sessionRef.current = session;
  const busyRef = useRef(false);
  const setSession = (next: ProjectSession | null) => { sessionRef.current = next; setSessionState(next); };
  const listSequence = useRef(0);
  const action = async (fn: () => Promise<void>) => { if (busyRef.current) return; busyRef.current = true; setBusy(true); setError(''); setNotice(''); try { await fn(); } catch (e) { setError(errorText(e)); } finally { busyRef.current = false; setBusy(false); } };
  const refresh = async (page = offset) => {
    const sequence = ++listSequence.current;
    const rows = await api.listProjects(50, page);
    if (sequence === listSequence.current) { setProjects(rows); setOffset(page); }
  };
  useEffect(() => { if (native) void action(() => refresh(0)); }, [native]);
  useEffect(() => {
    const flush = async () => { if (busyRef.current) { setError('Please wait for the current project operation before leaving.'); return false; } try { await sessionRef.current?.flush(); return true; } catch (e) { setError(`Could not leave Theme Studio: ${errorText(e)}`); return false; } };
    leaveGuard.current = flush;
    const unload = (event: BeforeUnloadEvent) => { const s = sessionRef.current; if (s && (s.dirty || s.busy || s.error)) { event.preventDefault(); event.returnValue = ''; } };
    window.addEventListener('beforeunload', unload);
    let disposed = false;
    let unlisten: (() => void) | undefined;
    if (native) void import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
      const appWindow = getCurrentWindow();
      const stop = await appWindow.onCloseRequested(guardedWindowClose(flush, e => setError(errorText(e))));
      if (disposed) stop(); else unlisten = stop;
    }).catch(e => { if (!disposed) setError(`Window close protection could not start: ${errorText(e)}`); });
    return () => { disposed = true; leaveGuard.current = null; unlisten?.(); window.removeEventListener('beforeunload', unload); };
  }, [native, leaveGuard]);

  const attach = async (record: api.ProjectRecord, json: string) => {
    try {
      const document = await api.inspectProject(json);
      setSession(new ProjectSession(record, document, api.writeProject)); setRecovery(null); setUneditable(null); setWizard(false);
    } catch (e) { setUneditable(json); throw e; }
  };
  const open = (id: string) => action(async () => {
    await sessionRef.current?.flush();
    const record = await api.openProject(id);
    setSession(null); setWizard(false); setUneditable(null);
    if (record.draftJson) setRecovery(record); else await attach(record, record.savedJson);
  });
  const create = (project: ThemeProject) => action(async () => { const record = await api.createProject(JSON.stringify(project)); await attach(record, record.savedJson); });
  const close = () => action(async () => { await sessionRef.current?.flush(); setSession(null); await refresh(); });
  const copy = (name: string) => action(async () => {
    const current = sessionRef.current; if (!current) return;
    const project = { ...current.document, metadata: { ...current.document.metadata, name } };
    const record = await api.createProject(JSON.stringify(project)); await attach(record, record.savedJson);
    setNotice(`Created a separate project: “${record.name}”.`);
  });
  const reload = () => action(async () => {
    const current = sessionRef.current; if (!current) return;
    const record = await api.openProject(current.record.id); setSession(null); setUneditable(null);
    if (record.draftJson) setRecovery(record); else await attach(record, record.savedJson);
  });

  return <div className="theme-studio" aria-busy={busy}>
    {notice && <div className="panel" role="status"><p>{notice}</p></div>}
    {error && <div className="panel" role="alert"><p>{error}</p><button className="ghost" onClick={() => setError('')}>Dismiss message</button></div>}
    {!native ? <section className="panel"><h2>Theme Studio</h2><p>Open RetroDrive's desktop app to create and save local projects. No emulator installation or cloud account is required.</p></section> : <>
      {uneditable !== null && <section className="panel"><h2>Source kept unchanged</h2><p>This document cannot be opened for editing by this build. The stored source has not been replaced.</p><details><summary>View original source</summary><pre>{uneditable}</pre></details><button onClick={() => { setUneditable(null); setRecovery(null); }}>Back to projects</button></section>}
      {uneditable === null && recovery && <section className="panel" aria-labelledby="recovery-title"><h2 id="recovery-title">Resume {recovery.name}</h2><p>An autosaved draft is available. Recover it to keep editing, or discard only the draft and return to the saved checkpoint.</p><button disabled={busy} onClick={() => { void action(async () => { const draft = await api.recoverProject(recovery.id, recovery.revision); await attach(recovery, draft.documentJson); }); }}>Recover draft</button><button disabled={busy} onClick={() => { void action(async () => { const record = await api.discardProjectDraft(recovery.id, recovery.revision); await attach(record, record.savedJson); }); }}>Discard draft and open checkpoint</button><button disabled={busy} className="ghost" onClick={() => setRecovery(null)}>Back</button></section>}
      {uneditable === null && !recovery && (session ? <fieldset className="ts-session" disabled={busy}><legend className="ts-sr-only">Project workspace</legend><ProjectOverview key={session.record.id} session={session} onClose={() => { void close(); }} onCopy={name => { void copy(name); }} onReload={() => { void reload(); }} /></fieldset> : wizard ? <NewProjectWizard busy={busy} onCreate={project => { void create(project); }} onCancel={() => setWizard(false)} /> : <ProjectBrowser projects={projects} busy={busy} offset={offset} onNew={() => setWizard(true)} onOpen={id => { void open(id); }} onDuplicate={id => { void action(async () => { const record = await api.openProject(id); const duplicate = await api.duplicateProject(id, record.revision, suggestCopyName(record.name, projects.map(project => project.name)), false); await attach(duplicate, duplicate.savedJson); setNotice(`Created a separate project: ${duplicate.name}.`); }); }} onPage={page => { void action(() => refresh(page)); }} onRefresh={() => { void action(() => refresh()); }} />)}
    </>}
  </div>;
}
