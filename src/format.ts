/** SQLite timestamps are UTC (`YYYY-MM-DD HH:MM:SS`); show them in local time. */
export function localTime(utc: string): string { const d=new Date(utc.replace(' ','T')+'Z'); return isNaN(d.getTime())?utc:d.toLocaleString(); }

/** Keep the drive and the last folders visible, e.g. C:\…\roms\snes\Game.sfc */
export function shortPath(path: string): string {
  const sep = path.includes('\\') ? '\\' : '/';
  const unc = path.startsWith('\\\\');
  const parts = path.split(sep).filter(Boolean);
  if (parts.length <= 5) return path;
  const head = unc ? '\\\\' + parts[0] + sep + parts[1] : parts[0];
  return head + sep + '…' + sep + parts.slice(-3).join(sep);
}

export const fmtSize = (n:number) => n >= 1073741824 ? `${(n/1073741824).toFixed(1)} GB` : n >= 1048576 ? `${(n/1048576).toFixed(1)} MB` : n >= 1024 ? `${(n/1024).toFixed(0)} KB` : `${n} B`;

/** The file name at the end of a path. */
export const baseName = (p:string) => p.split(/[\\/]/).pop() ?? p;
