// Games that need (or have) a decision, kept out of the way (ADR-044): one short line
// under the plan summary, and a review panel the user opens only if they want to.
import { useEffect, useMemo, useRef, useState } from 'react';
import type { Choice, DeploymentPlan, PlanGame } from './App';
import { fmtSize, shortPath } from './format';

export type KeptOffChange = { destination: string; title: string; kept: boolean };
type Kind = 'versions' | 'on_drive' | 'removed' | 'same_name' | 'cant_copy';
type Row = { key: string; title: string; platform: string; games: PlanGame[] };

const ROWS = 20;

const kindText: Record<Kind, { tab: string; about: string }> = {
  versions: { tab: 'Two versions in your library', about: 'Your library has two different files with the same name, often in two folders (like “roms” and “roms II”). They’re skipped until you pick which one to copy.' },
  on_drive: { tab: 'Different file on the drive', about: 'The drive already has a different file with this name. It’s kept unless you choose otherwise. Replacing first moves the drive’s file to .retrodrive\\backup, and is chosen one game at a time.' },
  removed: { tab: 'Removed from the drive', about: 'RetroDrive copied these before, but they’re no longer on the drive. They aren’t copied back unless you say so.' },
  same_name: { tab: 'Already there under another name', about: 'The same game is already on the drive (or being copied) under a different file name, so these aren’t copied.' },
  cant_copy: { tab: 'Can’t be copied', about: 'These can’t be copied as they are. The reason is shown for each.' },
};

/** The folder that tells two versions apart: the first part of their paths that differs. */
function folderLabels(games: PlanGame[]): string[] {
  const parts = games.map(g => g.source.split(/[\\/]/));
  const at = parts[0].findIndex((_, i) => parts.some(p => p[i] !== parts[0][i]));
  return games.map((g, n) => (at >= 0 && parts[n][at]) || shortPath(g.source));
}

function rows(list: PlanGame[]): Row[] {
  const out = new Map<string, Row>();
  for (const g of list) {
    const key = g.choice?.key ?? g.source;
    const row = out.get(key) ?? { key, title: g.title, platform: g.platform, games: [] };
    row.games.push(g);
    out.set(key, row);
  }
  return [...out.values()];
}

export function useDecisionGroups(plan: DeploymentPlan) {
  return useMemo(() => {
    // A game whose choice was changed (e.g. "copy anyway") is listed under "attention"; group by situation.
    const every = [...plan.attention, ...plan.removed, ...plan.same_game];
    const of = (...s: string[]) => rows(every.filter(x => x.choice && s.includes(x.choice.situation)));
    const groups: Record<Kind, Row[]> = {
      versions: of('clash'),
      on_drive: of('conflict'),
      removed: of('removed', 'kept_off'),
      same_name: of('same_game'),
      cant_copy: rows(plan.attention.filter(x => !x.choice)),
    };
    const waiting = groups.versions.filter(r => r.games[0].choice?.current === 'keep').length
      + groups.removed.filter(r => r.games[0].choice?.situation === 'removed' && r.games[0].choice.current === 'keep').length;
    return { groups, waiting };
  }, [plan]);
}

/** The current value of a row's choice, as the select shows it. */
function rowValue(row: Row, choices: Record<string, Choice>): string {
  const c = row.games[0].choice;
  if (!c) return '';
  if (c.situation === 'kept_off') return 'keep_off';
  const saved = choices[c.key];
  if (c.current === 'use' && saved?.kind === 'use') return 'use:' + saved.file_id;
  return c.current;
}

