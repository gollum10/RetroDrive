import { invoke } from '@tauri-apps/api/core';

/** Raw source is returned intact, including a schema this build cannot edit. */
export type ProjectRecord = {
  id: string; name: string; schemaVersion: string; savedJson: string; draftJson: string | null;
  revision: number; createdAt: string; updatedAt: string; savedAt: string;
};
export type ProjectSummary = Pick<ProjectRecord, 'id' | 'name' | 'schemaVersion' | 'revision' | 'updatedAt'> & { hasDraft: boolean };
export type ProjectError = { code: string; message: string };
export type RecoveredDraft = { id: string; documentJson: string; revision: number };

// Deliberately no automatic conflict retries: a stale editor must reopen/reconcile.
export const listProjects = (limit = 50, offset = 0) => invoke<ProjectSummary[]>('list_theme_projects', { limit, offset });
export const openProject = (id: string) => invoke<ProjectRecord>('open_theme_project', { id });
export const createProject = (documentJson: string) => invoke<ProjectRecord>('create_theme_project', { documentJson });
export const writeProject = (id: string, revision: number, documentJson: string, kind: 'autosave' | 'checkpoint') =>
  invoke<ProjectRecord>('write_theme_project', { id, revision, documentJson, kind });
export const renameProject = (id: string, revision: number, name: string) =>
  invoke<ProjectRecord>('rename_theme_project', { id, revision, name });
export const duplicateProject = (id: string, revision: number, name: string, useDraft: boolean) =>
  invoke<ProjectRecord>('duplicate_theme_project', { id, revision, name, useDraft });
export const recoverProject = (id: string, revision: number) => invoke<RecoveredDraft>('recover_theme_project', { id, revision });
export const discardProjectDraft = (id: string, revision: number) => invoke<ProjectRecord>('discard_theme_project_draft', { id, revision });
