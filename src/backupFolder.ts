// The folder backups go into (FR-09): chosen in Settings, used by each system's Backup
// panel in Emulation Systems. `null` means Documents\RetroDrive Backups.
const FOLDER_KEY = 'rd.backupFolder';

export function getBackupFolder(): string|null {
  try { return localStorage.getItem(FOLDER_KEY); } catch { return null; }
}

export function setBackupFolder(folder: string|null) {
  try { if (folder === null) localStorage.removeItem(FOLDER_KEY); else localStorage.setItem(FOLDER_KEY, folder); } catch { /* not remembered */ }
}