/** Apply one value to one row: updates `next` (choices) and `kept` (kept-off changes). */
function applyTo(row: Row, value: string, next: Record<string, Choice>, kept: KeptOffChange[]) {
  const g = row.games[0], c = g.choice;
  if (!c) return;
  if (c.situation === 'removed' && value === 'keep_off') { kept.push({ destination: g.destination, title: g.title, kept: true }); delete next[c.key]; return; }
  if (c.situation === 'kept_off') { if (value === 'keep_both') { kept.push({ destination: g.destination, title: g.title, kept: false }); next[c.key] = { kind: 'keep_both' }; } return; }
  if (value === 'keep') delete next[c.key];
  else if (value.startsWith('use:')) next[c.key] = { kind: 'use', file_id: value.slice(4) };
  else if (value === 'keep_both' && c.options.includes('keep_both')) next[c.key] = { kind: 'keep_both' };
  else if (value === 'replace' && c.options.includes('replace')) next[c.key] = { kind: 'replace' };
}

function RowSelect({ row, kind, value, disabled, onChange }: { row: Row; kind: Kind; value: string; disabled: boolean; onChange: (v: string) => void }) {
  const c = row.games[0].choice;
  if (!c) return <span className="muted">{row.games[0].reason}</span>;
  const labels = kind === 'versions' ? folderLabels(row.games) : [];
  return <select className="choice" aria-label={`What to do with ${row.title}`} value={value} disabled={disabled} onChange={e => onChange(e.target.value)}>
    {kind === 'versions' && <><option value="keep">Skip for now</option>{row.games.map((g, n) => <option key={g.choice!.file_id} value={'use:' + g.choice!.file_id}>Copy the one from “{labels[n]}” ({fmtSize(g.bytes)})</option>)}{c.options.includes('keep_both') && <option value="keep_both">Copy both (the second gets “(2)”)</option>}</>}
    {kind === 'on_drive' && <><option value="keep">Keep the drive’s file</option>{c.options.includes('keep_both') && <option value="keep_both">Copy both (adds “(2)”)</option>}{c.options.includes('replace') && <option value="replace">Replace (drive’s file backed up first)</option>}</>}
    {kind === 'removed' && <>{c.situation === 'removed' && <option value="keep">Ask me later</option>}<option value="keep_both">Copy it again</option><option value="keep_off">Keep it off this drive</option></>}
    {kind === 'same_name' && <><option value="keep">Don’t copy</option><option value="keep_both">Copy anyway</option></>}
  </select>;
}

/** "For all" choices per kind. Replace is never offered for all. */
function bulkOptions(kind: Kind, list: Row[]): { value: string; label: string }[] {
  if (kind === 'versions') {
    const folders = [...new Set(list.flatMap(r => folderLabels(r.games)))];
    return [{ value: 'keep', label: 'Skip them for now' }, ...folders.map(f => ({ value: 'folder:' + f, label: `Copy the one from “${f}”` })), { value: 'bigger', label: 'Copy the bigger file' }, { value: 'keep_both', label: 'Copy both' }];
  }
  if (kind === 'on_drive') return [{ value: 'keep', label: 'Keep the drive’s files' }, { value: 'keep_both', label: 'Copy both (adds “(2)”)' }];
  if (kind === 'removed') return [{ value: 'keep', label: 'Ask me later' }, { value: 'keep_both', label: 'Copy them again' }, { value: 'keep_off', label: 'Keep them off this drive' }];
  if (kind === 'same_name') return [{ value: 'keep', label: 'Don’t copy' }, { value: 'keep_both', label: 'Copy anyway' }];
  return [];
}

/** The per-row value a "for all" choice means for this row (null = leave it). */
function bulkValueFor(row: Row, bulk: string): string | null {
  if (bulk === 'bigger') { const big = [...row.games].sort((a, b) => b.bytes - a.bytes)[0]; return big.choice ? 'use:' + big.choice.file_id : null; }
  if (bulk.startsWith('folder:')) { const n = folderLabels(row.games).indexOf(bulk.slice(7)); return n >= 0 && row.games[n].choice ? 'use:' + row.games[n].choice!.file_id : null; }
  if (bulk === 'keep' && row.games[0].choice?.situation === 'kept_off') return null;
  return bulk;
}

