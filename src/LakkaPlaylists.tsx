// Lakka playlists and box art (ADR-086): Lakka shows games only through RetroArch
// playlists. Copies add their games automatically; this card adds every game RetroDrive
// copied (e.g. after an earlier copy or a Lakka rescan), with the user's PNG box art.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Eye, ListPlus } from 'lucide-react';

type PlaylistChange = { playlist: string; add: number; already: number; skipped: boolean };
type Preview = { playlists: PlaylistChange[]; art_to_add: number; art_not_png: number; unknown_systems: string[]; shas: Record<string,string> };

export default function LakkaPlaylists({ installationId, disabled, notify }: { installationId: string; disabled: boolean; notify: (text: string, error?: boolean) => void }) {
  const [p, setP] = useState<Preview|null>(null); const [working, setWorking] = useState(false);
  useEffect(() => { setP(null); }, [installationId]);
  const check = async () => { setWorking(true); try { setP(await invoke<Preview>('preview_lakka_playlists', { installationId })); } catch (e) { notify(String(e), true); } finally { setWorking(false); } };
  const add = async () => { if (!p) return; setWorking(true);
    try { notify(await invoke<string>('apply_lakka_playlists', { installationId, shas: p.shas })); setP(null); } catch (e) { notify(`Could not add: ${String(e)}`, true); } finally { setWorking(false); } };
  const adding = p ? p.playlists.reduce((n, l) => n + l.add, 0) : 0;
  return <div className="backups"><h3>Lakka playlists and box art</h3>
    <p className="plan-notes">Lakka shows games through its playlists. Games RetroDrive copies are added automatically; this adds any that are missing, plus your own PNG box art. Existing playlist entries are never removed, and pictures already there are kept.</p>
    {!p ? <div className="actions"><button className="ghost" disabled={disabled || working} onClick={() => void check()}><Eye size={15}/> {working ? 'Checking…' : 'Check what can be added'}</button></div>
    : <div className="confirm">
      {adding === 0 && p.art_to_add === 0 ? <p>Nothing new to add: the games RetroDrive copied are already in Lakka's playlists.</p> : <p><b>Add {adding} game{adding === 1 ? '' : 's'}{p.art_to_add ? ` and ${p.art_to_add} box art picture${p.art_to_add === 1 ? '' : 's'}` : ''}?</b></p>}
      <ul>{p.playlists.filter(l => l.add || l.skipped).map(l => <li key={l.playlist}>{l.playlist}: {l.skipped ? 'left alone (an older playlist format)' : `${l.add} to add${l.already ? `, ${l.already} already there` : ''}`}</li>)}
        {p.art_not_png > 0 && <li className="plan-notes">{p.art_not_png} game{p.art_not_png === 1 ? "'s" : "s'"} art isn't PNG, which Lakka can't show.</li>}
        {p.unknown_systems.length > 0 && <li className="plan-notes">No Lakka playlist known for {p.unknown_systems.join(', ')}; scan those folders on Lakka.</li>}
        <li className="plan-notes">Playlists are backed up first and Undo puts them back. On Lakka, the first time a game starts it asks which core to use.</li></ul>
      <div className="actions">{(adding > 0 || p.art_to_add > 0) && <button className="primary" disabled={disabled || working} onClick={() => void add()}><ListPlus size={15}/> {working ? 'Adding…' : 'Add to Lakka'}</button>}<button className="ghost" disabled={working} onClick={() => setP(null)}>Not now</button></div></div>}
  </div>;
}
