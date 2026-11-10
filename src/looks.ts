// App looks (ADR-104): Classic (the original), Galactic and Bridge. The look is a
// data-look attribute on <html>; src/looks.css restyles colours, type and light for it.
// Remembered per PC in localStorage (a convenience; Classic when it can't be read).
export const LOOKS = [
  { id: 'classic', name: 'Classic' },
  { id: 'galactic', name: 'Galactic' },
  { id: 'bridge', name: 'Bridge' },
] as const;
export type LookId = typeof LOOKS[number]['id'];
const KEY = 'rd.look';

export function savedLook(): LookId {
  try { const v = localStorage.getItem(KEY); return LOOKS.some(l => l.id === v) ? v as LookId : 'classic'; } catch { return 'classic'; }
}

export function applyLook(id: LookId) {
  if (id === 'classic') delete document.documentElement.dataset.look; else document.documentElement.dataset.look = id;
  try { localStorage.setItem(KEY, id); } catch { /* not remembered */ }
}
