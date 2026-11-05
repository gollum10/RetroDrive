import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import logoUrl from './assets/retrodrive-logo.png';
import { Activity, AlertTriangle, ChevronLeft, ChevronRight, CircleStop, Copy, Database, FolderOpen, Gamepad2, HardDrive, LayoutDashboard, LibraryBig, RefreshCcw, Search, Settings2, ShieldCheck } from 'lucide-react';

type Availability = 'online'|'offline'|'missing';
type Game = { id: string; game_id: string; title: string; platform: string; path: string; file_size: number; disc_count: number; incomplete: boolean; storage_name: string; availability: Availability; duplicate_count: number };
type GamePage = { rows: Game[]; total: number; total_capped: boolean; offset: number; limit: number };
type LibraryStats = { files: number; online: number; offline: number; missing: number; duplicate_files: number; platforms: { platform: string; files: number }[] };
type Filters = { text: string; platform: string; availability: ''|Availability; storage_id: string; duplicates_only: boolean; sort: 'title'|'platform'|'size' };
const noFilters: Filters = { text:'', platform:'', availability:'', storage_id:'', duplicates_only:false, sort:'title' };
const PAGE_SIZE = 100;
/** Keep the drive and the last folders visible, e.g. C:\…\roms\snes\Game.sfc */
function shortPath(path: string): string {
  const sep = path.includes('\\') ? '\\' : '/';
  const unc = path.startsWith('\\\\');
  const parts = path.split(sep).filter(Boolean);
  if (parts.length <= 5) return path;
  const head = unc ? '\\\\' + parts[0] + sep + parts[1] : parts[0];
  return head + sep + '…' + sep + parts.slice(-3).join(sep);
}
const fmtSize = (n:number) => n >= 1048576 ? `${(n/1048576).toFixed(1)} MB` : n >= 1024 ? `${(n/1024).toFixed(0)} KB` : `${n} B`;
type ScanIssue = { path: string; message: string; severity: 'error'|'warning' };
type ScanSummary = { indexed: number; games: number; new_files: number; marked_missing: number; skipped_unsupported: number; skipped_links: number; complete: boolean; cancelled: boolean; issue_count: number; error_count: number; warning_count: number; grouped_files: number; issues: ScanIssue[]; remounted_from: string|null };
type Presence = 'connected'|'not_connected'|'other_volume';
type StorageRow = { id: string; display_name: string; root_path: string; kind: string; stable_id: string|null; volume_label: string|null; filesystem: string|null; volume_serial: string|null; total_bytes: number|null; last_seen_at: string|null; file_count: number; missing_count: number; presence: Presence };
const sqlTime = (t: string|null) => t ? new Date(t.replace(' ','T')+'Z').toLocaleString() : 'never';
function presenceText(s: StorageRow): string {
  if(s.presence==='connected') return `Connected at ${s.root_path}`;
  if(s.presence==='other_volume') return `Not connected — ${s.root_path} is now a different drive`;
  return `Not connected — last seen at ${s.root_path}`;
}
type ScanProgress = { entries_checked: number; indexed: number; new_files: number; issue_count: number; current_path: string };
type ScanFinished = { job_id: string; state: 'completed'|'cancelled'|'failed'; summary: ScanSummary|null; error: string|null };
type ActiveScan = { job_id: string; folder: string; progress?: ScanProgress; cancelling?: boolean };
type JobRecord = { id: string; kind: string; state: string; input: { folder?: string }; progress: Partial<ScanSummary>|null; started_at: string; finished_at: string|null; error_message: string|null };
const jobStateLabel: Record<string,string> = { running:'Running', completed:'Completed', cancelled:'Cancelled', failed:'Failed', interrupted:'Interrupted (app closed)' };
function describeScan(r: ScanSummary): string {
  const moved = r.remounted_from ? `Recognized this drive from before (it was at ${r.remounted_from}); catalog paths now use the new drive letter. ` : '';
  const parts=[`Found ${r.games.toLocaleString()} games (${r.new_files.toLocaleString()} new files)`, `${r.skipped_unsupported.toLocaleString()} non-game files ignored`];
  if(r.marked_missing) parts.push(`${r.marked_missing} previously indexed files not found`);
  if(r.grouped_files) parts.push(`${r.grouped_files.toLocaleString()} disc/track files grouped into their games`);
  if(r.error_count) parts.push(`${r.error_count.toLocaleString()} item(s) could not be read`);
  if(r.warning_count) parts.push(`${r.warning_count.toLocaleString()} warning(s)`);
  if(r.cancelled) return moved+'Scan cancelled. '+parts.join(' · ')+'. Files found so far were kept; the missing-file check was skipped. No files were moved or modified.';
  if(!r.complete) parts.push('missing-file check skipped because the scan was incomplete');
  return moved+parts.join(' · ')+'. No files were moved or modified.';
}
const availabilityLabel: Record<Availability,string> = { online:'Online', offline:'Offline (drive not connected)', missing:'Missing at last scan' };
type Drive = { name: string; mount_point: string; total_bytes: number; available_bytes: number; removable: boolean };
type Page = 'Dashboard'|'Game Library'|'Drive Manager'|'Emulation Systems'|'Settings';
const menus = [
  {title:'Dashboard',icon:LayoutDashboard}, {title:'Game Library',icon:LibraryBig},
  {title:'Drive Manager',icon:HardDrive}, {title:'Emulation Systems',icon:Gamepad2}, {title:'Settings',icon:Settings2},
] as const;
const bytes = (n:number) => `${(n/1024/1024/1024).toFixed(1)} GB`;
export default function App() {
  const [page,setPage]=useState<Page>('Dashboard');
  const [stats,setStats]=useState<LibraryStats|null>(null); const [drives,setDrives]=useState<Drive[]>([]); const [storage,setStorage]=useState<StorageRow[]>([]);
  const [filters,setFilters]=useState<Filters>(noFilters); const [searchText,setSearchText]=useState(''); const [offset,setOffset]=useState(0);
  const [library,setLibrary]=useState<GamePage|null>(null); const [libraryLoading,setLibraryLoading]=useState(false); const [reloadKey,setReloadKey]=useState(0);
  const requestId=useRef(0);
  const [scan,setScan]=useState<ActiveScan|null>(null); const [jobs,setJobs]=useState<JobRecord[]>([]);
  const [message,setMessage]=useState('Ready. Connect a drive or import your existing ROM folder.'); const busy=scan!==null;
  const native=isTauri(); const [issues,setIssues]=useState<ScanIssue[]>([]); const [error,setError]=useState(false);
  const load=async()=>{
    if(!native){setError(true);setMessage('Desktop commands require the Tauri application. Use npm run tauri dev, not the browser preview.');return;}
    try { const [ls,d,j,st]=await Promise.all([invoke<LibraryStats>('library_stats'),invoke<Drive[]>('list_drives'),invoke<JobRecord[]>('recent_jobs',{limit:5}),invoke<StorageRow[]>('list_storage_locations')]); setStats(ls);setDrives(d);setJobs(j);setStorage(st); setReloadKey(k=>k+1); }
    catch(e){setError(true);setMessage(`Could not load the catalog: ${String(e)}`);}
  };
  const updateFilters=(change: Partial<Filters>)=>{ setFilters(f=>({...f,...change})); setOffset(0); };
  // Debounce typing so each keystroke doesn't run a query.
  useEffect(()=>{ const t=setTimeout(()=>updateFilters({text:searchText}),250); return ()=>clearTimeout(t); },[searchText]);
  useEffect(()=>{
    if(!native) return;
    const id=++requestId.current; setLibraryLoading(true);
    const query={ text:filters.text||null, platform:filters.platform||null, availability:filters.availability||null, storage_id:filters.storage_id||null, duplicates_only:filters.duplicates_only, sort:filters.sort, offset, limit:PAGE_SIZE };
    invoke<GamePage>('query_games',{query})
      .then(p=>{ if(id===requestId.current) setLibrary(p); })
      .catch(e=>{ if(id===requestId.current){ setError(true); setMessage(`Could not search the library: ${String(e)}`); } })
      .finally(()=>{ if(id===requestId.current) setLibraryLoading(false); });
  },[filters,offset,reloadKey]);
  const filtersActive = filters.text!==''||filters.platform!==''||filters.availability!==''||filters.storage_id!==''||filters.duplicates_only;
  const clearFilters=()=>{ setSearchText(''); setFilters(f=>({...noFilters,sort:f.sort})); setOffset(0); };
  const goToOffset=(next:number)=>{ setOffset(next); document.querySelector('.library-panel')?.scrollIntoView({block:'start'}); };
  const reviewDuplicates=()=>{ setSearchText(''); setFilters({...noFilters,duplicates_only:true}); setOffset(0); setPage('Game Library'); };
  useEffect(()=>{
    void load();
    if(!native) return;
    void invoke<ActiveScan|null>('active_scan').then(a=>{ if(a){ setScan(a); setMessage('Scan in progress. You can keep using the app while it runs.'); } });
    const progress=listen<{job_id:string;progress:ScanProgress}>('scan-progress',e=>setScan(s=>s&&s.job_id===e.payload.job_id?{...s,progress:e.payload.progress}:s));
    const finished=listen<ScanFinished>('scan-finished',e=>{
      const f=e.payload; setScan(null);
      if(f.summary){ setError(f.summary.error_count>0); setIssues(f.summary.issues); setMessage(describeScan(f.summary)); }
      else { setError(true); setMessage(`Import failed: ${f.error??'unknown error'}`); }
      void load();
    });
    return ()=>{ void progress.then(u=>u()); void finished.then(u=>u()); };
  },[]);
  const importFolder=async()=>{
    try { const folder=await open({directory:true,multiple:false,title:'Select a folder containing games'});
      if(!folder || typeof folder!=='string')return;
      setError(false);setIssues([]);setMessage('Scan in progress. You can keep using the app while it runs.');
      const jobId=await invoke<string>('start_scan',{folder});
      setScan(s=>s??{job_id:jobId,folder});
    } catch(e){setError(true);setMessage(`Import failed: ${String(e)}`)}
  };
  const cancelScan=async()=>{
    if(!scan) return;
    setScan({...scan,cancelling:true}); setMessage('Cancelling scan — finishing the current file...');
    try { await invoke<boolean>('cancel_job',{jobId:scan.job_id}); } catch(e){ setError(true); setMessage(`Could not cancel: ${String(e)}`); }
  };
  return <div className="app">
    <aside className="sidebar"><div className="brand"><span className="brand-icon"><Gamepad2 size={25}/></span><div><strong>RETRODRIVE</strong><small>LIBRARY MANAGER</small></div></div>
      <div className="nav-label">WORKSPACE</div><nav>{menus.map(item=><button key={item.title} className={'nav-item '+(page===item.title?'selected':'')} onClick={()=>setPage(item.title)}><item.icon size={19}/>{item.title}</button>)}</nav>
      <div className="aside-bottom"><img className="sidebar-logo" src={logoUrl} alt="RetroDrive" width={192} height={192}/><div className="aside-status"><ShieldCheck size={17}/> Read-only disk discovery</div><small>v0.1.0 · dev build</small></div>
    </aside>
    <main><header><div><div className="eyebrow">RETRO GAMING COMMAND CENTER</div><h1>{page}</h1></div><button className="ghost" onClick={()=>void load()}><RefreshCcw size={16}/> Refresh</button></header>
    <div className={'notice '+(error?'warn':'')}><Activity size={17}/>{message}</div>
    {scan && <section className="scan-progress" aria-live="polite"><div className="scan-head"><div><b>Scanning</b> <span className="scan-folder" title={scan.folder}>{scan.folder}</span></div><button className="ghost" disabled={scan.cancelling} onClick={()=>void cancelScan()}><CircleStop size={16}/>{scan.cancelling?'Cancelling...':'Cancel scan'}</button></div>
      <div className="bar indeterminate"><div/></div>
      <div className="scan-stats"><span><b>{(scan.progress?.entries_checked??0).toLocaleString()}</b> items checked</span><span><b>{(scan.progress?.indexed??0).toLocaleString()}</b> game files found</span><span><b>{(scan.progress?.new_files??0).toLocaleString()}</b> new</span>{(scan.progress?.issue_count??0)>0&&<span className="warn-text"><b>{scan.progress?.issue_count}</b> unreadable</span>}</div>
      <small className="scan-current" title={scan.progress?.current_path}>{scan.progress?.current_path||'Starting...'}</small></section>}
    {issues.length>0 && <details className="issues"><summary><AlertTriangle size={15}/> {issues.length} scan issue(s){issues.length>=200?' (first 200 shown)':''}</summary><ul>{issues.map((i,n)=><li key={n} className={'issue-'+i.severity}><b>{i.severity==='error'?'Error':'Warning'}:</b> <code>{i.path}</code> — {i.message}</li>)}</ul></details>}
    {page==='Dashboard' && <><section className="stats"><article><span>Cataloged games</span><strong>{(stats?.files??0).toLocaleString()}</strong><small>{stats?.platforms.length??0} platforms · local database</small></article><article><span>Detected volumes</span><strong>{drives.length}</strong><small>Read-only discovery</small></article><article><span>Available titles</span><strong>{(stats?.online??0).toLocaleString()}</strong><small>{stats&&(stats.offline||stats.missing)?`${stats.offline.toLocaleString()} offline · ${stats.missing.toLocaleString()} missing`:'Files currently accessible'}</small></article></section><div className="twocol"><section className="panel"><h2>Quick actions</h2><p>Get started with your existing library. Scanning never changes game files.</p><button className="primary" disabled={!native||busy} onClick={importFolder}><FolderOpen size={18}/>{busy?'Scanning...':'Import ROM folder'}</button><button className="subtle" onClick={()=>setPage('Game Library')}><LibraryBig size={18}/> Browse library</button>{(stats?.duplicate_files??0)>0&&<p className="dup-hint"><Copy size={15}/> {stats!.duplicate_files.toLocaleString()} files look like duplicate copies. <button className="link" onClick={reviewDuplicates}>Review them</button> (nothing is deleted).</p>}</section><section className="panel"><h2>Platform roadmap</h2><div className="platform-row"><span>Batocera</span><em>Adapter planned</em></div><div className="platform-row"><span>RetroBat</span><em>Adapter planned</em></div><div className="platform-row"><span>More systems</span><em>Future milestone</em></div></section></div>
    <section className="panel activity"><h2>Recent activity</h2>{jobs.length===0?<p>No scans yet.</p>:jobs.map(j=><div className="platform-row" key={j.id}><span><b className={'job '+j.state}>{jobStateLabel[j.state]??j.state}</b> <span className="job-folder" title={j.input.folder}>{j.input.folder??j.kind}</span></span><em>{j.progress?.indexed!=null?`${j.progress.indexed} files · `:''}{j.error_message?j.error_message+' · ':''}{new Date(j.started_at.replace(' ','T')+'Z').toLocaleString()}</em></div>)}</section></>}
    {page==='Game Library' && <section className="panel library-panel">
      <div className="section-heading"><div><h2>Master game catalog</h2><p>One searchable view across every imported folder and drive, including drives that aren't plugged in.</p></div><button className="primary" disabled={!native||busy} onClick={importFolder}><FolderOpen size={17}/> Import folder</button></div>
      <label className="search"><Search size={18}/><input placeholder="Search titles or systems" value={searchText} onChange={e=>setSearchText(e.target.value)} aria-label="Search titles or systems"/></label>
      <div className="filters">
        <select aria-label="Platform" value={filters.platform} onChange={e=>updateFilters({platform:e.target.value})}><option value="">All platforms</option>{stats?.platforms.map(p=><option key={p.platform} value={p.platform}>{p.platform} ({p.files.toLocaleString()})</option>)}</select>
        <select aria-label="Status" value={filters.availability} onChange={e=>updateFilters({availability:e.target.value as Filters['availability']})}><option value="">Any status</option><option value="online">Online</option><option value="offline">Offline</option><option value="missing">Missing</option></select>
        <select aria-label="Drive" value={filters.storage_id} onChange={e=>updateFilters({storage_id:e.target.value})}><option value="">All drives</option>{storage.map(d=><option key={d.id} value={d.id}>{d.volume_label??d.display_name}</option>)}</select>
        <select aria-label="Sort" value={filters.sort} onChange={e=>updateFilters({sort:e.target.value as Filters['sort']})}><option value="title">Sort: Title</option><option value="platform">Sort: Platform</option><option value="size">Sort: Largest first</option></select>
        <label className="check"><input type="checkbox" checked={filters.duplicates_only} onChange={e=>updateFilters({duplicates_only:e.target.checked})}/> Possible duplicates only</label>
        {filtersActive&&<button className="link" onClick={clearFilters}>Clear filters</button>}
      </div>
      <div className={'table-wrap'+(libraryLoading?' loading':'')}><table><thead><tr><th>Game</th><th>Platform</th><th>Drive</th><th>Size</th><th>Availability</th></tr></thead><tbody>{library?.rows.map(g=><tr key={g.id}><td><b>{g.title}{g.disc_count>1&&<span className="disc-badge" title="Multi-disc game: discs listed by its .m3u playlist">{g.disc_count} discs</span>}{g.incomplete&&<span className="incomplete-badge" title="A disc or track file this game needs is missing. See the scan warnings.">Incomplete</span>}{g.duplicate_count>1&&<span className="dup-badge" title={`Possible duplicate: ${g.duplicate_count} files share this platform, file name and size. Review before removing anything; RetroDrive never deletes files.`}>×{g.duplicate_count}</span>}</b><small title={g.path}>{shortPath(g.path)}</small></td><td>{g.platform}</td><td className="drive-cell" title={g.storage_name}>{g.storage_name}</td><td>{fmtSize(g.file_size)}</td><td><span className={'state '+g.availability}>{availabilityLabel[g.availability]}</span></td></tr>)}</tbody></table>
        {library&&library.rows.length===0&&<div className="empty"><Database size={26}/>{(stats?.files??0)===0?'No games yet. Import a ROM folder to begin.':<>No games match these filters. <button className="link" onClick={clearFilters}>Clear filters</button></>}</div>}</div>
      {library&&library.total>0&&<div className="pager"><span>{library.rows.length?`${(library.offset+1).toLocaleString()}–${(library.offset+library.rows.length).toLocaleString()}`:'0'} of {library.total_capped?`${(library.total-1).toLocaleString()}+`:library.total.toLocaleString()}</span>
        <button className="ghost" disabled={offset===0||libraryLoading} onClick={()=>goToOffset(Math.max(0,offset-PAGE_SIZE))}><ChevronLeft size={16}/> Previous</button>
        <button className="ghost" disabled={library.rows.length<PAGE_SIZE||(!library.total_capped&&offset+PAGE_SIZE>=library.total)||libraryLoading} onClick={()=>goToOffset(offset+PAGE_SIZE)}>Next <ChevronRight size={16}/></button></div>}
    </section>}
    {page==='Drive Manager' && <><section className="panel"><h2>Connected volumes</h2><p>Detected storage volumes are displayed for inspection. Formatting and image-writing are intentionally not implemented in this starter.</p><div className="drive-grid">{drives.map(d=><article className="drive" key={d.mount_point}><div className="drive-title"><HardDrive size={23}/><div><b>{d.name||d.mount_point}</b><small>{d.mount_point} {d.removable?'• Removable':''}</small></div></div><div className="bar"><div style={{width:`${d.total_bytes ? 100*(1-d.available_bytes/d.total_bytes):0}%`}}/></div><small>{bytes(Math.max(0,d.total_bytes-d.available_bytes))} used of {bytes(d.total_bytes)}</small></article>)}</div>{drives.length===0&&<div className="empty">No volumes reported by the desktop backend.</div>}</section>
    <section className="panel known-drives"><h2>Known drives</h2><p>Every drive the catalog has scanned, including ones that aren't plugged in. Drives are recognized by their volume ID, not their letter, so a drive that comes back as a different letter keeps its games.</p>
      {storage.length===0?<div className="empty">No drives scanned yet.</div>:storage.map(s=><article className="known-drive" key={s.id}>
        <div className="known-title"><HardDrive size={20}/><div><b>{s.volume_label??s.display_name}</b><small className={'presence '+s.presence}>{presenceText(s)}</small></div></div>
        <div className="known-meta"><span>{s.file_count.toLocaleString()} files{s.missing_count?` · ${s.missing_count.toLocaleString()} missing`:''}</span>{s.filesystem&&<span>{s.filesystem}</span>}{s.total_bytes!=null&&<span>{bytes(s.total_bytes)}</span>}<span>{s.kind}</span>{s.volume_serial&&<span title={s.stable_id??''}>ID {s.volume_serial}</span>}<span>Last scanned {sqlTime(s.last_seen_at)}</span></div>
      </article>)}
    </section></>}
    {page==='Emulation Systems' && <section className="panel"><h2>Emulation systems</h2><p>Batocera and RetroBat adapters are the next milestone. This screen will manage verified installers, device compatibility, and installed versions.</p><div className="platform-row"><span>Batocera</span><em>Not yet connected</em></div><div className="platform-row"><span>RetroBat</span><em>Not yet connected</em></div></section>}
    {page==='Settings' && <section className="panel"><h2>Application settings</h2><p>RetroDrive currently stores its SQLite catalog in the operating system's application data folder. Imports index files in place and never copy, move, or delete them.</p><p>Future settings: metadata providers, library scan schedules, backup locations, and platform defaults.</p></section>}
    <footer>RetroDrive · Early development build · No disk writes or platform installation features</footer>
    </main></div>;
}
