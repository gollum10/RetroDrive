import type { ProjectSummary } from './api';

export default function ProjectBrowser({ projects, busy, offset, onNew, onOpen, onDuplicate, onPage, onRefresh }: {
  projects: ProjectSummary[]; busy: boolean; offset: number; onNew: () => void; onOpen: (id: string) => void;
  onDuplicate: (id: string) => void; onPage: (offset: number) => void; onRefresh: () => void;
}) {
  return <section className="panel" aria-labelledby="theme-projects-title"><div className="ts-toolbar"><h2 id="theme-projects-title">Your theme projects</h2><button className="primary" disabled={busy} onClick={onNew}>New theme</button><button className="ghost" disabled={busy} onClick={onRefresh}>Refresh projects</button></div>
    <p>Create and edit offline. Projects are independent of installed themes and Customize designs.</p>
    {!busy && projects.length === 0 && <p>{offset ? 'No more projects on this page.' : 'No projects yet. Start with blank scenes or an original starter layout.'}</p>}
    {busy && <p role="status">Loading project…</p>}
    <div className="ts-projects">{projects.map(project => <article className="ts-project" key={project.id}><h3>{project.name}</h3><p>{project.schemaVersion === '1.0.0' ? 'Theme project' : 'Requires a compatible app version'}</p><p>{project.hasDraft ? 'Recoverable draft' : 'Saved checkpoint'}</p><small>Last edited: {project.updatedAt} UTC</small><div className="actions"><button disabled={busy} onClick={() => onOpen(project.id)}>{project.hasDraft ? 'Resume project' : 'Open project'}</button><button className="ghost" title="Create and open another project from this saved checkpoint" disabled={busy} onClick={() => onDuplicate(project.id)}>Duplicate saved</button></div></article>)}</div>
    <nav className="actions" aria-label="Project pages"><button disabled={busy || offset === 0} onClick={() => onPage(Math.max(0, offset - 50))}>Previous</button><span>Page {Math.floor(offset / 50) + 1}</span><button disabled={busy || projects.length < 50} onClick={() => onPage(offset + 50)}>Next</button></nav>
  </section>;
}
