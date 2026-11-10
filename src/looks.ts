// App looks (ADR-104): Classic (the original), Galactic, Bridge, Cartoon, Versus and 80s (ADR-123). The look is a
// data-look attribute on <html>; src/looks.css restyles colours, type and light for it.
// Remembered per PC in localStorage (a convenience; Classic when it can't be read).
export const LOOKS = [
  { id: 'classic', name: 'Classic' },
  { id: 'galactic', name: 'Galactic' },
  { id: 'bridge', name: 'Bridge' },
  { id: 'cartoon', name: 'Cartoon' },
  { id: 'versus', name: 'Versus' },
  { id: 'eighties', name: '80s' },
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
