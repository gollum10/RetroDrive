// "Game" choice for settings of one game (ADR-083): the games in the chosen system's
// folder, by the name EmulationStation uses in its settings. "All <system> games" = the
// system's own settings.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

// The settings scope for a system, or for one game of it (`snes["Mario.zip"]`).
export const gameScope = (system: string, game: string) => game ? `${system}["${game}"]` : system;

export default function GamePicker({ installationId, system, command, value, onChange, changed, disabled }: { installationId: string; system: string; command: string; value: string; onChange: (game: string) => void; changed: (game: string) => boolean; disabled: boolean }) {
  const [games, setGames] = useState<string[]>([]); const [problem, setProblem] = useState('');
  useEffect(() => { setGames([]); setProblem(''); if (system === 'global') return;
    invoke<string[]>(command, { installationId, system }).then(setGames).catch(e => setProblem(String(e))); }, [installationId, system, command]);
  if (system === 'global') return null;
  return <label title="Settings for one game override the system's settings for that game only">Game <select className="choice" value={value} disabled={disabled} onChange={e => onChange(e.target.value)}>
    <option value="">{`All ${system} games`}</option>
    {games.map(g => <option key={g} value={g}>{g}{changed(g) ? ' •' : ''}</option>)}</select>{problem && <small className="warn-text"> {problem}</small>}</label>;
}
