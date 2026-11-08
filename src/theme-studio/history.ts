import type { ThemeProject } from './types';

type Entry = { json: string; label: string };
/** Session-only history. Source snapshots preserve opaque fields; storage checkpoints are separate. */
export class EditHistory {
  private past: Entry[] = [];
  private future: Entry[] = [];
  private group: string | undefined;
  constructor(private maxEntries = 50, private maxCharacters = 8 * 1024 * 1024) {}
  get canUndo() { return this.past.length > 0; }
  get canRedo() { return this.future.length > 0; }
  get undoLabel() { return this.past[this.past.length - 1]?.label; }
  endGroup() { this.group = undefined; }
  record(before: ThemeProject, after: ThemeProject, label: string, group?: string): boolean {
    const json = JSON.stringify(before);
    if (json === JSON.stringify(after)) return false;
    if (!group || group !== this.group) this.past.push({ json, label });
    this.group = group; this.future = []; this.trim(); return true;
  }
  undo(current: ThemeProject): ThemeProject | null {
    const entry = this.past.pop(); if (!entry) return null;
    this.future.push({ json: JSON.stringify(current), label: entry.label });
    this.endGroup(); this.trim(); return JSON.parse(entry.json) as ThemeProject;
  }
  redo(current: ThemeProject): ThemeProject | null {
    const entry = this.future.pop(); if (!entry) return null;
    this.past.push({ json: JSON.stringify(current), label: entry.label });
    this.endGroup(); this.trim(); return JSON.parse(entry.json) as ThemeProject;
  }
  private trim() {
    const characters = () => [...this.past, ...this.future].reduce((sum, entry) => sum + entry.json.length, 0);
    while (this.past.length + this.future.length > this.maxEntries || characters() > this.maxCharacters) {
      if (this.past.length) this.past.shift(); else if (this.future.length) this.future.shift(); else break;
    }
  }
}
