import type { ThemeProject } from './types';
import type { ProjectRecord } from './api';

export type Writer = (id: string, revision: number, json: string, kind: 'autosave' | 'checkpoint') => Promise<ProjectRecord>;

/** One serialized write queue per open project. React and navigation share this owner. */
export class ProjectSession {
  private listeners = new Set<() => void>();
  private tail: Promise<void> = Promise.resolve();
  private generation = 0;
  private persisted = 0;
  private changes = 0;
  private pending = 0;
  error: unknown = null;
  constructor(public record: ProjectRecord, public document: ThemeProject, private writer: Writer) {}
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  snapshot = () => this.changes;
  get dirty() { return this.generation > this.persisted; }
  get busy() { return this.pending > 0; }
  get conflict() { return !!this.error && typeof this.error === 'object' && 'code' in this.error && this.error.code === 'conflict'; }
  private emit() { this.changes++; this.listeners.forEach(listener => listener()); }
  edit(document: ThemeProject) { this.document = document; this.generation++; this.emit(); }
  private enqueue(kind: 'autosave' | 'checkpoint'): Promise<void> {
    const generation = this.generation;
    const json = JSON.stringify(this.document);
    this.pending++; this.emit();
    const operation = this.tail.then(async () => {
      try {
        if (this.error) throw this.error;
        this.record = await this.writer(this.record.id, this.record.revision, json, kind);
        this.persisted = Math.max(this.persisted, generation);
        // Never replace edits made while this request was in flight.
        if (this.generation === generation) this.document = JSON.parse(this.record.draftJson ?? this.record.savedJson) as ThemeProject;
      } catch (error) { this.error = error; throw error; }
      finally { this.pending--; this.emit(); }
    });
    this.tail = operation.catch(() => undefined);
    return operation;
  }
  async flush(): Promise<void> {
    await this.tail;
    if (this.error) throw this.error;
    while (this.dirty) await this.enqueue('autosave');
  }
  async save(): Promise<void> {
    if (this.error) throw this.error;
    await this.enqueue('checkpoint');
    await this.flush();
  }
  async retry(): Promise<void> {
    if (this.conflict) throw this.error;
    this.error = null; this.emit();
    await this.flush();
  }
}
