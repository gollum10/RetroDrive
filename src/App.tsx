import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import sidebarArtUrl from './assets/retrodrive-sidebar-art.png';
import iconUrl from './assets/retrodrive-icon.png';
import { Activity, AlertTriangle, CheckCircle2, ChevronLeft, ChevronRight, CircleStop, Copy, Database, Eye, Plus, XCircle, FolderOpen, Gamepad2, HardDrive, LayoutDashboard, LayoutGrid, LibraryBig, List, RefreshCcw, Search, Settings2, ShieldCheck } from 'lucide-react';

type Availability = 'online'|'offline'|'missing';
type Game = { id: string; game_id: string; title: string; platform: string; path: string; file_size: number; disc_count: number; incomplete: boolean; storage_name: string; availability: Availability; duplicate_count: number; reference: RefInfo|null; file_id: string; art: string|null };
/** URL of a game's linked image (served only by RetroDrive's media protocol), or null. */
const artUrl = (g: { file_id: string; art: string|null }) => g.art ? `${convertFileSrc(g.file_id, 'rdmedia')}?v=${encodeURIComponent(g.art.split('|')[2] ?? '')}` : null;
const artSource = (g: { art: string|null }) => g.art?.split('|')[1] ?? null;
type RefInfo = { title: string; year: number|null; genre: string|null; publisher: string|null; developer: string|null; method: 'name'|'variant'|'title' };
type MatchCounts = { by_name: number; by_variant: number; by_title: number };
type PlatformMatch = { platform: string; games: number; matched: number };
type CatalogStatus = { name: string; file_name: string; imported_at: string; entries: number; genres_added: number; matched: MatchCounts; platforms: PlatformMatch[] };
type ImportSummary = { kind: 'catalog'|'genres'; file_name: string; entries: number; genres_added: number; skipped_rows: number; ignored_columns: string[]; matched: MatchCounts; platforms: PlatformMatch[] };
/** The catalog title for confident (name/variant) matches; otherwise the game's own title. */
const fileStem = (path: string) => { const name = path.split(/[\/]/).pop() ?? path; const dot = name.lastIndexOf('.'); return dot > 0 ? name.slice(0, dot) : name; };
/** The catalog title for confident matches; clone sets also show their set name so they can be told apart. */
const displayTitle = (g: Game) => !g.reference || g.reference.method === 'title' ? g.title : g.reference.method === 'variant' ? `${g.reference.title} (${fileStem(g.path)})` : g.reference.title;
const refLine = (r: RefInfo|null) => r ? [r.year, r.genre].filter(Boolean).join(' · ') : '';
type GamePage = { rows: Game[]; total: number; total_capped: boolean; offset: number; limit: number };
type LibraryStats = { files: number; online: number; offline: number; missing: number; duplicate_files: number; platforms: { platform: string; files: number }[] };
type SystemSummary = { platform: string; games: number; bytes: number };
type Filters = { text: string; platform: string; availability: ''|Availability; storage_id: string; duplicates_only: boolean; sort: 'title'|'platform'|'size' };
const noFilters: Filters = { text:'', platform:'', availability:'', storage_id:'', duplicates_only:false, sort:'title' };
const PAGE_SIZE = 100;
type LibraryMode = 'systems'|'games'; type LibraryView = 'list'|'grid';
function remembered<T extends string>(key: string, allowed: readonly T[], fallback: T): T { try { const v = localStorage.getItem(key); return v && (allowed as readonly string[]).includes(v) ? v as T : fallback; } catch { return fallback; } }
function remember(key: string, value: string) { try { localStorage.setItem(key, value); } catch { /* storage unavailable: the choice just isn't remembered */ } }
/** A stable colour per platform for placeholder tiles (until box art is added). */
function platformHue(name: string): number { let h = 0; for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360; return h; }
function initials(title: string): string { const words = title.replace(/\(.*?\)|\[.*?\]/g, '').split(/[\s:\-_]+/).filter(w => /[A-Za-z0-9]/.test(w)); return (words.slice(0, 2).map(w => w[0]).join('') || '?').toUpperCase(); }
/** Keep the drive and the last folders visible, e.g. C:\…\roms\snes\Game.sfc */
function shortPath(path: string): string {
  const sep = path.includes('\\') ? '\\' : '/';
  const unc = path.startsWith('\\\\');
  const parts = path.split(sep).filter(Boolean);
  if (parts.length <= 5) return path;
  const head = unc ? '\\\\' + parts[0] + sep + parts[1] : parts[0];
  return head + sep + '…' + sep + parts.slice(-3).join(sep);
}
const fmtSize = (n:number) => n >= 1073741824 ? `${(n/1073741824).toFixed(1)} GB` : n >= 1048576 ? `${(n/1048576).toFixed(1)} MB` : n >= 1024 ? `${(n/1024).toFixed(0)} KB` : `${n} B`;
type AdapterInfo = { id: string; display_name: string; kind: 'portable_app'|'bootable_os'|'frontend'; capabilities: { detect: boolean; validate_layout: boolean; plan_placement: boolean; install: boolean; write_files: boolean }; contract_version: number };
type Detection = { adapter_id: string; display_name: string; kind: string; confidence: 'not_detected'|'possible'|'detected'; evidence: string[]; version: string|null };
type LayoutCheck = { id: string; label: string; status: 'ok'|'warning'|'problem'; detail: string };
type Inspection = { path: string; detections: Detection[]; best: Detection|null; checks: LayoutCheck[] };
type Installation = { id: string; adapter_id: string; adapter_name: string; path: string; version: string|null; presence: 'connected'|'not_connected'|'other_volume'; checks: LayoutCheck[]; added_at: string; arcade_system: 'mame'|'fbneo'|null };
type PlanAction = 'in_place'|'already_there'|'duplicate'|'copy'|'conflict'|'blocked';
type PlanGame = { title: string; platform: string; system_folder: string; action: PlanAction; reason: string|null; source: string; destination: string; files: number; bytes: number; bytes_to_copy: number };
type DeploymentPlan = { installation_id: string; adapter_name: string; installation_path: string; systems: { system_folder: string; platforms: string[]; games: number; bytes: number; selected: boolean }[]; unsupported: { platform: string; reason: string; games: number }[]; unavailable_games: number; games: Record<PlanAction,number>; selected_games: number; files_to_copy: number; bytes_to_copy: number; folders_to_create: number; long_paths: number; capacity: { measured: boolean; filesystem: string|null; free_bytes: number; total_bytes: number; reserve_bytes: number; staging_bytes: number; required_bytes: number; fits: boolean|null; max_file_bytes: number|null }; attention: PlanGame[]; sample: PlanGame[]; fingerprint: string };
const actionLabel: Record<PlanAction,string> = { copy:'Would be copied', already_there:'Already there (skipped)', in_place:'Already in place', duplicate:'Duplicate (copied once)', conflict:'Conflict: existing file kept', blocked:'Blocked' };
const kindLabel: Record<string,string> = { portable_app:'Portable Windows app', bootable_os:'Bootable operating system', frontend:'Frontend' };
const installPresence: Record<Installation['presence'],string> = { connected:'Connected', not_connected:'Not connected (drive unplugged or folder moved)', other_volume:'Not connected (that drive letter is now a different drive)' };
function Checks({ checks }: { checks: LayoutCheck[] }) {
  return <ul className="checks">{checks.map(c=><li key={c.id} className={'check '+c.status}>{c.status==='ok'?<CheckCircle2 size={15}/>:c.status==='warning'?<AlertTriangle size={15}/>:<XCircle size={15}/>}<span><b>{c.label}</b> — {c.detail}</span></li>)}</ul>;
}
type ScanIssue = { path: string; message: string; severity: 'error'|'warning' };
type ScanSummary = { indexed: number; games: number; new_files: number; marked_missing: number; skipped_unsupported: number; skipped_links: number; complete: boolean; cancelled: boolean; issue_count: number; error_count: number; warning_count: number; grouped_files: number; issues: ScanIssue[]; remounted_from: string|null; images_found: number; images_linked: number };
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
type ActiveScan = { job_id: string; folder: string; kind?: string; progress?: ScanProgress; cancelling?: boolean };
type DeployProgress = { games_done: number; games_total: number; files_done: number; files_total: number; bytes_done: number; bytes_total: number; current: string };
type DeploySummary = { deployment_id: string; status: 'completed'|'completed_with_issues'|'cancelled'|'interrupted'; games_copied: number; games_already_there: number; games_incomplete: number; games_conflict: number; files_copied: number; bytes_copied: number; files_identical: number; files_failed: number; files_not_attempted: number; cancelled: boolean; drive_lost: boolean; destination: string; manifest_path: string|null; manifest_error: string|null; issues: { game: string; path: string; message: string }[] };
type ActiveCopy = { job_id: string; folder: string; progress?: DeployProgress; cancelling?: boolean };
type CopyResult = { summary: DeploySummary|null; error: string|null };
const copyHeadline: Record<DeploySummary['status'],string> = { completed:'Copy finished: every file was verified.', completed_with_issues:'Copy finished, with some games needing attention.', cancelled:'Copy cancelled. Games already verified were kept; nothing half-written was left behind.', interrupted:'Copy stopped: the drive was disconnected. Verified files were kept; run the copy again to finish.' };
type JobRecord = { id: string; kind: string; state: string; input: { folder?: string }; progress: (Partial<ScanSummary>&Partial<DeploySummary>)|null; started_at: string; finished_at: string|null; error_message: string|null };
const jobStateLabel: Record<string,string> = { running:'Running', completed:'Completed', cancelled:'Cancelled', failed:'Failed', interrupted:'Interrupted (app closed)' };
function describeScan(r: ScanSummary): string {
  const moved = r.remounted_from ? `Recognized this drive from before (it was at ${r.remounted_from}); catalog paths now use the new drive letter. ` : '';
  const parts=[`Found ${r.games.toLocaleString()} games (${r.new_files.toLocaleString()} new files)`, `${r.skipped_unsupported.toLocaleString()} non-game files ignored`];
  if(r.marked_missing) parts.push(`${r.marked_missing} previously indexed files not found`);
  if(r.grouped_files) parts.push(`${r.grouped_files.toLocaleString()} disc/track files grouped into their games`);
  if(r.images_found) parts.push(`${r.images_found.toLocaleString()} images found, ${r.images_linked.toLocaleString()} games linked to art`);
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
  const [libraryMode,setLibraryMode]=useState<LibraryMode>(()=>remembered('rd.libraryMode',['systems','games'] as const,'systems'));
  const [libraryView,setLibraryView]=useState<LibraryView>(()=>remembered('rd.libraryView',['list','grid'] as const,'grid'));
  const chooseMode=(m:LibraryMode)=>{ setLibraryMode(m); remember('rd.libraryMode',m); };
  const [systems,setSystems]=useState<SystemSummary[]|null>(null);
  useEffect(()=>{ if(page!=='Game Library'||libraryMode!=='systems'||!native) return; let live=true;
    invoke<SystemSummary[]>('system_summary').then(r=>{ if(live) setSystems(r); }).catch(e=>{ if(live){ setError(true); setMessage(`Could not load systems: ${String(e)}`); } });
    return ()=>{ live=false; }; },[page,libraryMode,stats]);
  const chooseView=(v:LibraryView)=>{ setLibraryView(v); remember('rd.libraryView',v); };
  const [filters,setFilters]=useState<Filters>(noFilters); const [searchText,setSearchText]=useState(''); const [offset,setOffset]=useState(0);
  const [refStatus,setRefStatus]=useState<CatalogStatus|null>(null); const [refBusy,setRefBusy]=useState(false); const [refResult,setRefResult]=useState<ImportSummary|null>(null); const [refRemoving,setRefRemoving]=useState(false);
  const [library,setLibrary]=useState<GamePage|null>(null); const [libraryLoading,setLibraryLoading]=useState(false); const [reloadKey,setReloadKey]=useState(0);
  const requestId=useRef(0); const finishedJobs=useRef(new Set<string>()); const resultRef=useRef<HTMLElement|null>(null);
  const [adapters,setAdapters]=useState<AdapterInfo[]>([]); const [installs,setInstalls]=useState<Installation[]>([]);
  const [inspection,setInspection]=useState<Inspection|null>(null); const [plan,setPlan]=useState<DeploymentPlan|null>(null);
  const [planLoading,setPlanLoading]=useState<string|null>(null); const [planBuilt,setPlanBuilt]=useState<Date|null>(null); const [forgetting,setForgetting]=useState<string|null>(null);
  const [scan,setScan]=useState<ActiveScan|null>(null); const [jobs,setJobs]=useState<JobRecord[]>([]);
  const [copy,setCopy]=useState<ActiveCopy|null>(null); const [copyResult,setCopyResult]=useState<CopyResult|null>(null); const [confirming,setConfirming]=useState(false);
  const [message,setMessage]=useState('Ready. Connect a drive or import your existing ROM folder.'); const busy=scan!==null||copy!==null;
  const native=isTauri(); const [issues,setIssues]=useState<ScanIssue[]>([]); const [error,setError]=useState(false);
  const load=async()=>{
    if(!native){setError(true);setMessage('Desktop commands require the Tauri application. Use npm run tauri dev, not the browser preview.');return;}
    try { const [ls,d,j,st]=await Promise.all([invoke<LibraryStats>('library_stats'),invoke<Drive[]>('list_drives'),invoke<JobRecord[]>('recent_jobs',{limit:5}),invoke<StorageRow[]>('list_storage_locations')]); setStats(ls);setDrives(d);setJobs(j);setStorage(st); setReloadKey(k=>k+1);
      const [ad,ins]=await Promise.all([invoke<AdapterInfo[]>('list_adapters'),invoke<Installation[]>('list_installations')]); setAdapters(ad); setInstalls(ins); }
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
  useEffect(()=>{ if(copyResult) resultRef.current?.scrollIntoView({behavior:'smooth',block:'nearest'}); },[copyResult]);
  useEffect(()=>{
    void load();
    if(!native) return;
    void invoke<ActiveScan|null>('active_scan').then(a=>{ if(!a) return; if(a.kind==='deployment'){ setCopy({job_id:a.job_id,folder:a.folder}); setMessage('Copy in progress.'); } else { setScan(a); setMessage('Scan in progress. You can keep using the app while it runs.'); } });
    const progress=listen<{job_id:string;progress:ScanProgress}>('scan-progress',e=>setScan(s=>s&&s.job_id===e.payload.job_id?{...s,progress:e.payload.progress}:s));
    const finished=listen<ScanFinished>('scan-finished',e=>{
      const f=e.payload; finishedJobs.current.add(f.job_id); setScan(null);
      if(f.summary){ setError(f.summary.error_count>0); setIssues(f.summary.issues); setMessage(describeScan(f.summary)); }
      else { setError(true); setMessage(`Import failed: ${f.error??'unknown error'}`); }
      void load();
    });
    const copyProgress=listen<{job_id:string;progress:DeployProgress}>('deploy-progress',e=>setCopy(c=>c&&c.job_id===e.payload.job_id?{...c,progress:e.payload.progress}:c));
    const copyFinished=listen<{job_id:string;state:string;summary:DeploySummary|null;error:string|null}>('deploy-finished',e=>{
      const f=e.payload; finishedJobs.current.add(f.job_id); setCopy(null); setPlan(null); setCopyResult({summary:f.summary,error:f.error});
      if(f.summary){ setError(f.summary.status!=='completed'); setMessage(copyHeadline[f.summary.status]); }
      else { setError(true); setMessage(`The copy didn't complete: ${f.error??'unknown error'}`); }
      void load();
    });
    return ()=>{ void progress.then(u=>u()); void finished.then(u=>u()); void copyProgress.then(u=>u()); void copyFinished.then(u=>u()); };
  },[]);
  const importFolder=async()=>{
    try { const folder=await open({directory:true,multiple:false,title:'Select a folder containing games'});
      if(!folder || typeof folder!=='string')return;
      setError(false);setIssues([]);setMessage('Scan in progress. You can keep using the app while it runs.');
      const jobId=await invoke<string>('start_scan',{folder});
      if(!finishedJobs.current.has(jobId)) setScan(s=>s??{job_id:jobId,folder});
    } catch(e){setError(true);setMessage(`Import failed: ${String(e)}`)}
  };
  const addInstallation=async()=>{
    try { const folder=await open({directory:true,multiple:false,title:'Select your RetroBat folder (the one containing RetroBat.exe)'});
      if(!folder || typeof folder!=='string') return;
      setPlan(null); setInspection(await invoke<Inspection>('inspect_installation',{folder}));
    } catch(e){ setError(true); setMessage(`Could not check that folder: ${String(e)}`); }
  };
  const saveInspection=async()=>{
    if(!inspection?.best) return;
    try { await invoke<string>('add_installation',{folder:inspection.path,adapterId:inspection.best.adapter_id});
      setError(false); setMessage(`${inspection.best.display_name} installation added. Nothing in it was changed.`); setInspection(null); await load();
    } catch(e){ setError(true); setMessage(`Could not add the installation: ${String(e)}`); }
  };
  const showPlan=async(id:string, systems:string[]|null=null)=>{
    setPlanLoading(id); setConfirming(false); setCopyResult(null);
    try { setPlan(await invoke<DeploymentPlan>('deployment_plan',{installationId:id, systems})); setPlanBuilt(new Date()); }
    catch(e){ setError(true); setMessage(`Could not build the deployment plan: ${String(e)}`); }
    finally { setPlanLoading(null); }
  };
  const chooseSystems=(change:(selected:Set<string>)=>void)=>{
    if(!plan) return;
    const selected=new Set(plan.systems.filter(x=>x.selected).map(x=>x.system_folder)); change(selected);
    void showPlan(plan.installation_id, plan.systems.map(x=>x.system_folder).filter(x=>selected.has(x)));
  };
  const setArcade=async(id:string, system:string)=>{
    try { await invoke('set_arcade_system',{installationId:id, system: system||null}); await load();
      if(plan?.installation_id===id) await showPlan(id, plan.systems.filter(x=>x.selected).map(x=>x.system_folder));
      setError(false); setMessage(system?`Arcade games will go to roms\\${system}. Nothing has been copied.`:'Arcade games will not be placed.');
    } catch(e){ setError(true); setMessage(`Could not save the arcade choice: ${String(e)}`); }
  };
  const forgetInstallation=async(id:string)=>{
    if(forgetting!==id){ setForgetting(id); return; }
    try { await invoke<boolean>('remove_installation',{id}); setForgetting(null); if(plan?.installation_id===id) setPlan(null); setMessage('Installation forgotten. No files were touched.'); await load(); }
    catch(e){ setError(true); setMessage(`Could not forget the installation: ${String(e)}`); }
  };
  const startCopy=async()=>{
    if(!plan) return; setConfirming(false); setCopyResult(null);
    const systems=plan.systems.filter(x=>x.selected).map(x=>x.system_folder);
    try { setError(false); setMessage('Copying. Each file is verified before it gets its real name.');
      const jobId=await invoke<string>('start_deployment',{installationId:plan.installation_id, systems, fingerprint:plan.fingerprint, destination:plan.installation_path});
      if(!finishedJobs.current.has(jobId)) setCopy(c=>c??{job_id:jobId,folder:plan.installation_path});
    } catch(e){ setError(true); setMessage(`Could not start the copy: ${String(e)}`); }
  };
  const cancelCopy=async()=>{
    if(!copy) return;
    setCopy({...copy,cancelling:true}); setMessage('Cancelling the copy. The file in progress is discarded; verified files are kept.');
    try { await invoke<boolean>('cancel_job',{jobId:copy.job_id}); } catch(e){ setError(true); setMessage(`Could not cancel: ${String(e)}`); }
  };
  const loadReference=async()=>{ try { setRefStatus(await invoke<CatalogStatus|null>('reference_status')); } catch { /* shown when used */ } };
  useEffect(()=>{ if(native&&page==='Settings') void loadReference(); },[page]);
  const importReference=async()=>{
    try { const path=await open({multiple:false,directory:false,title:'Select your game catalog (.xlsx or .csv) or a genre supplement',filters:[{name:'Catalog',extensions:['xlsx','xlsm','csv']}]});
      if(!path||typeof path!=='string') return;
      setRefBusy(true); setRefResult(null); setError(false); setMessage('Importing the catalog and matching your games…');
      const r=await invoke<ImportSummary>('import_reference',{path}); setRefResult(r);
      const m=r.matched.by_name+r.matched.by_variant+r.matched.by_title;
      setMessage(r.kind==='genres'?`Added ${r.genres_added.toLocaleString()} genres from ${r.file_name}.`:`Imported ${r.entries.toLocaleString()} catalog entries; ${m.toLocaleString()} of your games are matched. Your files weren't changed.`);
      await loadReference(); setReloadKey(k=>k+1);
    } catch(e){ setError(true); setMessage(`Could not import the catalog: ${String(e)}`); }
    finally { setRefBusy(false); }
  };
  const removeReference=async()=>{
    if(!refRemoving){ setRefRemoving(true); return; }
    try { await invoke<boolean>('remove_reference'); setRefRemoving(false); setRefResult(null); await loadReference(); setReloadKey(k=>k+1); setMessage('Reference catalog removed. Your library and files are unchanged.'); }
    catch(e){ setError(true); setMessage(`Could not remove the catalog: ${String(e)}`); }
  };
  const setArt=async(g:Game)=>{
    try { const path=await open({multiple:false,directory:false,title:`Choose box art for ${displayTitle(g)}`,filters:[{name:'Images',extensions:['png','jpg','jpeg','webp','gif','bmp']}]});
      if(!path||typeof path!=='string') return;
      await invoke('set_game_art',{fileId:g.file_id, path}); setError(false); setMessage(`Box art set for ${displayTitle(g)}. The image stays where it is; nothing was copied.`); setReloadKey(k=>k+1);
    } catch(e){ setError(true); setMessage(`Could not set the box art: ${String(e)}`); }
  };
  const removeArt=async(g:Game)=>{
    try { await invoke<boolean>('remove_game_art',{fileId:g.file_id}); setMessage(`Your box art for ${displayTitle(g)} was removed (the image file wasn't touched).`); setReloadKey(k=>k+1); }
    catch(e){ setError(true); setMessage(`Could not remove the box art: ${String(e)}`); }
  };
  const cancelScan=async()=>{
    if(!scan) return;
    setScan({...scan,cancelling:true}); setMessage('Cancelling scan — finishing the current file...');
    try { await invoke<boolean>('cancel_job',{jobId:scan.job_id}); } catch(e){ setError(true); setMessage(`Could not cancel: ${String(e)}`); }
  };
  const copyProgressView = copy ? (()=>{ const p=copy.progress; const pct=p&&p.bytes_total>0?Math.min(100,p.bytes_done/p.bytes_total*100):0; return <section className="scan-progress" aria-live="polite"><div className="scan-head"><div><b>Copying to</b> <span className="scan-folder" title={copy.folder}>{copy.folder}</span></div><button className="ghost" disabled={copy.cancelling} onClick={()=>void cancelCopy()}><CircleStop size={16}/>{copy.cancelling?'Cancelling...':'Cancel copy'}</button></div>
      <div className="bar"><div style={{width:`${pct}%`}}/></div>
      <div className="scan-stats"><span><b>{(p?.games_done??0).toLocaleString()}</b> of {(p?.games_total??0).toLocaleString()} games</span><span><b>{(p?.files_done??0).toLocaleString()}</b> of {(p?.files_total??0).toLocaleString()} files</span><span><b>{fmtSize(p?.bytes_done??0)}</b> of {fmtSize(p?.bytes_total??0)}</span></div>
      <small className="scan-current">{p?.current||'Checking the drive and the plan...'}</small></section>; })() : null;
  return <div className="app">
    <aside className="sidebar"><div className="brand"><img className="brand-icon" src={iconUrl} alt="" width={46} height={46}/><div><strong>RETRODRIVE</strong><small>LIBRARY MANAGER</small></div></div>
      <div className="nav-label">WORKSPACE</div><nav>{menus.map(item=><button key={item.title} className={'nav-item '+(page===item.title?'selected':'')} onClick={()=>setPage(item.title)}><item.icon size={19}/>{item.title}</button>)}</nav>
      <div className="aside-bottom"><img className="sidebar-logo" src={sidebarArtUrl} alt="RetroDrive: retro cartridges and disks streaming onto a drive" width={192} height={192}/><div className="aside-status"><ShieldCheck size={17}/> Never overwrites or deletes</div><small>v0.1.0 · dev build</small></div>
    </aside>
    <main><header><div><div className="eyebrow">RETRO GAMING COMMAND CENTER</div><h1>{page}</h1></div><button className="ghost" onClick={()=>void load()}><RefreshCcw size={16}/> Refresh</button></header>
    <div className={'notice '+(error?'warn':'')}><Activity size={17}/>{message}</div>
    {scan && <section className="scan-progress" aria-live="polite"><div className="scan-head"><div><b>Scanning</b> <span className="scan-folder" title={scan.folder}>{scan.folder}</span></div><button className="ghost" disabled={scan.cancelling} onClick={()=>void cancelScan()}><CircleStop size={16}/>{scan.cancelling?'Cancelling...':'Cancel scan'}</button></div>
      <div className="bar indeterminate"><div/></div>
      <div className="scan-stats"><span><b>{(scan.progress?.entries_checked??0).toLocaleString()}</b> items checked</span><span><b>{(scan.progress?.indexed??0).toLocaleString()}</b> game files found</span><span><b>{(scan.progress?.new_files??0).toLocaleString()}</b> new</span>{(scan.progress?.issue_count??0)>0&&<span className="warn-text"><b>{scan.progress?.issue_count}</b> unreadable</span>}</div>
      <small className="scan-current" title={scan.progress?.current_path}>{scan.progress?.current_path||'Starting...'}</small></section>}
    {copyProgressView}
    {issues.length>0 && <details className="issues"><summary><AlertTriangle size={15}/> {issues.length} scan issue(s){issues.length>=200?' (first 200 shown)':''}</summary><ul>{issues.map((i,n)=><li key={n} className={'issue-'+i.severity}><b>{i.severity==='error'?'Error':'Warning'}:</b> <code>{i.path}</code> — {i.message}</li>)}</ul></details>}
    {page==='Dashboard' && <><section className="stats"><article><span>Cataloged games</span><strong>{(stats?.files??0).toLocaleString()}</strong><small>{stats?.platforms.length??0} platforms · local database</small></article><article><span>Detected volumes</span><strong>{drives.length}</strong><small>Read-only discovery</small></article><article><span>Available titles</span><strong>{(stats?.online??0).toLocaleString()}</strong><small>{stats&&(stats.offline||stats.missing)?`${stats.offline.toLocaleString()} offline · ${stats.missing.toLocaleString()} missing`:'Files currently accessible'}</small></article></section><div className="twocol"><section className="panel"><h2>Quick actions</h2><p>Get started with your existing library. Scanning never changes game files.</p><button className="primary" disabled={!native||busy||refBusy} onClick={importFolder}><FolderOpen size={18}/>{scan?'Scanning...':copy?'Copying...':'Import ROM folder'}</button><button className="subtle" onClick={()=>setPage('Game Library')}><LibraryBig size={18}/> Browse library</button>{(stats?.duplicate_files??0)>0&&<p className="dup-hint"><Copy size={15}/> {stats!.duplicate_files.toLocaleString()} files look like duplicate copies. <button className="link" onClick={reviewDuplicates}>Review them</button> (nothing is deleted).</p>}</section><section className="panel"><h2>Platform roadmap</h2><div className="platform-row"><span>Batocera</span><em>Adapter planned</em></div><div className="platform-row"><span>RetroBat</span><em>{installs.length?`${installs.length} installation${installs.length>1?'s':''} · plan & copy games`:'Recognize, plan & copy games'}</em></div><div className="platform-row"><span>More systems</span><em>Future milestone</em></div></section></div>
    <section className="panel activity"><h2>Recent activity</h2>{jobs.length===0?<p>No scans or copies yet.</p>:jobs.map(j=><div className="platform-row" key={j.id}><span><b className={'job '+j.state}>{jobStateLabel[j.state]??j.state}</b> <span className="job-folder" title={j.input.folder}>{j.kind==='deployment'?'Copy to ':''}{j.input.folder??j.kind}</span></span><em>{j.progress?.indexed!=null?`${j.progress.indexed} files · `:''}{j.progress?.files_copied!=null?`${j.progress.files_copied} files copied · `:''}{j.error_message?j.error_message+' · ':''}{new Date(j.started_at.replace(' ','T')+'Z').toLocaleString()}</em></div>)}</section></>}
    {page==='Game Library' && <section className="panel library-panel">
      <div className="section-heading"><div><h2>Master game catalog</h2><p>One searchable view across every imported folder and drive, including drives that aren't plugged in.</p></div><button className="primary" disabled={!native||busy||refBusy} onClick={importFolder}><FolderOpen size={17}/> Import folder</button></div>
      <div className="library-bar"><div className="tabs" role="tablist"><button role="tab" aria-selected={libraryMode==='systems'} className={libraryMode==='systems'?'on':''} onClick={()=>chooseMode('systems')}>Systems</button><button role="tab" aria-selected={libraryMode==='games'} className={libraryMode==='games'?'on':''} onClick={()=>chooseMode('games')}>{filters.platform?`Games · ${filters.platform}`:'All games'}</button></div>
        {libraryMode==='games'&&<div className="view-toggle" role="group" aria-label="View"><button className={libraryView==='grid'?'on':''} aria-pressed={libraryView==='grid'} title="Grid" onClick={()=>chooseView('grid')}><LayoutGrid size={16}/></button><button className={libraryView==='list'?'on':''} aria-pressed={libraryView==='list'} title="List" onClick={()=>chooseView('list')}><List size={16}/></button></div>}</div>
      {libraryMode==='systems' ? <div className="system-grid">{systems===null?<div className="empty">Loading systems…</div>:systems.length===0?<div className="empty"><Database size={26}/>No games yet. Import a ROM folder to begin.</div>:[...systems].sort((a,b)=>(['Unclassified','BIOS'].includes(a.platform)?1:0)-(['Unclassified','BIOS'].includes(b.platform)?1:0)||b.games-a.games).map(sp=><button key={sp.platform} className="system-card" style={{['--hue' as string]:platformHue(sp.platform)}} onClick={()=>{ updateFilters({platform:sp.platform}); chooseMode('games'); }}><span className="system-name">{sp.platform}</span><span className="system-count">{sp.games.toLocaleString()} {sp.platform==='BIOS'?(sp.games===1?'file':'files'):(sp.games===1?'game':'games')}</span><small>{fmtSize(sp.bytes)}{sp.platform==='Unclassified'?' · platform not recognized yet':sp.platform==='BIOS'?' · BIOS and firmware, not games':''}</small></button>)}</div> : <>
      <label className="search"><Search size={18}/><input placeholder="Search titles or systems" value={searchText} onChange={e=>setSearchText(e.target.value)} aria-label="Search titles or systems"/></label>
      <div className="filters">
        <select aria-label="Platform" value={filters.platform} onChange={e=>updateFilters({platform:e.target.value})}><option value="">All platforms</option>{stats?.platforms.map(p=><option key={p.platform} value={p.platform}>{p.platform} ({p.files.toLocaleString()})</option>)}</select>
        <select aria-label="Status" value={filters.availability} onChange={e=>updateFilters({availability:e.target.value as Filters['availability']})}><option value="">Any status</option><option value="online">Online</option><option value="offline">Offline</option><option value="missing">Missing</option></select>
        <select aria-label="Drive" value={filters.storage_id} onChange={e=>updateFilters({storage_id:e.target.value})}><option value="">All drives</option>{storage.map(d=><option key={d.id} value={d.id}>{d.volume_label??d.display_name}</option>)}</select>
        <select aria-label="Sort" value={filters.sort} onChange={e=>updateFilters({sort:e.target.value as Filters['sort']})}><option value="title">Sort: Title</option><option value="platform">Sort: Platform</option><option value="size">Sort: Largest first</option></select>
        <label className="check"><input type="checkbox" checked={filters.duplicates_only} onChange={e=>updateFilters({duplicates_only:e.target.checked})}/> Possible duplicates only</label>
        {filtersActive&&<button className="link" onClick={clearFilters}>Clear filters</button>}
      </div>
      {libraryView==='grid' ? <div className={'game-grid'+(libraryLoading?' loading':'')}>{library?.rows.map(g=><article key={g.id} className={'game-card '+g.availability} title={`${displayTitle(g)}${g.reference?.publisher?`\n${g.reference.publisher}`:''}\n${g.path}\n${availabilityLabel[g.availability]}`}>
        <div className="cover" style={{['--hue' as string]:platformHue(g.platform)}}><span className="cover-initials">{initials(displayTitle(g))}</span>{artUrl(g)&&<img className="cover-img" src={artUrl(g)!} alt="" loading="lazy" onError={e=>{e.currentTarget.style.display='none';}}/>}<span className="cover-platform">{g.platform}</span>{native&&<div className="art-actions"><button onClick={()=>void setArt(g)}>{g.art?'Change art':'Add art'}</button>{artSource(g)==='user'&&<button onClick={()=>void removeArt(g)}>Remove my art</button>}</div>}</div>
        <div className="card-body"><b>{displayTitle(g)}</b>{g.reference&&refLine(g.reference)&&<small className="ref-line">{refLine(g.reference)}</small>}<small>{fmtSize(g.file_size)} · <span className={'state '+g.availability}>{g.availability==='online'?'Online':g.availability==='offline'?'Offline':'Missing'}</span></small>
          <div className="card-badges">{g.disc_count>1&&<span className="disc-badge">{g.disc_count} discs</span>}{g.incomplete&&<span className="incomplete-badge">Incomplete</span>}{g.duplicate_count>1&&<span className="dup-badge" title="Possible duplicate copies (same platform, name and size)">×{g.duplicate_count}</span>}</div></div></article>)}
        {library&&library.rows.length===0&&<div className="empty"><Database size={26}/>{(stats?.files??0)===0?'No games yet. Import a ROM folder to begin.':<>No games match these filters. <button className="link" onClick={clearFilters}>Clear filters</button></>}</div>}</div>
      : <div className={'table-wrap'+(libraryLoading?' loading':'')}><table><thead><tr><th>Game</th><th>Platform</th><th>Year</th><th>Genre</th><th>Drive</th><th>Size</th><th>Availability</th></tr></thead><tbody>{library?.rows.map(g=><tr key={g.id}><td><b>{displayTitle(g)}{g.disc_count>1&&<span className="disc-badge" title="Multi-disc game: discs listed by its .m3u playlist">{g.disc_count} discs</span>}{g.incomplete&&<span className="incomplete-badge" title="A disc or track file this game needs is missing. See the scan warnings.">Incomplete</span>}{g.duplicate_count>1&&<span className="dup-badge" title={`Possible duplicate: ${g.duplicate_count} files share this platform, file name and size. Review before removing anything; RetroDrive never deletes files.`}>×{g.duplicate_count}</span>}</b><small title={g.path}>{shortPath(g.path)}</small></td><td>{g.platform}</td><td>{g.reference?.year??''}</td><td className="genre-cell" title={g.reference?.genre??''}>{g.reference?.genre??''}</td><td className="drive-cell" title={g.storage_name}>{g.storage_name}</td><td>{fmtSize(g.file_size)}</td><td><span className={'state '+g.availability}>{availabilityLabel[g.availability]}</span></td></tr>)}</tbody></table>
        {library&&library.rows.length===0&&<div className="empty"><Database size={26}/>{(stats?.files??0)===0?'No games yet. Import a ROM folder to begin.':<>No games match these filters. <button className="link" onClick={clearFilters}>Clear filters</button></>}</div>}</div>
      }
      {library&&library.total>0&&<div className="pager"><span>{library.rows.length?`${(library.offset+1).toLocaleString()}–${(library.offset+library.rows.length).toLocaleString()}`:'0'} of {library.total_capped?`${(library.total-1).toLocaleString()}+`:library.total.toLocaleString()}</span>
        <button className="ghost" disabled={offset===0||libraryLoading} onClick={()=>goToOffset(Math.max(0,offset-PAGE_SIZE))}><ChevronLeft size={16}/> Previous</button>
        <button className="ghost" disabled={library.rows.length<PAGE_SIZE||(!library.total_capped&&offset+PAGE_SIZE>=library.total)||libraryLoading} onClick={()=>goToOffset(offset+PAGE_SIZE)}>Next <ChevronRight size={16}/></button></div>}
      </>}
    </section>}
    {page==='Drive Manager' && <><section className="panel"><h2>Connected volumes</h2><p>Detected storage volumes are displayed for inspection. Formatting and image-writing are intentionally not implemented in this starter.</p><div className="drive-grid">{drives.map(d=><article className="drive" key={d.mount_point}><div className="drive-title"><HardDrive size={23}/><div><b>{d.name||d.mount_point}</b><small>{d.mount_point} {d.removable?'• Removable':''}</small></div></div><div className="bar"><div style={{width:`${d.total_bytes ? 100*(1-d.available_bytes/d.total_bytes):0}%`}}/></div><small>{bytes(Math.max(0,d.total_bytes-d.available_bytes))} used of {bytes(d.total_bytes)}</small></article>)}</div>{drives.length===0&&<div className="empty">No volumes reported by the desktop backend.</div>}</section>
    <section className="panel known-drives"><h2>Known drives</h2><p>Every drive the catalog has scanned, including ones that aren't plugged in. Drives are recognized by their volume ID, not their letter, so a drive that comes back as a different letter keeps its games.</p>
      {storage.length===0?<div className="empty">No drives scanned yet.</div>:storage.map(s=><article className="known-drive" key={s.id}>
        <div className="known-title"><HardDrive size={20}/><div><b>{s.volume_label??s.display_name}</b><small className={'presence '+s.presence}>{presenceText(s)}</small></div></div>
        <div className="known-meta"><span>{s.file_count.toLocaleString()} files{s.missing_count?` · ${s.missing_count.toLocaleString()} missing`:''}</span>{s.filesystem&&<span>{s.filesystem}</span>}{s.total_bytes!=null&&<span>{bytes(s.total_bytes)}</span>}<span>{s.kind}</span>{s.volume_serial&&<span title={s.stable_id??''}>ID {s.volume_serial}</span>}<span>Last scanned {sqlTime(s.last_seen_at)}</span></div>
      </article>)}
    </section></>}
    {page==='Emulation Systems' && <>
    <section className="panel">
      <div className="section-heading"><div><h2>Emulation systems</h2><p>Platforms RetroDrive works with. RetroDrive recognizes an installation, plans where your games would go, checks drive space and existing files, and copies only after you confirm. It never overwrites or deletes files on the drive.</p></div><button className="primary" disabled={!native} onClick={()=>void addInstallation()}><Plus size={17}/> Add existing installation</button></div>
      <div className="adapter-grid">
        {adapters.map(a=><article className="adapter" key={a.id}><b>{a.display_name}</b><small>{kindLabel[a.kind]??a.kind}</small>
          <ul className="caps"><li className={a.capabilities.detect?'yes':'no'}>Recognizes installations</li><li className={a.capabilities.validate_layout?'yes':'no'}>Checks folder layout</li><li className={a.capabilities.plan_placement?'yes':'no'}>Plans deployment (space and conflicts)</li><li className="yes">Copies games (verified; never overwrites)</li><li className="no">Installs RetroBat (needs RetroBat's written permission first)</li></ul></article>)}
        <article className="adapter planned"><b>Batocera</b><small>Bootable operating system</small><ul className="caps"><li className="no">Planned next (RDS-008)</li></ul></article>
      </div>
    </section>
    {inspection && <section className="panel">
      <h2>Folder check</h2><p className="mono">{inspection.path}</p>
      {inspection.best ? <>
        <div className="verdict ok"><CheckCircle2 size={18}/> {inspection.best.display_name} recognized{inspection.best.version?` (version ${inspection.best.version})`:' (version unknown)'}</div>
        <ul className="evidence">{inspection.best.evidence.map(e=><li key={e}>{e}</li>)}</ul>
        <Checks checks={inspection.checks}/>
        <div className="actions"><button className="primary" onClick={()=>void saveInspection()}><Plus size={17}/> Add this installation</button><button className="ghost" onClick={()=>setInspection(null)}>Cancel</button></div>
      </> : <>
        <div className="verdict warn"><AlertTriangle size={18}/> No supported platform was recognized in this folder.</div>
        {inspection.detections.filter(d=>d.confidence==='possible').map(d=><p key={d.adapter_id}>It looks partly like {d.display_name} ({d.evidence.join(', ')}). Choose the folder that contains RetroBat.exe.</p>)}
        <div className="actions"><button className="ghost" onClick={()=>setInspection(null)}>Close</button></div>
      </>}
    </section>}
    <section className="panel">
      <h2>Your installations</h2>
      {installs.length===0 ? <div className="empty">No installations yet. Click <b>&nbsp;Add existing installation&nbsp;</b> and pick your RetroBat folder.</div> : installs.map(ins=><article className="installation" key={ins.id}>
        <div className="installation-head"><div><b>{ins.adapter_name}</b><small className="mono">{ins.path}</small><small className={'presence '+ins.presence}>{installPresence[ins.presence]} · {ins.version?`Version ${ins.version}`:'Version unknown'}</small></div>
          <div className="actions"><button className="primary" disabled={ins.presence!=='connected'||planLoading!==null} onClick={()=>void showPlan(ins.id)}><Eye size={16}/>{planLoading===ins.id?'Planning...':'Plan deployment'}</button><button className="ghost" onClick={()=>void forgetInstallation(ins.id)} onBlur={()=>setForgetting(null)}>{forgetting===ins.id?'Click again to forget':'Forget'}</button></div></div>
        <div className="arcade-choice"><label>Arcade games go to <select value={ins.arcade_system??''} disabled={busy} onChange={e=>void setArcade(ins.id, e.target.value)}><option value="">Not chosen (arcade games aren't placed)</option><option value="mame">MAME (roms\mame)</option><option value="fbneo">FinalBurn Neo (roms\fbneo)</option></select></label>
          <small>Pick the emulator your arcade romset was made for: romset versions must match the emulator. RetroBat recommends non-merged sets and doesn't support merged ones. CHD disks aren't placed yet.</small></div>
        {ins.checks.length>0&&<Checks checks={ins.checks}/>}
      </article>)}
    </section>
    {plan && (()=>{ const c=plan.capacity; const g=plan.games; const verdict=c.fits===true?'ok':c.fits===false?'problem':'warn'; return <section className="panel">
      <h2>Deployment plan: {plan.adapter_name}</h2>
      <p>What copying your games to <span className="mono">{plan.installation_path}</span> would do. <b>Nothing has been copied yet.</b> When you copy, RetroDrive checks everything again first, verifies every file, and never overwrites or deletes anything on the drive.</p>
      <div className={'verdict '+verdict}>{verdict==='ok'?<CheckCircle2 size={18}/>:verdict==='problem'?<XCircle size={18}/>:<AlertTriangle size={18}/>}
        {c.fits===true?(plan.files_to_copy===0?' Nothing new to copy.':' Fits on this drive.'):c.fits===false?" Doesn't fit: free up space or choose fewer platforms.":" Free space couldn't be measured on this drive."}</div>
      {c.measured&&<div className="capacity"><div className="capacity-bar"><span className="need" style={{width:`${Math.min(100,c.required_bytes/Math.max(1,c.free_bytes)*100)}%`}}/><span className="reserve" style={{width:`${Math.min(100,c.reserve_bytes/Math.max(1,c.free_bytes)*100)}%`}}/></div>
        <div className="preview-summary"><span>Needs <b>{fmtSize(c.required_bytes)}</b></span><span>Free <b>{fmtSize(c.free_bytes)}</b> of {fmtSize(c.total_bytes)}</span><span>Kept free <b>{fmtSize(c.reserve_bytes)}</b> (safety reserve)</span>{c.filesystem&&<span>{c.filesystem}{c.max_file_bytes?` · max ${fmtSize(c.max_file_bytes)} per file`:''}</span>}</div></div>}
      <div className="plan-counts">
        <span className="pc copy"><b>{g.copy.toLocaleString()}</b> to copy<small>{plan.files_to_copy.toLocaleString()} files · {fmtSize(plan.bytes_to_copy)}</small></span>
        <span className="pc"><b>{g.already_there.toLocaleString()}</b> already there</span>
        {g.in_place>0&&<span className="pc"><b>{g.in_place.toLocaleString()}</b> already in place</span>}
        {g.duplicate>0&&<span className="pc"><b>{g.duplicate.toLocaleString()}</b> duplicates</span>}
        <span className={'pc'+(g.conflict?' conflict':'')}><b>{g.conflict.toLocaleString()}</b> conflicts (kept)</span>
        <span className={'pc'+(g.blocked?' blocked':'')}><b>{g.blocked.toLocaleString()}</b> blocked</span>
        {plan.unavailable_games>0&&<span className="pc"><b>{plan.unavailable_games.toLocaleString()}</b> left out<small>drive not connected or files missing</small></span>}
      </div>
      {(plan.folders_to_create>0||plan.long_paths>0)&&<p className="plan-notes">{plan.folders_to_create>0&&<>{plan.folders_to_create.toLocaleString()} new {plan.folders_to_create===1?'folder':'folders'} would be created. </>}{plan.long_paths>0&&<span className="warn-text">{plan.long_paths.toLocaleString()} {plan.long_paths===1?'path is':'paths are'} longer than 260 characters, which some emulators can't open.</span>}</p>}
      <h3>Platforms to include</h3>
      <div className="table-wrap"><table><thead><tr><th className="pick"><input type="checkbox" aria-label="All platforms" checked={plan.systems.length>0&&plan.systems.every(x=>x.selected)} disabled={planLoading!==null} onChange={e=>{const all=e.target.checked; chooseSystems(sel=>{sel.clear(); if(all) plan.systems.forEach(x=>sel.add(x.system_folder));});}}/></th><th>{plan.adapter_name} folder</th><th>Platforms</th><th>Games</th><th>Size</th></tr></thead><tbody>{plan.systems.map(sp=><tr key={sp.system_folder} className={sp.selected?'':'unpicked'}><td className="pick"><input type="checkbox" aria-label={sp.system_folder} checked={sp.selected} disabled={planLoading!==null} onChange={()=>chooseSystems(sel=>{ if(sel.has(sp.system_folder)) sel.delete(sp.system_folder); else sel.add(sp.system_folder); })}/></td><td><code>roms\{sp.system_folder}</code></td><td>{sp.platforms.join(', ')}</td><td>{sp.games.toLocaleString()}</td><td>{fmtSize(sp.bytes)}</td></tr>)}</tbody></table>{plan.systems.length===0&&<div className="empty">No games can be placed yet.</div>}</div>
      {plan.attention.length>0&&<><h3>Needs your attention</h3><p className="plan-notes">These games won't be copied. Files already on the drive are always kept.</p><div className="table-wrap"><table><thead><tr><th>Game</th><th>What happens</th><th>Why</th></tr></thead><tbody>{plan.attention.map(x=><tr key={x.source}><td><b>{x.title}</b><small title={`From: ${x.source}\nTo: ${x.destination}`}>From {shortPath(x.source)}</small></td><td><span className={'tag '+x.action}>{actionLabel[x.action]}</span></td><td>{x.reason}</td></tr>)}</tbody></table></div></>}
      {plan.unsupported.length>0&&<><h3>Not placed automatically</h3><ul className="unsupported">{plan.unsupported.map(u=><li key={u.platform+u.reason}><b>{u.platform}</b> ({u.games.toLocaleString()} {u.games===1?'game':'games'}): {u.reason}</li>)}</ul></>}
      {plan.sample.length>0&&<><h3>Examples</h3><div className="table-wrap"><table><thead><tr><th>Game</th><th>Would be copied to</th><th>Files</th><th>Size</th></tr></thead><tbody>{plan.sample.map(x=><tr key={x.source}><td><b>{x.title}</b><small title={x.source}>{shortPath(x.source)}</small></td><td className="mono" title={x.destination}>{shortPath(x.destination)}</td><td>{x.files}</td><td>{fmtSize(x.bytes_to_copy)}</td></tr>)}</tbody></table></div></>}
      <div className="copy-actions">{copy ? <div className="inline-progress">{copyProgressView}</div> : confirming ? <div className="confirm"><p><b>{g.copy>0?`Copy ${g.copy.toLocaleString()} ${g.copy===1?'game':'games'} (${fmtSize(plan.bytes_to_copy)})`:`Check ${g.already_there.toLocaleString()} ${g.already_there===1?'game':'games'} already`}</b> {g.copy>0?'to':'on'} <span className="mono">{plan.installation_path}</span>?</p><ul><li>Files already on the drive are never overwritten or deleted.</li><li>Every copy is read back and checked before it gets its real name.</li>{g.already_there>0&&<li>{g.already_there.toLocaleString()} {g.already_there===1?'game':'games'} already there will be checked by content.</li>}<li>You can cancel at any time; verified games are kept.</li></ul><div className="actions"><button className="primary" onClick={()=>void startCopy()}><Copy size={16}/> {g.copy>0?'Yes, copy now':'Yes, check them'}</button><button className="ghost" onClick={()=>setConfirming(false)}>Not now</button></div></div>
        : <button className="primary" disabled={!native||busy||planLoading!==null||c.fits!==true||g.copy+g.already_there===0} onClick={()=>setConfirming(true)}><Copy size={16}/>{g.copy>0?`Copy ${g.copy.toLocaleString()} ${g.copy===1?'game':'games'}`:'Check the games already there'}</button>}
        {!copy&&c.fits!==true&&<small className="warn-text">Copying needs a plan that fits on the drive.</small>}</div>
      <small className="plan-id">Plan {plan.fingerprint}{planBuilt?` · built ${planBuilt.toLocaleTimeString()}`:''}. Build it again if the drive changes.</small>
    </section>; })()}
    {copyResult && <section className="panel" ref={resultRef}>
      <h2>Copy result</h2>
      {copyResult.summary ? (()=>{ const r=copyResult.summary; return <>
        <div className={'verdict '+(r.status==='completed'?'ok':r.status==='completed_with_issues'?'warn':'problem')}>{r.status==='completed'?<CheckCircle2 size={18}/>:<AlertTriangle size={18}/>} {copyHeadline[r.status]}</div>
        <div className="plan-counts"><span className="pc copy"><b>{r.games_copied.toLocaleString()}</b> games copied<small>{r.files_copied.toLocaleString()} files · {fmtSize(r.bytes_copied)}</small></span><span className="pc"><b>{r.games_already_there.toLocaleString()}</b> already there<small>checked by content</small></span>{r.games_incomplete>0&&<span className="pc conflict"><b>{r.games_incomplete}</b> incomplete<small>run the copy again to finish</small></span>}{r.games_conflict>0&&<span className="pc conflict"><b>{r.games_conflict}</b> conflicts<small>existing files kept</small></span>}{r.files_not_attempted>0&&<span className="pc"><b>{r.files_not_attempted}</b> files not reached</span>}</div>
        {r.issues.length>0&&<div className="table-wrap"><table><thead><tr><th>Game</th><th>What happened</th></tr></thead><tbody>{r.issues.map((x,n)=><tr key={n}><td><b>{x.game}</b><small title={x.path}>{shortPath(x.path)}</small></td><td>{x.message}</td></tr>)}</tbody></table></div>}
        {r.manifest_path&&<p className="plan-notes">A record of this copy was saved to <span className="mono">{shortPath(r.manifest_path)}</span>.</p>}{r.manifest_error&&<p className="warn-text">{r.manifest_error}</p>}
      </>; })() : <div className="verdict problem"><XCircle size={18}/> {copyResult.error}</div>}
      <div className="actions"><button className="ghost" onClick={()=>setCopyResult(null)}>Close</button></div>
    </section>}
    </>}
    {page==='Settings' && <section className="panel"><h2>Reference catalog</h2>
      <p>Import your own game catalog (.xlsx or .csv) to show proper titles, years, genres and publishers. RetroDrive matches games by their exact No-Intro or MAME name first, then by title on the same system. Only names and details are imported; <b>art columns are never read</b>. Your files are never changed, and removing the catalog leaves your library as it was.</p>
      {refStatus ? <div className="ref-status"><div><b>{refStatus.name}</b><small>{refStatus.file_name} · {refStatus.entries.toLocaleString()} entries{refStatus.genres_added?` · ${refStatus.genres_added.toLocaleString()} genres added`:''} · imported {sqlTime(refStatus.imported_at)}</small></div>
        <div className="plan-counts"><span className="pc copy"><b>{(refStatus.matched.by_name+refStatus.matched.by_variant+refStatus.matched.by_title).toLocaleString()}</b> games matched</span><span className="pc"><b>{refStatus.matched.by_name.toLocaleString()}</b> by exact name</span><span className="pc"><b>{refStatus.matched.by_variant.toLocaleString()}</b> by MAME clone set</span><span className="pc"><b>{refStatus.matched.by_title.toLocaleString()}</b> by title</span></div>
        <div className="table-wrap"><table><thead><tr><th>System</th><th>Games</th><th>Matched</th></tr></thead><tbody>{refStatus.platforms.map(p=><tr key={p.platform}><td>{p.platform}</td><td>{p.games.toLocaleString()}</td><td>{p.matched.toLocaleString()} ({p.games?Math.round(100*p.matched/p.games):0}%)</td></tr>)}</tbody></table></div></div>
      : <div className="empty">No reference catalog imported.</div>}
      {busy&&<p className="plan-notes warn-text">A scan or copy is running. You can import or remove the catalog when it finishes.</p>}
      {refResult&&refResult.ignored_columns.length>0&&<p className="plan-notes">Not imported (by design): {refResult.ignored_columns.join(', ')}.{refResult.skipped_rows?` ${refResult.skipped_rows.toLocaleString()} rows had no title or system and were skipped.`:''}</p>}
      <div className="actions"><button className="primary" disabled={!native||refBusy||busy} title={busy?'Available when the scan or copy finishes':undefined} onClick={()=>void importReference()}><FolderOpen size={17}/>{refBusy?'Importing…':refStatus?'Import a catalog or genre file':'Import catalog'}</button>{refStatus&&<button className="ghost" disabled={refBusy||busy} onClick={()=>void removeReference()} onBlur={()=>setRefRemoving(false)}>{refRemoving?'Click again to remove':'Remove catalog'}</button>}</div>
      <h2 className="legal-heading">Application settings</h2><p>RetroDrive currently stores its SQLite catalog in the operating system's application data folder. Imports index files in place and never copy, move, or delete them.</p><p>Future settings: metadata providers, library scan schedules, backup locations, and platform defaults.</p><h2 className="legal-heading">About and legal</h2><p>RetroDrive doesn't include or download games, BIOS or firmware. It only organizes and copies files you choose, so use only files you have the right to use. RetroDrive is for your own personal use; don't use it to prepare drives or machines with games or third-party software for others or for sale.</p><p className="legal-notice">RetroDrive is an independent retro gaming management application. All third-party product names and trademarks belong to their respective owners. Compatibility references do not imply sponsorship, affiliation, or endorsement.</p><small className="legal-draft">Draft wording from the legal assessment (docs/16), pending counsel review.</small></section>}
    <footer>RetroDrive · Early development build · Copies only after you confirm; never overwrites or deletes</footer>
    </main></div>;
}
