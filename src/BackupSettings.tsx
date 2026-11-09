// Settings → Backups (FR-09): where every system's backups go.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { FolderOpen, HardDriveDownload } from 'lucide-react';
import { getBackupFolder, setBackupFolder } from './backupFolder';

export default function BackupSettings({ notify }: { notify: (text: string, error?: boolean) => void }) {
  const [folder, setFolder] = useState<string|null>(() => getBackupFolder());
  const [path, setPath] = useState('');
  useEffect(() => { void invoke<string>('backup_destination_path', { folder }).then(setPath).catch(e => notify(String(e), true)); }, [folder]);
  const choose = async () => {
    const picked = await open({ directory: true, multiple: false, title: 'Choose where RetroDrive keeps backups' });
    if (typeof picked !== 'string') return;
    setBackupFolder(picked); setFolder(picked);
  };
  const useDefault = () => { setBackupFolder(null); setFolder(null); };
  const openIt = () => void invoke('open_system_backups', { folder, backup: null }).catch(e => notify(String(e), true));
  return <section className="panel"><h2><HardDriveDownload size={18}/> Backups</h2>
    <p className="plan-notes">Backups of your systems' saves, settings and game lists go here. Make and restore them from each system in <b>Emulation Systems</b>.</p>
    <p><span className="mono">{path}</span>{!folder && ' (the default)'}</p>
    <div className="actions"><button className="ghost" onClick={() => void choose()}>Change folder…</button>
      {folder && <button className="ghost" onClick={useDefault}>Use Documents\RetroDrive Backups</button>}
      <button className="ghost" onClick={openIt}><FolderOpen size={16}/> Open folder</button></div>
  </section>;
}
