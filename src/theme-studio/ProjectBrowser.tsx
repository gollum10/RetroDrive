import type { ProjectSummary } from './api';

export default function ProjectBrowser({ projects, busy, offset, onNew, onOpen, onDuplicate, onPage, onRefresh }: {
  projects: ProjectSummary[]; busy: boolean; offset: number; onNew: () => void; onOpen: (id: string) => void;
  onDuplicate: (id: string) => void; onPage: (offset: number) => void; onRefresh: () => void;
}) {
  return <section className="panel" aria-labelledby="theme-projects-title"><div className="ts-toolbar"><h2 id="theme-projects-title">Your themes</h2><button className="primary" disabled={busy} onClick={onNew}>New theme</button><button className="ghost" disabled={busy} onClick={onRefresh}>Refresh list</button></div>
    <p>Design your own themes here, with your own art. Everything stays on this PC, and nothing changes on your systems or in Customize while you design.</p>
    {!busy && projects.length === 0 && <p>{offset ? 'No more themes on this page.' : 'No themes yet. Choose New theme to start from blank screens or a simple starter.'}</p>}
    {busy && <p role="status">Loading project…</p>}
    <div className="ts-projects">{projects.map(project => <article className="ts-project" key={project.id}><h3>{project.name}</h3><p>{project.schemaVersion === '1.0.0' ? 'Theme' : 'Needs a newer RetroDrive to open'}</p><p>{project.hasDraft ? 'Has changes not saved as a version' : 'Saved'}</p><small>Last edited: {project.updatedAt} UTC</small><div className="actions"><button disabled={busy} onClick={() => onOpen(project.id)}>{project.hasDraft ? 'Carry on' : 'Open'}</button><button className="ghost" title="Make a separate copy from its last saved version" disabled={busy} onClick={() => onDuplicate(project.id)}>Make a copy</button></div></article>)}</div>
    <nav className="actions" aria-label="Theme pages"><button disabled={busy || offset === 0} onClick={() => onPage(Math.max(0, offset - 50))}>Previous</button><span>Page {Math.floor(offset / 50) + 1}</span><button disabled={busy || projects.length < 50} onClick={() => onPage(offset + 50)}>Next</button></nav>
  </section>;
}