/** One line under the summary; nothing if there's nothing to review. */
export function DecisionsLine({ plan, open, onToggle }: { plan: DeploymentPlan; open: boolean; onToggle: () => void }) {
  const { groups, waiting } = useDecisionGroups(plan);
  const total = Object.values(groups).reduce((n, g) => n + g.length, 0);
  if (total === 0) return null;
  return <p className="plan-notes decisions-line">
    {waiting > 0 ? <>{waiting.toLocaleString()} {waiting === 1 ? 'game is' : 'games are'} skipped until you choose; everything else goes ahead. </> : <>Nothing is waiting for you. </>}
    <button className="link" onClick={onToggle}>{open ? 'Hide choices' : `Review choices (${total.toLocaleString()})`}</button>
  </p>;
}

export function DecisionsPanel({ plan, choices, disabled, onApply, onClose }: {
  plan: DeploymentPlan; choices: Record<string, Choice>; disabled: boolean;
  onApply: (next: Record<string, Choice>, kept: KeptOffChange[]) => void; onClose: () => void;
}) {
  const { groups } = useDecisionGroups(plan);
  const kinds = (Object.keys(groups) as Kind[]).filter(k => groups[k].length > 0);
  const [picked, setPicked] = useState<Kind | null>(null);
  const [all, setAll] = useState(false);
  const [bulk, setBulk] = useState('');
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => { box.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' }); }, []);
  const kind = picked && groups[picked].length > 0 ? picked : kinds[0];
  if (!kind) return null;
  const list = groups[kind];
  const shown = all ? list : list.slice(0, ROWS);
  const options = bulkOptions(kind, list);
  const capped = (kind === 'removed' ? plan.removed.length : kind === 'same_name' ? plan.same_game.length : plan.attention.length) >= 500;
  const one = (row: Row, value: string) => { const next = { ...choices }, kept: KeptOffChange[] = []; applyTo(row, value, next, kept); onApply(next, kept); };
  const forAll = () => {
    if (!bulk) return;
    const next = { ...choices }, kept: KeptOffChange[] = [];
    for (const row of list) { const v = bulkValueFor(row, bulk); if (v) applyTo(row, v, next, kept); }
    setBulk(''); onApply(next, kept);
  };
  return <div className="decisions" ref={box}>
    <div className="decisions-head"><b>Choices</b><button className="link" onClick={onClose}>Close</button></div>
    <div className="decisions-tabs" role="tablist">{kinds.map(k => <button key={k} role="tab" aria-selected={k === kind} className={k === kind ? 'on' : ''} onClick={() => { setPicked(k); setAll(false); setBulk(''); }}>{kindText[k].tab} <span>{groups[k].length.toLocaleString()}</span></button>)}</div>
    <p className="plan-notes">{kindText[kind].about}</p>
    {options.length > 0 && <div className="decisions-bulk"><label>For all {list.length.toLocaleString()}: <select className="choice" value={bulk} disabled={disabled} onChange={e => setBulk(e.target.value)}><option value="">Choose…</option>{options.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}</select></label><button className="ghost small" disabled={!bulk || disabled} onClick={forAll}>Apply</button></div>}
    <div className="decisions-list">{shown.map(row => <div className="decision" key={row.key}>
      <div className="decision-game"><b>{row.title}</b><small title={row.games.map(g => g.source).join('\n')}>{row.platform}{row.games.length > 1 ? ` · ${row.games.length} files` : ` · ${shortPath(kind === 'removed' ? row.games[0].destination : row.games[0].source)}`}</small></div>
      <RowSelect row={row} kind={kind} value={rowValue(row, choices)} disabled={disabled} onChange={v => one(row, v)} />
    </div>)}</div>
    {list.length > ROWS && <button className="link" onClick={() => setAll(v => !v)}>{all ? 'Show fewer' : `Show all ${list.length.toLocaleString()}`}</button>}
    {capped && <p className="plan-notes">Only the first 500 are listed; after you choose, the rest appear here.</p>}
  </div>;
}
