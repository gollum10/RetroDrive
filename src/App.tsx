import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import sidebarArtUrl from './assets/retrodrive-sidebar-art.png';
import iconUrl from './assets/retrodrive-icon.png';
import Customize from './Customize';
import ThemeStudio, { type StudioLeaveGuard } from './ThemeStudio';
import InstallHelper from './InstallHelper';
import Controllers from './Controllers';
import SystemBackup, { BACKUP_ADAPTERS } from './SystemBackup';
import BackupSettings from './BackupSettings';
import CatalogTools from './CatalogTools';
import ControllerWizard from './ControllerWizard';
import SetupWizard from './SetupWizard';
import HealthPanel from './HealthPanel';
import { fmtSize, localTime, shortPath } from './format';
import { DecisionsLine, DecisionsPanel, type KeptOffChange } from './Decisions';

/** Platforms with game lists RetroDrive fills in and keeps up to date (Sync, art and details). */
const GAMELIST_PLATFORMS=['retrobat','batocera','retropie','esde','recalbox','rocknix'];
import { Activity, HardDriveDownload, AlertTriangle, CheckCircle2, ChevronLeft, ChevronRight, CircleStop, Copy, RefreshCw, Database, Eye, History, Palette, Plus, Undo2, XCircle, FolderOpen, Gamepad2, HardDrive, LayoutDashboard, LayoutGrid, LibraryBig, List, RefreshCcw, Search, Settings2, ShieldCheck, Download, Joystick, Crosshair, RectangleVertical, Star, Sparkles, Wand2 } from 'lucide-react';

type Availability = 'online'|'offline'|'missing';
type Game = { id: string; game_id: string; title: string; platform: string; path: string; file_size: number; disc_count: number; incomplete: boolean; storage_name: string; availability: Availability; duplicate_count: number; reference: RefInfo|null; file_id: string; art: string|null; lightgun: boolean; vertical: boolean; favorite: boolean; description: string|null; players: string|null; rating: number|null };
// "1-2 players · ★ 4.5" from a game's gamelist details.
const detailLine = (g: { players: string|null; rating: number|null }) => [g.players ? `${g.players.replace('-', '–')} player${g.players === '1' ? '' : 's'}` : '', g.rating != null ? `★ ${(g.rating * 5).toFixed(1).replace(/\.0$/, '')}` : ''].filter(Boolean).join(' · ');
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
type LibraryStats = { files: number; online: number; offline: number; missing: number; duplicate_files: number; left_out: number; platforms: { platform: string; files: number }[] };
type CleanupPreview = { groups: number; files_to_leave_out: number; bytes_to_leave_out: number; examples: { title: string; keep: string; leave_out: string[] }[] };
type SystemSummary = { platform: string; games: number; bytes: number };
type Filters = { text: string; platform: string; availability: ''|Availability|'left_out'; storage_id: string; duplicates_only: boolean; lightgun_only: boolean; vertical_only: boolean; genre: string; year_from: string; year_to: string; players_min: string; sort: 'title'|'platform'|'size'; letter: string; collection_id: string };
const noFilters: Filters = { text:'', platform:'', availability:'', storage_id:'', duplicates_only:false, lightgun_only:false, vertical_only:false, genre:'', year_from:'', year_to:'', players_min:'', sort:'title', letter:'', collection_id:'' };
const LETTERS = ['#', ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'];
const initialOf = (name: string) => { const c = name.trim().charAt(0).toUpperCase(); return c >= 'A' && c <= 'Z' ? c : '#'; };
const PAGE_SIZE = 100;
type LibraryMode = 'systems'|'collections'|'games';
type CollectionRow = { id: string; name: string; games: number; bytes: number; created_at: string; kind: 'list'|'favorites'|'smart'; rule: Record<string, unknown>|null };
// What a smart collection's saved filter picks, in words.
const ruleSummary = (r: Record<string, unknown>|null) => { if (!r) return ''; const p: string[] = []; if (r.platform) p.push(String(r.platform)); if (r.genre) p.push(`genre ${String(r.genre)}`); if (r.year_from || r.year_to) p.push(`${r.year_from ?? '…'}–${r.year_to ?? '…'}`); if (r.players_min) p.push(`${String(r.players_min)}+ players`); if (r.lightgun_only) p.push('light gun'); if (r.vertical_only) p.push('vertical'); if (r.text) p.push(`“${String(r.text)}”`); if (r.letter) p.push(`starting with ${String(r.letter)}`); return p.join(' · '); }; type LibraryView = 'list'|'grid';
function remembered<T extends string>(key: string, allowed: readonly T[], fallback: T): T { try { const v = localStorage.getItem(key); return v && (allowed as readonly string[]).includes(v) ? v as T : fallback; } catch { return fallback; } }
function remember(key: string, value: string) { try { localStorage.setItem(key, value); } catch { /* storage unavailable: the choice just isn't remembered */ } }
/** A stable colour per platform for placeholder tiles (until box art is added). */
function platformHue(name: string): number { let h = 0; for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360; return h; }
function initials(title: string): string { const words = title.replace(/\(.*?\)|\[.*?\]/g, '').split(/[\s:\-_]+/).filter(w => /[A-Za-z0-9]/.test(w)); return (words.slice(0, 2).map(w => w[0]).join('') || '?').toUpperCase(); }
type AdapterInfo = { id: string; display_name: string; kind: 'portable_app'|'bootable_os'|'frontend'; capabilities: { detect: boolean; validate_layout: boolean; plan_placement: boolean; copy_games: boolean; install: boolean; write_files: boolean }; contract_version: number };
type Detection = { adapter_id: string; display_name: string; kind: string; confidence: 'not_detected'|'possible'|'detected'; evidence: string[]; version: string|null; hint: string|null };
type LayoutCheck = { id: string; label: string; status: 'ok'|'warning'|'problem'; detail: string };
type Inspection = { path: string; detections: Detection[]; best: Detection|null; checks: LayoutCheck[] };
type BackupRow = { item_id: number; game_title: string; path: string; state: 'replaced'|'undone'; backup: string|null; backup_bytes: number|null; when: string; blocked: string|null };
type BackupList = { installation_path: string; connected: boolean; rows: BackupRow[]; folder_bytes: number };
type Installation = { id: string; adapter_id: string; adapter_name: string; path: string; version: string|null; presence: 'connected'|'not_connected'|'other_volume'; checks: LayoutCheck[]; added_at: string; arcade_system: 'mame'|'fbneo'|null };
type PlanAction = 'in_place'|'already_there'|'duplicate'|'skipped'|'kept_off'|'removed'|'same_game'|'copy'|'replace'|'conflict'|'blocked';
export type ChoiceInfo = { key: string; situation: 'conflict'|'clash'|'same_game'|'removed'|'kept_off'; options: ('keep'|'use'|'keep_both'|'replace'|'keep_off')[]; current: 'keep'|'use'|'keep_both'|'replace'; file_id: string };
export type Choice = { kind: 'keep'|'keep_both'|'replace' } | { kind: 'use'; file_id: string };
export type PlanGame = { title: string; platform: string; system_folder: string; action: PlanAction; reason: string|null; source: string; destination: string; files: number; bytes: number; bytes_to_copy: number; choice: ChoiceInfo|null };
export type DeploymentPlan = { format_warnings?: { system_folder: string; extension: string; games: number; accepted: string }[]; installation_id: string; adapter_id: string; adapter_name: string; installation_path: string; copy_supported: boolean; systems_prefix: string; systems: { system_folder: string; platforms: string[]; games: number; bytes: number; selected: boolean }[]; unsupported: { platform: string; reason: string; games: number }[]; unavailable_games: number; games: Record<PlanAction,number>; selected_games: number; files_to_copy: number; bytes_to_copy: number; folders_to_create: number; long_paths: number; capacity: { measured: boolean; filesystem: string|null; free_bytes: number; total_bytes: number; reserve_bytes: number; staging_bytes: number; required_bytes: number; fits: boolean|null; max_file_bytes: number|null }; attention: PlanGame[]; same_game: PlanGame[]; removed: PlanGame[]; sample: PlanGame[]; fingerprint: string };
const actionLabel: Record<PlanAction,string> = { copy:'Would be copied', already_there:'Already there (skipped)', in_place:'Already in place', duplicate:'Duplicate (copied once)', same_game:'Already there under another name', removed:'Removed from the drive', kept_off:'Kept off the drive', conflict:'Conflict: existing file kept', blocked:'Blocked', skipped:'Skipped (your choice)', replace:'Replace (original backed up)' };
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
type DeploySummary = { deployment_id: string; status: 'completed'|'completed_with_issues'|'cancelled'|'interrupted'; games_copied: number; games_already_there: number; games_incomplete: number; games_conflict: number; files_copied: number; files_replaced: number; bytes_copied: number; files_identical: number; files_failed: number; files_not_attempted: number; cancelled: boolean; drive_lost: boolean; destination: string; manifest_path: string|null; manifest_error: string|null; issues: { game: string; path: string; message: string }[] };
type ActiveCopy = { job_id: string; folder: string; progress?: DeployProgress; cancelling?: boolean };
type CopyResult = { summary: DeploySummary|null; error: string|null; art?: string|null; controllers?: string|null };
type InstallationControllers = { confirmed: number; to_add: string[]; manual: string[]; manual_message: string|null };
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
type Page = 'Dashboard'|'Game Library'|'Drive Manager'|'Emulation Systems'|'Controllers'|'Customize'|'Theme Studio'|'Settings';
const menus = [
  {title:'Dashboard',icon:LayoutDashboard}, {title:'Game Library',icon:LibraryBig},
  {title:'Drive Manager',icon:HardDrive}, {title:'Emulation Systems',icon:Gamepad2}, {title:'Controllers',icon:Joystick}, {title:'Customize',icon:Palette}, {title:'Theme Studio',icon:Palette}, {title:'Settings',icon:Settings2},
] as const;
const bytes = (n:number) => `${(n/1024/1024/1024).toFixed(1)} GB`;
export default function App() {
  const [page,setPageRaw]=useState<Page>('Dashboard');
  const studioLeave: StudioLeaveGuard = useRef(null);
  const navigating = useRef(false);
  const setPage = (next: Page) => { void (async () => {
    if (navigating.current) return;
    navigating.current = true;
    try { if (page !== 'Theme Studio' || next === page || !studioLeave.current || await studioLeave.current()) setPageRaw(next); }
    finally { navigating.current = false; }
  })(); };
  const [stats,setStats]=useState<LibraryStats|null>(null); const [drives,setDrives]=useState<Drive[]>([]); const [storage,setStorage]=useState<StorageRow[]>([]);
  const [libraryMode,setLibraryMode]=useState<LibraryMode>(()=>remembered('rd.libraryMode',['systems','collections','games'] as const,'systems'));
  const [libraryView,setLibraryView]=useState<LibraryView>(()=>remembered('rd.libraryView',['list','grid'] as const,'grid'));
  const chooseMode=(m:LibraryMode)=>{ setLibraryMode(m); remember('rd.libraryMode',m); };
  const [systems,setSystems]=useState<SystemSummary[]|null>(null);
  const [collectionList,setCollectionList]=useState<CollectionRow[]>([]); const [newCollection,setNewCollection]=useState(''); const [renaming,setRenaming]=useState<{id:string;name:string}|null>(null); const [deletingCollection,setDeletingCollection]=useState<string|null>(null);
  const [addTarget,setAddTarget]=useState<{mode:'one';game:Game}|{mode:'all'}|null>(null); const [planCollection,setPlanCollection]=useState('');
  const loadCollections=async()=>{ if(!native) return; try { setCollectionList(await invoke<CollectionRow[]>('list_collections')); } catch { /* shown when used */ } };
  useEffect(()=>{ void loadCollections(); },[page,libraryMode]);
  const collectionName=(id:string)=>collectionList.find(c=>c.id===id)?.name??'collection';
  const currentQuery=()=>({ text:filters.text||null, platform:filters.platform||null, availability:filters.availability&&filters.availability!=='left_out'?filters.availability:null, left_out:filters.availability==='left_out', letter:filters.letter||null, collection_id:filters.collection_id||null, storage_id:filters.storage_id||null, duplicates_only:filters.duplicates_only, lightgun_only:filters.lightgun_only, vertical_only:filters.vertical_only, genre:filters.genre||null, year_from:filters.year_from?Number(filters.year_from):null, year_to:filters.year_to?Number(filters.year_to):null, players_min:filters.players_min?Number(filters.players_min):null, sort:filters.sort, offset:0 });
  const createCollection=async(name:string)=>{ try { const id=await invoke<string>('create_collection',{name}); setNewCollection(''); await loadCollections(); return id; } catch(e){ setError(true); setMessage(`Could not create the collection: ${String(e)}`); return null; } };
  const renameCollection=async()=>{ if(!renaming) return; try { await invoke('rename_collection',{id:renaming.id,name:renaming.name}); setRenaming(null); await loadCollections(); } catch(e){ setError(true); setMessage(`Could not rename the collection: ${String(e)}`); } };
  const deleteCollection=async(c:CollectionRow)=>{ if(deletingCollection!==c.id){ setDeletingCollection(c.id); return; } try { await invoke('delete_collection',{id:c.id}); setDeletingCollection(null); if(filters.collection_id===c.id) updateFilters({collection_id:''}); setError(false); setMessage(`Collection "${c.name}" deleted. Its games are still in your library.`); await loadCollections(); } catch(e){ setError(true); setMessage(`Could not delete the collection: ${String(e)}`); } };
  const openCollection=(c:CollectionRow)=>{ setSearchText(''); setFilters(f=>({...noFilters,sort:f.sort,collection_id:c.id})); setOffset(0); chooseMode('games'); };
  const addToCollection=async(id:string,name?:string)=>{ if(!addTarget) return; try { const n=await invoke<number>('add_to_collection', addTarget.mode==='one'?{id, fileIds:[addTarget.game.file_id]}:{id, query:currentQuery()}); setAddTarget(null); setError(false); setMessage(`Added ${n.toLocaleString()} ${n===1?'game':'games'} to "${name??collectionName(id)}".${addTarget.mode==='all'&&n>=10000?' (At most 10,000 at a time.)':''}`); await loadCollections(); } catch(e){ setError(true); setMessage(`Could not add to the collection: ${String(e)}`); } };
  const removeFromCollection=async(g:Game)=>{ if(!filters.collection_id) return; try { await invoke<number>('remove_from_collection',{id:filters.collection_id, fileIds:[g.file_id]}); setMessage(`${displayTitle(g)} removed from "${collectionName(filters.collection_id)}" (it's still in your library).`); await loadCollections(); setReloadKey(k=>k+1); } catch(e){ setError(true); setMessage(`Could not remove it: ${String(e)}`); } };
  useEffect(()=>{ if(page!=='Game Library'||libraryMode!=='systems'||!native) return; let live=true;
    invoke<SystemSummary[]>('system_summary').then(r=>{ if(live) setSystems(r); }).catch(e=>{ if(live){ setError(true); setMessage(`Could not load systems: ${String(e)}`); } });
    return ()=>{ live=false; }; },[page,libraryMode,stats]);
  const chooseView=(v:LibraryView)=>{ setLibraryView(v); remember('rd.libraryView',v); };
  const [filters,setFilters]=useState<Filters>(noFilters); const [searchText,setSearchText]=useState(''); const [offset,setOffset]=useState(0);
  const [refStatus,setRefStatus]=useState<CatalogStatus|null>(null); const [refBusy,setRefBusy]=useState(false); const [refResult,setRefResult]=useState<ImportSummary|null>(null); const [refRemoving,setRefRemoving]=useState(false);
  const [genres,setGenres]=useState<string[]>([]); const [smartName,setSmartName]=useState<string|null>(null);
  const [library,setLibrary]=useState<GamePage|null>(null); const [libraryLoading,setLibraryLoading]=useState(false); const [reloadKey,setReloadKey]=useState(0);
  const requestId=useRef(0); const finishedJobs=useRef(new Set<string>()); const resultRef=useRef<HTMLElement|null>(null);
  const [adapters,setAdapters]=useState<AdapterInfo[]>([]); const [installs,setInstalls]=useState<Installation[]>([]);
  const [inspection,setInspection]=useState<Inspection|null>(null); const [plan,setPlan]=useState<DeploymentPlan|null>(null);
  const [planLoading,setPlanLoading]=useState<string|null>(null); const [planBuilt,setPlanBuilt]=useState<Date|null>(null); const [forgetting,setForgetting]=useState<string|null>(null);
  const [scan,setScan]=useState<ActiveScan|null>(null); const [jobs,setJobs]=useState<JobRecord[]>([]);
  const [copy,setCopy]=useState<ActiveCopy|null>(null);
  const planInstallation=plan?.installation_id; const [copyResult,setCopyResult]=useState<CopyResult|null>(null); const [confirming,setConfirming]=useState(false); const [addArt,setAddArt]=useState(true); const [setupControllers,setSetupControllers]=useState(true); const [ctrl,setCtrl]=useState<InstallationControllers|null>(null);
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
  useEffect(()=>{ if(!confirming||!planInstallation) return; let live=true; void invoke<InstallationControllers>('controllers_for_installation',{installationId:planInstallation}).then(c=>{ if(live) setCtrl(c); }).catch(()=>{ if(live) setCtrl(null); }); return ()=>{ live=false; }; },[confirming,planInstallation]);
  useEffect(()=>{ const t=setTimeout(()=>updateFilters({text:searchText}),250); return ()=>clearTimeout(t); },[searchText]);
  useEffect(()=>{
    if(!native) return;
    const id=++requestId.current; setLibraryLoading(true);
    const query={ text:filters.text||null, platform:filters.platform||null, availability:filters.availability&&filters.availability!=='left_out'?filters.availability:null, left_out:filters.availability==='left_out', letter:filters.letter||null, collection_id:filters.collection_id||null, storage_id:filters.storage_id||null, duplicates_only:filters.duplicates_only, lightgun_only:filters.lightgun_only, vertical_only:filters.vertical_only, genre:filters.genre||null, year_from:filters.year_from?Number(filters.year_from):null, year_to:filters.year_to?Number(filters.year_to):null, players_min:filters.players_min?Number(filters.players_min):null, sort:filters.sort, offset, limit:PAGE_SIZE };
    invoke<GamePage>('query_games',{query})
      .then(p=>{ if(id===requestId.current) setLibrary(p); })
      .catch(e=>{ if(id===requestId.current){ setError(true); setMessage(`Could not search the library: ${String(e)}`); } })
      .finally(()=>{ if(id===requestId.current) setLibraryLoading(false); });
  },[filters,offset,reloadKey]);
  const filtersActive = filters.collection_id!==''||filters.letter!==''||filters.text!==''||filters.platform!==''||filters.availability!==''||filters.storage_id!==''||filters.duplicates_only||filters.lightgun_only||filters.vertical_only||filters.genre!==''||filters.year_from!==''||filters.year_to!==''||filters.players_min!=='';
  const clearFilters=()=>{ setSearchText(''); setFilters(f=>({...noFilters,sort:f.sort})); setOffset(0); };
  const goToOffset=(next:number)=>{ setOffset(next); document.querySelector('.library-panel')?.scrollIntoView({block:'start'}); };
  const [cleanup,setCleanup]=useState<CleanupPreview|null>(null); const [cleanupBusy,setCleanupBusy]=useState(false);
  const previewCleanup=async()=>{ setPage('Game Library'); setCleanupBusy(true); try { setCleanup(await invoke<CleanupPreview>('duplicate_cleanup_preview')); } catch(e){ setError(true); setMessage(`Could not check duplicates: ${String(e)}`); } finally { setCleanupBusy(false); } };
  const applyCleanup=async()=>{ setCleanupBusy(true); try { const n=await invoke<number>('clean_up_duplicates'); setCleanup(null); setError(false); setMessage(`Left ${n.toLocaleString()} duplicate files out of the library. They're still on your drives; choose Status: Left out of library to see or bring them back.`); await load(); setReloadKey(k=>k+1); } catch(e){ setError(true); setMessage(`Could not clean up duplicates: ${String(e)}`); } finally { setCleanupBusy(false); } };
  const setLeftOut=async(g:Game, leftOut:boolean)=>{ try { await invoke<number>('set_left_out',{fileIds:[g.file_id], leftOut}); setError(false); setMessage(leftOut?`${displayTitle(g)} was left out of the library. The file is still on your drive.`:`${displayTitle(g)} is back in the library.`); await load(); setReloadKey(k=>k+1); } catch(e){ setError(true); setMessage(`Could not change ${displayTitle(g)}: ${String(e)}`); } };
  useEffect(()=>{ if(native) void invoke<string[]>('library_genres').then(setGenres).catch(()=>{}); },[native]);
  const toggleFavorite=async(g:Game)=>{ try { await invoke('set_favorite',{fileId:g.file_id, favorite:!g.favorite}); setLibrary(l=>l&&{...l, rows:l.rows.map(r=>r.file_id===g.file_id?{...r,favorite:!g.favorite}:r)}); if(filters.collection_id) setReloadKey(k=>k+1); void loadCollections(); } catch(e){ setError(true); setMessage(`Could not change Favorites: ${String(e)}`); } };
  const saveSmart=async()=>{ if(!smartName?.trim()) return; try { await invoke<string>('create_smart_collection',{name:smartName, query:currentQuery()}); setError(false); setMessage(`“${smartName.trim()}” saved as a smart collection. It fills itself from these filters, now and as you add games.`); setSmartName(null); await loadCollections(); } catch(e){ setError(true); setMessage(`Could not save the smart collection: ${String(e)}`); } };
  const showGunGames=()=>{ setSearchText(''); setFilters({...noFilters,lightgun_only:true}); setOffset(0); chooseMode('games'); setPage('Game Library'); };
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
    const copyFinished=listen<{job_id:string;state:string;summary:DeploySummary|null;error:string|null;art:string|null;controllers:string|null}>('deploy-finished',e=>{
      const f=e.payload; finishedJobs.current.add(f.job_id); setCopy(null); setPlan(null); setCopyResult({summary:f.summary,error:f.error,art:f.art,controllers:f.controllers});
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
  const [installHelp,setInstallHelp]=useState(false);
  // Setup wizard (ADR-100): optional. Offered once on first start when no system is set
  // up; after that only when the user clicks Set up a system.
  const WIZARD_OFFERED='rd.wizardOffered';
  const [wizard,setWizard]=useState<{first?:boolean; task:string; collection?:string}|null>(null);
  useEffect(()=>{ if(!native||stats===null||adapters.length===0||installs.length>0||wizard) return; /* adapters and installs load together */ let offered=false; try { offered=localStorage.getItem(WIZARD_OFFERED)==='1'; localStorage.setItem(WIZARD_OFFERED,'1'); } catch { offered=true; } if(!offered) setWizard({first:true, task:'everything'}); },[stats,adapters,installs]);
  const wizardButton=<button className="ghost" disabled={!native} onClick={()=>setWizard(w=>w?.task==='everything'?null:{task:'everything'})} title="A step-by-step guide to setting up a system (optional)"><Wand2 size={17}/> Set up a system</button>;
  const addInstallation=async()=>{
    try { const folder=await open({directory:true,multiple:false,title:'Select your system: a RetroBat folder, or a game share such as \\\\BATOCERA\\share, \\\\KNULLI\\share, \\\\RETROPIE\\roms, \\\\ROCKNIX\\games-roms or \\\\LAKKA\\ROMs'});
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
  const [choices,setChoices]=useState<Record<string,Choice>>({});
  const [syncMode,setSyncMode]=useState(false); const [syncPlatforms,setSyncPlatforms]=useState(false);
  // What Sync keeps up to date (ADR-045): the platforms and collections copied from plans.
  const [syncScope,setSyncScope]=useState<{systems:string[];collections:string[]}>({systems:[],collections:[]});
  // Sync (ADR-043): the saved selection (every platform except those left out) brought up to date.
  const [decisionsOpen,setDecisionsOpen]=useState(false);
  // Bring a newly opened plan or sync into view, so nobody has to scroll to find it.
  const planRef=useRef<HTMLElement>(null); const [revealPlan,setRevealPlan]=useState(false);
  useEffect(()=>{ if(revealPlan&&plan){ planRef.current?.scrollIntoView({behavior:'smooth',block:'start'}); setRevealPlan(false); } },[plan,revealPlan]);
  // Saved decisions (ADR-044): loaded when a plan or sync is opened, saved on every change.
  const savedChoices=async(id:string)=>{ try { return await invoke<Record<string,Choice>>('saved_choices',{installationId:id}); } catch { return {}; } };
  const openSync=async(id:string)=>{ setDecisionsOpen(false); setSyncPlatforms(false); await showSync(id, await savedChoices(id)); setRevealPlan(true); };
  // A plan starts with nothing ticked: the user chooses what goes on the drive (ADR-045).
  const openPlan=async(id:string)=>{ setDecisionsOpen(false); await showPlan(id, [], await savedChoices(id), ''); setRevealPlan(true); };
  const showSync=async(id:string, nextChoices:Record<string,Choice>={})=>{
    setPlanLoading(id); setConfirming(false); setCopyResult(null); setChoices(nextChoices); setPlanCollection(''); setSyncMode(true); setAddArt(true); setSetupControllers(true); setCtrl(null);
    try { setSyncScope(await invoke('sync_scope',{installationId:id})); setPlan(await invoke<DeploymentPlan>('sync_plan',{installationId:id, choices:nextChoices})); setPlanBuilt(new Date()); }
    catch(e){ setPlan(null); setError(true); setMessage(String(e).startsWith('Nothing is synced')?String(e):`Could not check what to sync: ${String(e)}`); }
    finally { setPlanLoading(null); }
  };
  const showPlan=async(id:string, systems:string[]|null=null, nextChoices:Record<string,Choice>=choices, nextCollection:string=planCollection)=>{
    setSyncMode(false); setPlanLoading(id); setConfirming(false); setCopyResult(null); setChoices(nextChoices); setPlanCollection(nextCollection);
    try { setPlan(await invoke<DeploymentPlan>('deployment_plan',{installationId:id, systems, choices:nextChoices, collectionId:nextCollection||null})); setPlanBuilt(new Date()); }
    catch(e){ setError(true); setMessage(`Could not build the deployment plan: ${String(e)}`); }
    finally { setPlanLoading(null); }
  };
  const replan=(next:Record<string,Choice>)=>{ if(!plan) return; if(syncMode) void showSync(plan.installation_id, next); else void showPlan(plan.installation_id, plan.systems.filter(y=>y.selected).map(y=>y.system_folder), next); };
  const applyChoices=async(next:Record<string,Choice>, kept:KeptOffChange[])=>{
    if(!plan) return;
    try {
      for(const k of kept) await invoke('set_kept_off',{installationId:plan.installation_id, destination:k.destination, title:k.title, kept:k.kept});
      await invoke('set_saved_choices',{installationId:plan.installation_id, choices:next});
    } catch(e){ setError(true); setMessage(`Could not save the choice: ${String(e)}`); return; }
    replan(next);
  };
  const chooseSystems=(change:(selected:Set<string>)=>void)=>{
    if(!plan) return;
    const selected=new Set(plan.systems.filter(x=>x.selected).map(x=>x.system_folder)); change(selected);
    void showPlan(plan.installation_id, plan.systems.map(x=>x.system_folder).filter(x=>selected.has(x)));
  };
  // "Stop syncing" a platform or collection: saved; nothing on the drive changes.
  const stopSyncing=async(kind:'systems'|'collections', value:string)=>{
    if(!plan) return; const id=plan.installation_id;
    const next={...syncScope, [kind]:syncScope[kind].filter(x=>x!==value)};
    try { await invoke('set_sync_scope',{installationId:id, scope:next}); await showSync(id, choices); }
    catch(e){ setError(true); setMessage(`Could not save what's synced: ${String(e)}`); }
  };
  const setArcade=async(id:string, system:string)=>{
    try { await invoke('set_arcade_system',{installationId:id, system: system||null}); await load();
      if(plan?.installation_id===id) await showPlan(id, plan.systems.filter(x=>x.selected).map(x=>x.system_folder));
      setError(false); setMessage(system?`Arcade games will go to roms\\${system}. Nothing has been copied.`:'Arcade games will not be placed.');
    } catch(e){ setError(true); setMessage(`Could not save the arcade choice: ${String(e)}`); }
  };
  const [backupFor,setBackupFor]=useState<string|null>(null);
  const [healthFor,setHealthFor]=useState<string|null>(null);
  const [backups,setBackups]=useState<{id:string; list:BackupList}|null>(null); const [undoing,setUndoing]=useState<number|null>(null); const [undoBusy,setUndoBusy]=useState(false);
  const showBackups=async(id:string)=>{ if(backups?.id===id){ setBackups(null); return; } try { setBackups({id, list: await invoke<BackupList>('list_backups',{installationId:id})}); setUndoing(null); } catch(e){ setError(true); setMessage(`Could not list the backups: ${String(e)}`); } };
  const undoReplace=async(id:string, row:BackupRow)=>{
    if(undoing!==row.item_id){ setUndoing(row.item_id); return; }
    setUndoBusy(true);
    try { const m=await invoke<string>('undo_replace',{itemId:row.item_id}); setError(false); setMessage(`${row.game_title}: ${m}`); }
    catch(e){ setError(true); setMessage(`Could not undo the replace: ${String(e)}`); }
    finally { setUndoBusy(false); setUndoing(null); try { setBackups({id, list: await invoke<BackupList>('list_backups',{installationId:id})}); } catch { /* list refresh is best effort */ } }
  };
  const openBackupFolder=async(id:string)=>{ try { await invoke('open_backup_folder',{installationId:id}); } catch(e){ setError(true); setMessage(String(e)); } };
  const forgetInstallation=async(id:string)=>{
    if(forgetting!==id){ setForgetting(id); return; }
    try { await invoke<boolean>('remove_installation',{id}); setForgetting(null); if(plan?.installation_id===id) setPlan(null); setMessage('Installation forgotten. No files were touched.'); await load(); }
    catch(e){ setError(true); setMessage(`Could not forget the installation: ${String(e)}`); }
  };
  const startCopy=async()=>{
    if(!plan) return; setConfirming(false); setCopyResult(null);
    const systems=plan.systems.filter(x=>x.selected).map(x=>x.system_folder);
    try { setError(false); setMessage('Copying. Each file is verified before it gets its real name.');
      const jobId=await invoke<string>('start_deployment',{installationId:plan.installation_id, systems, fingerprint:plan.fingerprint, destination:plan.installation_path, choices, collectionId:planCollection||null, addArt: GAMELIST_PLATFORMS.includes(plan.adapter_id)&&addArt, sync: syncMode, setupControllers: setupControllers&&(ctrl?.to_add.length??0)>0});
      if(!finishedJobs.current.has(jobId)) setCopy(c=>c??{job_id:jobId,folder:plan.installation_path});
    } catch(e){ setError(true); setMessage(`Could not start the copy: ${String(e)}`); }
  };
  // A health-check finding's button (FR-10).
  const healthAction=(id:string, a:string)=>{ if(a==='sync'){ setPage('Emulation Systems'); void openSync(id); } else if(a==='plan'){ setPage('Emulation Systems'); void openPlan(id); } else if(a==='backup'){ setPage('Emulation Systems'); setBackupFor(id); } else if(a==='bios') setPage('Customize'); else if(a==='open_backup_folder') void openBackupFolder(id); };
  // Setup wizard (ADR-101): the usual checked copy, started from the wizard's plan.
  const wizardCopy=async(p:DeploymentPlan, collectionId:string|null)=>{
    setCopyResult(null); setError(false); setMessage('Copying. Each file is verified before it gets its real name.');
    const jobId=await invoke<string>('start_deployment',{installationId:p.installation_id, systems:p.systems.filter(x=>x.selected).map(x=>x.system_folder), fingerprint:p.fingerprint, destination:p.installation_path, choices:await savedChoices(p.installation_id), collectionId, addArt:GAMELIST_PLATFORMS.includes(p.adapter_id), sync:false, setupControllers:false});
    if(!finishedJobs.current.has(jobId)) setCopy(c=>c??{job_id:jobId,folder:p.installation_path});
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
    {wizard?.task==='controllers' && <ControllerWizard systems={installs.length} onCopyCollection={id=>setWizard({task:'copy',collection:id})} onShowGunGames={()=>{ setWizard(null); showGunGames(); }} onClose={()=>setWizard(null)} notify={(text,isError=false)=>{ setError(isError); setMessage(text); }}/>}
    {wizard && wizard.task!=='controllers' && <SetupWizard key={wizard.task+(wizard.collection??'')} task={wizard.task} collection={wizard.collection} firstRun={!!wizard.first} games={stats?.files??0} scanning={scan!==null} busy={busy} installs={installs}
      copy={{running:copy!==null, progress:copy?.progress??null, result:copyResult?.summary??null, error:copyResult?.error??null}}
      onScan={()=>void importFolder()} onAdded={load} onSetArcade={setArcade} onStartCopy={wizardCopy}
      onOpenPlan={id=>{ setWizard(null); setPage('Emulation Systems'); void openPlan(id); }} onHealthAction={(id,a)=>{ setWizard(null); healthAction(id,a); }} onInstallHelp={()=>{ setWizard(null); setPage('Emulation Systems'); setInstallHelp(true); }}
      onClose={()=>setWizard(null)} notify={(text,isError=false)=>{ setError(isError); setMessage(text); }}/>}
    {page==='Dashboard' && <><section className="stats"><article><span>Cataloged games</span><strong>{(stats?.files??0).toLocaleString()}</strong><small>{stats?.platforms.length??0} platforms · local database</small></article><article><span>Detected volumes</span><strong>{drives.length}</strong><small>Read-only discovery</small></article><article><span>Available titles</span><strong>{(stats?.online??0).toLocaleString()}</strong><small>{stats&&(stats.offline||stats.missing)?`${stats.offline.toLocaleString()} offline · ${stats.missing.toLocaleString()} missing`:'Files currently accessible'}</small></article></section><div className="twocol"><section className="panel quick"><h2>Quick actions</h2><p>Step-by-step guides, all optional: use the one you need, or do it yourself any time. Nothing on your drives changes until you confirm.</p>
      <button className="wizard-main" disabled={!native} onClick={()=>setWizard({task:'everything'})}><Wand2 size={22}/><span><b>Set up everything</b><small>From your games to a ready system: games, system, what to copy, BIOS, copy, first backup.</small></span></button>
      <div className="task-grid">
        <button className="task" disabled={!native||busy} onClick={()=>setWizard({task:'games'})}><FolderOpen size={18}/><b>Add your games</b><small>{scan?'Scanning…':'Show RetroDrive where your games are'}</small></button>
        <button className="task" disabled={!native} onClick={()=>setWizard({task:'system'})}><Search size={18}/><b>Find or add a system</b><small>RetroBat, ES-DE, Batocera and more</small></button>
        <button className="task" disabled={!native||installs.length===0} onClick={()=>setWizard({task:'copy'})}><RefreshCw size={18}/><b>Copy games to a system</b><small>{installs.length===0?'Add a system first':'Choose what to copy, then copy'}</small></button>
        <button className="task" disabled={!native} onClick={()=>setWizard({task:'bios'})}><ShieldCheck size={18}/><b>Check BIOS files</b><small>What your systems need, and what's missing</small></button>
        <button className="task" disabled={!native||installs.length===0} onClick={()=>setWizard({task:'backup'})}><HardDriveDownload size={18}/><b>Back up a system</b><small>{installs.length===0?'Add a system first':'Saves, settings and game lists'}</small></button>
        <button className="task" disabled={!native} onClick={()=>setWizard({task:'controllers'})}><Joystick size={18}/><b>Controllers and light guns</b><small>Find and set up your controller or gun</small></button>
      </div>
      <button className="subtle" onClick={()=>setPage('Game Library')}><LibraryBig size={18}/> Browse library</button>{(stats?.duplicate_files??0)>0&&<p className="dup-hint"><Copy size={15}/> {stats!.duplicate_files.toLocaleString()} files look like duplicate copies. <button className="link" onClick={reviewDuplicates}>Review them</button> or <button className="link" onClick={()=>void previewCleanup()}>clean them up</button> (nothing is deleted).</p>}</section><section className="panel"><h2>Platform roadmap</h2>{([['batocera','Batocera','Plan & copy games (drive or network)'],['retrobat','RetroBat','Recognize, plan & copy games'],['retropie','RetroPie','Plan & copy games (network or USB)'],['esde','ES-DE','Plan & copy games'],['recalbox','Recalbox','Plan & copy games (network or drive)'],['rocknix','ROCKNIX','Plan & copy games (network or SD card)'],['lakka','Lakka','Plan & copy games, playlists (network)']] as const).map(([id,name,what])=>{ const n=installs.filter(i=>i.adapter_id===id).length; return <div key={id} className="platform-row"><span>{name}</span><em>{n?`${n} installation${n>1?'s':''} · plan & copy games`:what}</em></div>; })}<div className="platform-row"><span>More systems</span><em>Future milestone</em></div></section></div>
    <section className="panel activity"><h2>Recent activity</h2>{jobs.length===0?<p>No scans or copies yet.</p>:jobs.map(j=><div className="platform-row" key={j.id}><span><b className={'job '+j.state}>{jobStateLabel[j.state]??j.state}</b> <span className="job-folder" title={j.input.folder}>{j.kind==='deployment'?'Copy to ':''}{j.input.folder??j.kind}</span></span><em>{j.progress?.indexed!=null?`${j.progress.indexed} files · `:''}{j.progress?.files_copied!=null?`${j.progress.files_copied} files copied · `:''}{j.error_message?j.error_message+' · ':''}{new Date(j.started_at.replace(' ','T')+'Z').toLocaleString()}</em></div>)}</section></>}
    {page==='Game Library' && <section className="panel library-panel">
      <div className="section-heading"><div><h2>Master game catalog</h2><p>One searchable view across every imported folder and drive, including drives that aren't plugged in.</p></div><button className="primary" disabled={!native||busy||refBusy} onClick={importFolder}><FolderOpen size={17}/> Import folder</button></div>
      <div className="library-bar"><div className="tabs" role="tablist"><button role="tab" aria-selected={libraryMode==='systems'} className={libraryMode==='systems'?'on':''} onClick={()=>chooseMode('systems')}>Systems</button><button role="tab" aria-selected={libraryMode==='collections'} className={libraryMode==='collections'?'on':''} onClick={()=>chooseMode('collections')}>Collections</button><button role="tab" aria-selected={libraryMode==='games'} className={libraryMode==='games'?'on':''} onClick={()=>chooseMode('games')}>{filters.collection_id?`Games · ${collectionName(filters.collection_id)}`:filters.platform?`Games · ${filters.platform}`:'All games'}</button></div>
        {libraryMode==='games'&&<div className="view-toggle" role="group" aria-label="View"><button className={libraryView==='grid'?'on':''} aria-pressed={libraryView==='grid'} title="Grid" onClick={()=>chooseView('grid')}><LayoutGrid size={16}/></button><button className={libraryView==='list'?'on':''} aria-pressed={libraryView==='list'} title="List" onClick={()=>chooseView('list')}><List size={16}/></button></div>}</div>
      {libraryMode==='systems'&&systems&&systems.length>0&&(()=>{ const named=systems.filter(x=>!['Unclassified','BIOS'].includes(x.platform)).sort((a,b)=>a.platform.localeCompare(b.platform)); const first=new Map<string,string>(); named.forEach(x=>{ const l=initialOf(x.platform); if(!first.has(l)) first.set(l,x.platform); });
        return <nav className="az-bar" aria-label="Jump to systems by letter">{LETTERS.map(l=><button key={l} disabled={!first.has(l)} onClick={()=>document.getElementById(`system-${first.get(l)}`)?.scrollIntoView({behavior:'smooth',block:'start'})}>{l}</button>)}</nav>; })()}
      {addTarget&&<div className="modal-backdrop" onClick={()=>setAddTarget(null)}><div className="modal" role="dialog" aria-label="Add to collection" onClick={e=>e.stopPropagation()}>
        <h3>{addTarget.mode==='one'?`Add “${displayTitle(addTarget.game)}” to a collection`:`Add all ${library?.total_capped?'10,000+':(library?.total??0).toLocaleString()} shown games to a collection`}</h3>
        {collectionList.length>0?<div className="modal-list">{collectionList.map(c=><button key={c.id} className="ghost" onClick={()=>void addToCollection(c.id)}>{c.name}<small>{c.games.toLocaleString()} games</small></button>)}</div>:<p className="plan-notes">No collections yet. Name one below.</p>}
        <div className="new-collection"><input placeholder="New collection name" aria-label="New collection name" value={newCollection} onChange={e=>setNewCollection(e.target.value)} onKeyDown={async e=>{ if(e.key==='Enter'&&newCollection.trim()){ const name=newCollection.trim().replace(/\s+/g,' '); const id=await createCollection(newCollection); if(id) void addToCollection(id,name); } }}/><button className="primary" disabled={!newCollection.trim()} onClick={async()=>{ const name=newCollection.trim().replace(/\s+/g,' '); const id=await createCollection(newCollection); if(id) void addToCollection(id,name); }}><Plus size={16}/> Create and add</button></div>
        <div className="actions"><button className="ghost" onClick={()=>setAddTarget(null)}>Cancel</button></div></div></div>}
      {libraryMode==='collections' ? <div className="collections">
        <div className="new-collection"><input placeholder="New collection, e.g. Couch co-op" aria-label="New collection name" value={newCollection} onChange={e=>setNewCollection(e.target.value)} onKeyDown={e=>{ if(e.key==='Enter'&&newCollection.trim()) void createCollection(newCollection); }}/><button className="primary" disabled={!native||!newCollection.trim()} onClick={()=>void createCollection(newCollection)}><Plus size={16}/> New collection</button></div>
        {collectionList.length===0?<div className="empty">No collections yet. Create one here, then use “Add to collection” on any game, or “Add all shown” in the games view.</div>:
        <div className="system-grid">{collectionList.map((c,i)=><div key={c.id} className="system-card collection-card" style={{['--hue' as string]:platformHue(c.name)}}>
          {renaming?.id===c.id?<input className="rename" autoFocus aria-label="Collection name" value={renaming.name} onChange={e=>setRenaming({id:c.id,name:e.target.value})} onKeyDown={e=>{ if(e.key==='Enter') void renameCollection(); if(e.key==='Escape') setRenaming(null); }} onBlur={()=>void renameCollection()}/>:<button className="collection-open" onClick={()=>openCollection(c)}><span className="system-name">{c.kind==='favorites'?<Star size={15} fill="currentColor"/>:c.kind==='smart'?<Sparkles size={15}/>:null} {c.name}</span></button>}
          <span className="system-count">{c.games.toLocaleString()} {c.games===1?'game':'games'}</span><small>{fmtSize(c.bytes)}{c.kind==='smart'?` · fills itself: ${ruleSummary(c.rule)}`:c.kind==='favorites'?' · star games to add them':''}</small>
          <div className="collection-actions"><button onClick={()=>openCollection(c)}>Open</button>{c.kind!=='favorites'&&<><button title="Move up" aria-label={`Move ${c.name} up`} disabled={i<=1} onClick={()=>void invoke('move_collection',{id:c.id, up:true}).then(loadCollections)}>↑</button><button title="Move down" aria-label={`Move ${c.name} down`} disabled={i===collectionList.length-1} onClick={()=>void invoke('move_collection',{id:c.id, up:false}).then(loadCollections)}>↓</button><button onClick={()=>setRenaming({id:c.id,name:c.name})}>Rename</button><button onClick={()=>void deleteCollection(c)} onBlur={()=>setDeletingCollection(null)}>{deletingCollection===c.id?'Click again to delete':'Delete'}</button></>}</div></div>)}</div>}
      </div> : libraryMode==='systems' ? <div className="system-grid">{systems===null?<div className="empty">Loading systems…</div>:systems.length===0?<div className="empty"><Database size={26}/>No games yet. Import a ROM folder to begin.</div>:[...systems].sort((a,b)=>(['Unclassified','BIOS'].includes(a.platform)?1:0)-(['Unclassified','BIOS'].includes(b.platform)?1:0)||a.platform.localeCompare(b.platform)).map(sp=><button key={sp.platform} id={`system-${sp.platform}`} className="system-card" style={{['--hue' as string]:platformHue(sp.platform)}} onClick={()=>{ updateFilters({platform:sp.platform}); chooseMode('games'); }}><span className="system-name">{sp.platform}</span><span className="system-count">{sp.games.toLocaleString()} {sp.platform==='BIOS'?(sp.games===1?'file':'files'):(sp.games===1?'game':'games')}</span><small>{fmtSize(sp.bytes)}{sp.platform==='Unclassified'?' · platform not recognized yet':sp.platform==='BIOS'?' · BIOS and firmware, not games':''}</small></button>)}</div> : <>
      {filters.availability!=='left_out'&&(stats?.duplicate_files??0)>0&&!cleanup&&<p className="dup-hint"><Copy size={15}/> {stats!.duplicate_files.toLocaleString()} files look like duplicate copies. <button className="link" onClick={reviewDuplicates}>Show them</button> · <button className="link" disabled={cleanupBusy||busy} onClick={()=>void previewCleanup()}>{cleanupBusy?'Checking…':'Clean up duplicates…'}</button></p>}
      {cleanup&&<div className="confirm cleanup"><p><b>Keep one copy of each of {cleanup.groups.toLocaleString()} duplicate groups, and leave {cleanup.files_to_leave_out.toLocaleString()} extra copies ({fmtSize(cleanup.bytes_to_leave_out)}) out of the library?</b></p>
        <ul><li>Copies left out stay on your drives, untouched. RetroDrive just stops listing and copying them.</li><li>The copy kept is one that is present on a connected drive, with the shortest path.</li><li>You can bring any of them back (Status: Left out of library).</li></ul>
        {cleanup.examples.length>0&&<div className="table-wrap"><table><thead><tr><th>Game</th><th>Kept</th><th>Left out</th></tr></thead><tbody>{cleanup.examples.slice(0,8).map(x=><tr key={x.keep}><td><b>{x.title}</b></td><td className="mono" title={x.keep}>{shortPath(x.keep)}</td><td className="mono" title={x.leave_out.join('\n')}>{x.leave_out.map(shortPath).join(', ')}</td></tr>)}</tbody></table></div>}
        <div className="actions"><button className="primary" disabled={cleanupBusy||busy||cleanup.files_to_leave_out===0} onClick={()=>void applyCleanup()}><Copy size={16}/> Leave {cleanup.files_to_leave_out.toLocaleString()} duplicates out</button><button className="ghost" onClick={()=>setCleanup(null)}>Cancel</button></div></div>}
      <label className="search"><Search size={18}/><input placeholder="Search titles or systems" value={searchText} onChange={e=>setSearchText(e.target.value)} aria-label="Search titles or systems"/></label>
      <div className="filters">
        <select aria-label="Platform" value={filters.platform} onChange={e=>updateFilters({platform:e.target.value})}><option value="">All platforms</option>{stats?.platforms.map(p=><option key={p.platform} value={p.platform}>{p.platform} ({p.files.toLocaleString()})</option>)}</select>
        <select aria-label="Status" value={filters.availability} onChange={e=>updateFilters({availability:e.target.value as Filters['availability']})}><option value="">Any status</option><option value="online">Online</option><option value="offline">Offline</option><option value="missing">Missing</option><option value="left_out">Left out of library{stats?.left_out?` (${stats.left_out.toLocaleString()})`:''}</option></select>
        <select aria-label="Drive" value={filters.storage_id} onChange={e=>updateFilters({storage_id:e.target.value})}><option value="">All drives</option>{storage.map(d=><option key={d.id} value={d.id}>{d.volume_label??d.display_name}</option>)}</select>
        <select aria-label="Sort" value={filters.sort} onChange={e=>updateFilters({sort:e.target.value as Filters['sort']})}><option value="title">Sort: Title</option><option value="platform">Sort: Platform</option><option value="size">Sort: Largest first</option></select>
        <label className="check"><input type="checkbox" checked={filters.duplicates_only} onChange={e=>updateFilters({duplicates_only:e.target.checked})}/> Possible duplicates only</label>
        <label className="check" title="Games made for a light gun (from Batocera's game list)"><input type="checkbox" checked={filters.lightgun_only} onChange={e=>updateFilters({lightgun_only:e.target.checked})}/> Light gun games</label>
        <label className="check" title="Arcade games made for a portrait (vertical) screen (from Batocera's game list)"><input type="checkbox" checked={filters.vertical_only} onChange={e=>updateFilters({vertical_only:e.target.checked})}/> Vertical games</label>
        <select aria-label="Players" title="From the details in your library's game lists" value={filters.players_min} onChange={e=>updateFilters({players_min:e.target.value})}><option value="">Any players</option><option value="2">2+ players</option><option value="3">3+ players</option><option value="4">4+ players</option></select>
        {genres.length>0&&<select aria-label="Genre" value={filters.genre} onChange={e=>updateFilters({genre:e.target.value})}><option value="">Any genre</option>{genres.map(g=><option key={g} value={g}>{g}</option>)}</select>}
        <span className="year-range" title="Release year, from your game catalog">Years <input aria-label="From year" inputMode="numeric" placeholder="from" value={filters.year_from} onChange={e=>updateFilters({year_from:e.target.value.replace(/[^0-9]/g,'').slice(0,4)})}/>–<input aria-label="To year" inputMode="numeric" placeholder="to" value={filters.year_to} onChange={e=>updateFilters({year_to:e.target.value.replace(/[^0-9]/g,'').slice(0,4)})}/></span>
        {filtersActive&&<button className="link" onClick={clearFilters}>Clear filters</button>}{native&&library&&library.total>0&&filters.availability!=='left_out'&&<button className="link" onClick={()=>{ void loadCollections(); setAddTarget({mode:'all'}); }}>Add all shown to a collection…</button>}{native&&filtersActive&&!filters.collection_id&&filters.availability!=='left_out'&&smartName===null&&<button className="link" title="A collection that fills itself from these filters, now and as you add games" onClick={()=>setSmartName('')}><Sparkles size={13}/> Save as smart collection…</button>}
        {smartName!==null&&<span className="smart-save"><input autoFocus placeholder="Name, e.g. SNES RPGs" aria-label="Smart collection name" value={smartName} onChange={e=>setSmartName(e.target.value)} onKeyDown={e=>{ if(e.key==='Enter') void saveSmart(); if(e.key==='Escape') setSmartName(null); }}/><button className="primary" disabled={!smartName.trim()} onClick={()=>void saveSmart()}>Save</button><button className="ghost" onClick={()=>setSmartName(null)}>Cancel</button></span>}
      </div>
      <nav className="az-bar" aria-label="Show games by first letter"><button className={filters.letter===''?'on':''} onClick={()=>updateFilters({letter:''})}>All</button>{LETTERS.map(l=><button key={l} className={filters.letter===l?'on':''} aria-pressed={filters.letter===l} onClick={()=>updateFilters({letter:filters.letter===l?'':l})}>{l}</button>)}</nav>
      {libraryView==='grid' ? <div className={'game-grid'+(libraryLoading?' loading':'')}>{library?.rows.map(g=><article key={g.id} className={'game-card '+g.availability} title={`${displayTitle(g)}${g.reference?.publisher?`\n${g.reference.publisher}`:''}${g.description?`\n\n${g.description.length>400?g.description.slice(0,400)+'…':g.description}\n`:''}\n${g.path}\n${availabilityLabel[g.availability]}`}>
        <div className="cover" style={{['--hue' as string]:platformHue(g.platform)}}><span className="cover-initials">{initials(displayTitle(g))}</span>{artUrl(g)&&<img className="cover-img" src={artUrl(g)!} alt="" loading="lazy" onError={e=>{e.currentTarget.style.display='none';}}/>}<span className="cover-platform">{g.platform}</span>{native&&<button className={'fav-star'+(g.favorite?' on':'')} title={g.favorite?'In Favorites (click to take out)':'Add to Favorites'} aria-label={g.favorite?'Remove from Favorites':'Add to Favorites'} aria-pressed={g.favorite} onClick={e=>{ e.stopPropagation(); void toggleFavorite(g); }}><Star size={16} fill={g.favorite?'currentColor':'none'}/></button>}{(g.lightgun||g.vertical)&&<span className="cover-badges">{g.lightgun&&<span className="cover-gun" title="A light gun game"><Crosshair size={12}/> Light gun</span>}{g.vertical&&<span className="cover-gun" title="Made for a vertical (portrait) screen"><RectangleVertical size={12}/> Vertical</span>}</span>}{native&&<div className="art-actions"><button onClick={()=>void setArt(g)}>{g.art?'Change art':'Add art'}</button>{artSource(g)==='user'&&<button onClick={()=>void removeArt(g)}>Remove my art</button>}{filters.availability==='left_out'?<button onClick={()=>void setLeftOut(g,false)}>Bring back</button>:<><button onClick={()=>{ void loadCollections(); setAddTarget({mode:'one',game:g}); }}>Add to collection</button>{filters.collection_id&&<button onClick={()=>void removeFromCollection(g)}>Remove from collection</button>}<button onClick={()=>void setLeftOut(g,true)}>Leave out</button></>}</div>}</div>
        <div className="card-body"><b>{displayTitle(g)}</b>{g.reference&&refLine(g.reference)&&<small className="ref-line">{refLine(g.reference)}</small>}{detailLine(g)&&<small className="ref-line">{detailLine(g)}</small>}<small>{fmtSize(g.file_size)} · <span className={'state '+g.availability}>{g.availability==='online'?'Online':g.availability==='offline'?'Offline':'Missing'}</span></small>
          <div className="card-badges">{g.disc_count>1&&<span className="disc-badge">{g.disc_count} discs</span>}{g.incomplete&&<span className="incomplete-badge">Incomplete</span>}{g.duplicate_count>1&&<span className="dup-badge" title="Possible duplicate copies (same platform, name and size)">×{g.duplicate_count}</span>}</div></div></article>)}
        {library&&library.rows.length===0&&<div className="empty"><Database size={26}/>{(stats?.files??0)===0?'No games yet. Import a ROM folder to begin.':<>No games match these filters. <button className="link" onClick={clearFilters}>Clear filters</button></>}</div>}</div>
      : <div className={'table-wrap'+(libraryLoading?' loading':'')}><table className="game-list"><thead><tr><th aria-label="Cover"></th><th>Game</th><th>Platform</th><th>Year</th><th>Genre</th><th>Drive</th><th>Size</th><th>Availability</th></tr></thead><tbody>{library?.rows.map(g=><tr key={g.id}><td className="list-thumb">{artUrl(g)?<img src={artUrl(g)!} alt="" loading="lazy" onError={e=>{e.currentTarget.style.display='none';}}/>:<span className="thumb-blank" style={{['--hue' as string]:platformHue(g.platform)}}>{initials(displayTitle(g))}</span>}</td><td>{native&&<button className={'list-star'+(g.favorite?' on':'')} title={g.favorite?'In Favorites (click to take out)':'Add to Favorites'} aria-label={g.favorite?'Remove from Favorites':'Add to Favorites'} aria-pressed={g.favorite} onClick={()=>void toggleFavorite(g)}><Star size={13} fill={g.favorite?'currentColor':'none'}/></button>}<b title={g.description??undefined}>{displayTitle(g)}{g.lightgun&&<span className="list-mark" title="A light gun game"><Crosshair size={12}/></span>}{g.vertical&&<span className="list-mark" title="Made for a vertical screen"><RectangleVertical size={12}/></span>}{g.disc_count>1&&<span className="disc-badge" title="Multi-disc game: discs listed by its .m3u playlist">{g.disc_count} discs</span>}{g.incomplete&&<span className="incomplete-badge" title="A disc or track file this game needs is missing. See the scan warnings.">Incomplete</span>}{g.duplicate_count>1&&<span className="dup-badge" title={`Possible duplicate: ${g.duplicate_count} files share this platform, file name and size. Review before removing anything; RetroDrive never deletes files.`}>×{g.duplicate_count}</span>}</b>{detailLine(g)&&<small className="list-detail">{detailLine(g)}</small>}<small className="list-path" title={g.path}>{shortPath(g.path)}</small></td><td>{g.platform}</td><td>{g.reference?.year??''}</td><td className="genre-cell" title={g.reference?.genre??''}>{g.reference?.genre??''}</td><td className="drive-cell" title={g.storage_name}>{g.storage_name}</td><td>{fmtSize(g.file_size)}</td><td><span className={'state '+g.availability}>{availabilityLabel[g.availability]}</span>{native&&<button className="link row-action" onClick={()=>void setLeftOut(g,filters.availability!=='left_out')}>{filters.availability==='left_out'?'Bring back':'Leave out'}</button>}</td></tr>)}</tbody></table>
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
    {installHelp&&<InstallHelper notify={(text, isError=false)=>{ setError(isError); setMessage(text); }} onClose={()=>setInstallHelp(false)} onFind={()=>{ setInstallHelp(false); setWizard({task:'system'}); }}/>}
    <section className="panel">
      <div className="section-heading"><div><h2>Emulation systems</h2><p>Platforms RetroDrive works with. RetroDrive recognizes an installation, plans where your games would go, checks drive space and existing files, and copies only after you confirm. It never overwrites or deletes files on the drive.</p></div><div className="heading-actions">{wizardButton}<button className="ghost" disabled={!native} onClick={()=>setInstallHelp(v=>!v)}><Download size={17}/> Install a system</button><button className="primary" disabled={!native} onClick={()=>void addInstallation()}><Plus size={17}/> Add existing installation</button></div></div>
      <div className="adapter-grid">
        {adapters.map(a=><article className="adapter" key={a.id}><b>{a.display_name}</b><small>{kindLabel[a.kind]??a.kind}</small>
          <ul className="caps"><li className={a.capabilities.detect?'yes':'no'}>Recognizes installations</li><li className={a.capabilities.validate_layout?'yes':'no'}>Checks folder layout</li><li className={a.capabilities.plan_placement?'yes':'no'}>Plans deployment (space and conflicts)</li><li className={a.capabilities.copy_games?'yes':'no'}>{a.capabilities.copy_games?'Copies games (verified; never overwrites)':'Copies games (not yet: plans are previews until a safety review)'}</li>{a.id==='retrobat'||a.id==='esde'?<li className="yes">Helps you install it (you download it from its website; RetroDrive finds it)</li>:<li className="yes">Helps you install it (you download it; Raspberry Pi Imager or balenaEtcher writes it)</li>}</ul></article>)}
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
        {inspection.detections.filter(d=>d.confidence==='possible').map(d=><p key={d.adapter_id}>It looks partly like {d.display_name} ({d.evidence.join(', ')}). {d.hint}</p>)}
        <div className="actions"><button className="ghost" onClick={()=>setInspection(null)}>Close</button></div>
      </>}
    </section>}
    <section className="panel">
      <h2>Your installations</h2>
      {installs.length===0 ? <div className="empty">No installations yet. Click <b>&nbsp;Add existing installation&nbsp;</b> and pick your RetroBat folder, Batocera game folder, RetroPie roms folder, ES-DE ROMs folder or Recalbox share.</div> : installs.map(ins=><article className="installation" key={ins.id}>
        <div className="installation-head"><div><b>{ins.adapter_name}</b><small className="mono">{ins.path}</small><small className={'presence '+ins.presence}>{installPresence[ins.presence]} · {ins.version?`Version ${ins.version}`:'Version unknown'}</small></div>
          <div className="actions">{(GAMELIST_PLATFORMS.includes(ins.adapter_id)||ins.adapter_id==='esde'||ins.adapter_id==='recalbox'||ins.adapter_id==='rocknix')&&<button className="primary" disabled={ins.presence!=='connected'||planLoading!==null||busy} onClick={()=>void openSync(ins.id)} title="Bring the drive up to date with your library: new and missing games, checked and verified. Never overwrites or deletes."><RefreshCw size={16}/>{planLoading===ins.id&&syncMode?'Checking…':'Sync'}</button>}<button className={GAMELIST_PLATFORMS.includes(ins.adapter_id)||ins.adapter_id==='esde'||ins.adapter_id==='recalbox'?'ghost':'primary'} disabled={ins.presence!=='connected'||planLoading!==null} onClick={()=>void openPlan(ins.id)}><Eye size={16}/>{planLoading===ins.id&&!syncMode?'Planning...':'Plan deployment'}</button><button className="ghost" onClick={()=>void showBackups(ins.id)}><History size={16}/>{backups?.id===ins.id?'Hide replaced files':'Replaced files'}</button>{BACKUP_ADAPTERS.includes(ins.adapter_id)&&<button className="ghost" onClick={()=>setBackupFor(backupFor===ins.id?null:ins.id)} title="Back up this system's saves, settings and game lists to your PC, or restore them"><HardDriveDownload size={16}/>{backupFor===ins.id?'Hide backup':'Backup'}</button>}<button className="ghost" disabled={ins.presence!=='connected'} onClick={()=>setHealthFor(healthFor===ins.id?null:ins.id)} title="Check space, games, folders, BIOS, backups and leftovers (changes nothing)"><Activity size={16}/>{healthFor===ins.id?'Hide health':'Check health'}</button><button className="ghost" onClick={()=>void forgetInstallation(ins.id)} onBlur={()=>setForgetting(null)}>{forgetting===ins.id?'Click again to forget':'Forget'}</button></div></div>
        <div className="arcade-choice"><label>Arcade games go to <select value={ins.arcade_system??''} disabled={busy} onChange={e=>void setArcade(ins.id, e.target.value)}><option value="">Not chosen (arcade games aren't placed)</option><option value="mame">MAME (roms\mame)</option><option value="fbneo">FinalBurn Neo (roms\fbneo)</option></select></label>
          <small>Pick the emulator your arcade romset was made for: romset versions must match the emulator.{ins.adapter_id==='retrobat'?' RetroBat recommends non-merged sets and doesn\'t support merged ones.':''}{ins.adapter_id==='batocera'?' On Batocera, CPS1/2/3 games go here too.':''} CHD disks aren't placed yet.</small></div>
        {healthFor===ins.id&&<HealthPanel installationId={ins.id} disabled={busy||ins.presence!=='connected'} notify={(text,isError)=>{ setError(!!isError); setMessage(text); }} onAction={a=>healthAction(ins.id,a)}/>}
        {backupFor===ins.id&&<SystemBackup installationId={ins.id} sysName={ins.adapter_name} disabled={busy||ins.presence!=='connected'} notify={(text,isError)=>{ setError(!!isError); setMessage(text); }}/>}
        {backups?.id===ins.id&&<div className="backups"><h3>Replaced files</h3>
          <p className="plan-notes">When you chose <b>Replace</b>, the drive's original was moved to <span className="mono">.retrodrive\backup</span>. <b>Undo replace</b> puts the original back and moves RetroDrive's copy into the backup folder instead. Nothing is deleted.{backups.list.folder_bytes>0?` The backup folder holds ${fmtSize(backups.list.folder_bytes)}.`:''}</p>
          {backups.list.rows.length===0?<p className="plan-notes">No files have been replaced on this installation.</p>:<div className="table-wrap"><table><thead><tr><th>Game</th><th>File</th><th>When</th><th></th></tr></thead><tbody>{backups.list.rows.map(r=><tr key={r.item_id}><td><b>{r.game_title}</b>{r.backup&&<small className="mono" title={r.backup}>{r.state==='undone'?'RetroDrive’s copy: ':'Original: '}{shortPath(r.backup)}{r.backup_bytes!==null?` · ${fmtSize(r.backup_bytes)}`:''}</small>}</td><td className="mono" title={r.path}>{shortPath(r.path)}</td><td>{localTime(r.when)}</td>
            <td>{r.state==='undone'?<span className="tag identical">Undone</span>:r.blocked?<small className="warn-text" title={r.blocked}>{r.blocked}</small>:<button className={undoing===r.item_id?'primary':'ghost'} disabled={undoBusy||busy} onClick={()=>void undoReplace(ins.id, r)} onBlur={()=>{ if(!undoBusy) setUndoing(null); }}><Undo2 size={15}/>{undoBusy&&undoing===r.item_id?'Undoing…':undoing===r.item_id?'Click again to undo':'Undo replace'}</button>}</td></tr>)}</tbody></table></div>}
          <div className="actions"><button className="ghost" disabled={!backups.list.connected} onClick={()=>void openBackupFolder(ins.id)}><FolderOpen size={16}/> Open backup folder</button></div>
          <small className="plan-notes">To free up space, delete files from the backup folder yourself; RetroDrive never deletes them.</small>
        </div>}
        {ins.checks.length>0&&<Checks checks={ins.checks}/>}
      </article>)}
    </section>
    {plan && (()=>{ const c=plan.capacity; const g=plan.games; const verdict=c.fits===true?'ok':c.fits===false?'problem':'warn';
      // Two versions waiting for a choice are counted under "Review choices", not as blocked.
      const blocked=g.blocked-plan.attention.filter(x=>x.choice?.situation==='clash'&&x.action==='blocked').length;
      return <section className="panel" ref={planRef}>
      <h2>{syncMode?`Sync: ${plan.adapter_name}`:`Deployment plan: ${plan.adapter_name}`}</h2>
      {syncMode?<p>Keeps what you copied to <span className="mono">{plan.installation_path}</span> up to date: checks those games, copies games newly added to those platforms in your library, and asks about anything removed or changed on the drive.{GAMELIST_PLATFORMS.includes(plan.adapter_id)?' Then adds box art and details.':''} It never overwrites or deletes anything. <b>Nothing has been copied yet.</b></p>:
      <p>What copying your games to <span className="mono">{plan.installation_path}</span> would do. <b>Nothing has been copied yet.</b> When you copy, RetroDrive checks everything again first, verifies every file, and never overwrites or deletes anything on the drive.</p>}
      <div className={'verdict '+verdict}>{verdict==='ok'?<CheckCircle2 size={18}/>:verdict==='problem'?<XCircle size={18}/>:<AlertTriangle size={18}/>}
        {c.fits===true?(plan.files_to_copy===0?' Nothing new to copy.':' Fits on this drive.'):c.fits===false?" Doesn't fit: free up space or choose fewer platforms.":" Free space couldn't be measured on this drive."}</div>
      {c.measured&&<div className="capacity"><div className="capacity-bar"><span className="need" style={{width:`${Math.min(100,c.required_bytes/Math.max(1,c.free_bytes)*100)}%`}}/><span className="reserve" style={{width:`${Math.min(100,c.reserve_bytes/Math.max(1,c.free_bytes)*100)}%`}}/></div>
        <div className="preview-summary"><span>Needs <b>{fmtSize(c.required_bytes)}</b></span><span>Free <b>{fmtSize(c.free_bytes)}</b> of {fmtSize(c.total_bytes)}</span><span>Kept free <b>{fmtSize(c.reserve_bytes)}</b> (safety reserve)</span>{c.filesystem&&<span>{c.filesystem}{c.max_file_bytes?` · max ${fmtSize(c.max_file_bytes)} per file`:''}</span>}</div></div>}
      <div className="plan-counts">
        <span className="pc copy"><b>{g.copy.toLocaleString()}</b> to copy<small>{plan.files_to_copy.toLocaleString()} files · {fmtSize(plan.bytes_to_copy)}</small></span>
        <span className="pc"><b>{g.already_there.toLocaleString()}</b> already there</span>
        {g.in_place>0&&<span className="pc"><b>{g.in_place.toLocaleString()}</b> already in place</span>}
        {g.duplicate>0&&<span className="pc"><b>{g.duplicate.toLocaleString()}</b> not needed<small>duplicates or versions you didn’t pick</small></span>}
        {g.same_game>0&&<span className="pc"><b>{g.same_game.toLocaleString()}</b> already there under another name<small>not copied</small></span>}
        {g.replace>0&&<span className="pc conflict"><b>{g.replace.toLocaleString()}</b> to replace<small>originals backed up first</small></span>}{g.conflict>0&&<span className="pc"><b>{g.conflict.toLocaleString()}</b> {syncMode?'changed on the drive':'different file on the drive'}<small>left as it is</small></span>}
        {blocked>0&&<span className="pc blocked"><b>{blocked.toLocaleString()}</b> can’t be copied</span>}
        {g.removed>0&&<span className="pc conflict"><b>{g.removed.toLocaleString()}</b> removed from the drive<small>not copied back</small></span>}
        {g.kept_off>0&&<span className="pc"><b>{g.kept_off.toLocaleString()}</b> kept off the drive</span>}
        {plan.unavailable_games>0&&<span className="pc"><b>{plan.unavailable_games.toLocaleString()}</b> left out<small>drive not connected or files missing</small></span>}
      </div>
      {syncMode&&<p className="plan-notes">Keeping up to date: <b>{syncScope.systems.map(f=>plan.systems.find(x=>x.system_folder===f)?.platforms.join(', ')??f).join(' · ')||'no platforms'}</b>{syncScope.collections.length>0&&<> and {syncScope.collections.map(c=>`“${collectionName(c)}”`).join(', ')}</>}. To add more, use <b>Plan deployment</b>. <button className="link" onClick={()=>setSyncPlatforms(v=>!v)}>{syncPlatforms?'Done':'Stop syncing something'}</button></p>}
      {syncMode&&syncPlatforms&&<div className="sync-scope">{syncScope.systems.map(f=><label key={f} className="check"><input type="checkbox" checked disabled={planLoading!==null||busy} onChange={()=>void stopSyncing('systems',f)}/> {plan.systems.find(x=>x.system_folder===f)?.platforms.join(', ')??f} <small className="mono">roms\{f}</small></label>)}{syncScope.collections.map(c=><label key={c} className="check"><input type="checkbox" checked disabled={planLoading!==null||busy} onChange={()=>void stopSyncing('collections',c)}/> Collection “{collectionName(c)}”</label>)}<small className="plan-notes">Untick to stop syncing it. Games already on the drive stay there.</small></div>}
      <DecisionsLine plan={plan} open={decisionsOpen} onToggle={()=>setDecisionsOpen(v=>!v)}/>
      {!syncMode&&!planCollection&&plan.systems.length>0&&plan.systems.every(x=>!x.selected)&&<p className="plan-notes"><b>Tick the platforms you want on this drive</b> in the list below (or pick a collection). Nothing is copied until you confirm, and Sync keeps what you copy up to date.</p>}
      <div className="copy-actions">{!plan.copy_supported ? <p className="preview-note"><Eye size={16}/> This is a preview. RetroDrive doesn't copy to {plan.adapter_name} yet; copying there needs its own safety review first.</p> : copy ? <div className="inline-progress">{copyProgressView}</div> : confirming ? <div className="confirm"><p><b>{g.copy+g.replace>0?`Copy ${(g.copy+g.replace).toLocaleString()} ${g.copy+g.replace===1?'game':'games'} (${fmtSize(plan.bytes_to_copy)})`:`Check ${g.already_there.toLocaleString()} ${g.already_there===1?'game':'games'} already`}</b> {g.copy+g.replace>0?'to':'on'} <span className="mono">{plan.installation_path}</span>?</p><ul><li>{g.replace>0?'Nothing on the drive is deleted, and files you didn’t choose to replace are left exactly as they are.':'Files already on the drive are never overwritten or deleted.'}</li><li>Every copy is read back and checked before it gets its real name.</li>{plan.adapter_id==='lakka'&&<li>The copied games are added to Lakka's playlists (with your PNG box art), so they show in its menu; the first time a game starts, Lakka asks which core to use.</li>}{GAMELIST_PLATFORMS.includes(plan.adapter_id)&&<li><label className="check"><input type="checkbox" checked={addArt} onChange={e=>setAddArt(e.target.checked)}/> Also add your box art, logos, catalog details and ★ favorites to {plan.adapter_name}'s game lists (fills in only what's missing; {plan.adapter_id==='retrobat'?'needs RetroBat closed':plan.adapter_id==='esde'?"in ES-DE's own lists and media folders; needs ES-DE closed":plan.adapter_id==='retropie'?"only lists already in RetroPie's rom folders; restart RetroPie to see them":plan.adapter_id==='recalbox'?"a running Recalbox shows them after updating its game lists":plan.adapter_id==='rocknix'?'restart ROCKNIX to see them':'a running Batocera shows them after Update Gamelists'}; undo under Customize)</label></li>}{ctrl&&ctrl.to_add.length>0&&<li><label className="check"><input type="checkbox" checked={setupControllers} onChange={e=>setSetupControllers(e.target.checked)}/> Also set up your controller{ctrl.to_add.length===1?'':'s'} ({ctrl.to_add.join(', ')}) on {plan.adapter_name} (adds setup files only; undo under Customize)</label></li>}{ctrl&&ctrl.confirmed===0&&<li className="plan-notes">No controller is set up in RetroDrive yet. <button className="link" onClick={()=>setPage('Controllers')}>Set one up</button>, or set it up later in {plan.adapter_name}'s own controller menu.</li>}{ctrl&&ctrl.manual.length>0&&ctrl.to_add.length===0&&<li className="plan-notes">{ctrl.manual.join(', ')}: RetroDrive can't set {ctrl.manual.length===1?'it':'them'} up on {plan.adapter_name}. {ctrl.manual_message}</li>}{g.already_there>0&&<li>{g.already_there.toLocaleString()} {g.already_there===1?'game':'games'} already there will be checked by content.</li>}{g.replace>0&&<li><b>{g.replace.toLocaleString()} existing {g.replace===1?'file':'files'} will be replaced.</b> Each original is first moved to <span className="mono">.retrodrive\backup</span> on the drive and put back if anything goes wrong.</li>}{plan.installation_path.startsWith('\\')&&<li><b>This copies over the network.</b> Keep {plan.adapter_name} switched on and connected until it finishes; if it goes away, the copy stops safely and you can run it again.</li>}<li>You can cancel at any time; verified games are kept.</li><li>From now on, <b>Sync</b> keeps {planCollection?`the “${collectionName(planCollection)}” collection`:'these platforms'} up to date on this drive.</li></ul><div className="actions"><button className="primary" onClick={()=>void startCopy()}><Copy size={16}/> {g.copy+g.replace>0?'Yes, copy now':'Yes, check them'}</button><button className="ghost" onClick={()=>setConfirming(false)}>Not now</button></div></div>
        : (!syncMode&&!planCollection&&plan.systems.every(x=>!x.selected)) ? null : <button className="primary" disabled={!native||busy||planLoading!==null||c.fits!==true||g.copy+g.replace+g.already_there===0} onClick={()=>syncMode?void startCopy():setConfirming(true)}>{syncMode?<><RefreshCw size={16}/>{g.copy+g.replace>0?`Start sync: copy ${(g.copy+g.replace).toLocaleString()} ${g.copy+g.replace===1?'game':'games'} (${fmtSize(plan.bytes_to_copy)})`:'Start sync: check the games already there'}</>:<><Copy size={16}/>{g.copy+g.replace>0?`Copy ${(g.copy+g.replace).toLocaleString()} ${g.copy+g.replace===1?'game':'games'}`:'Check the games already there'}</>}</button>}
        {plan.copy_supported&&!copy&&c.fits!==true&&<small className="warn-text">Copying needs a plan that fits on the drive.</small>}</div>
      {!syncMode&&(plan.folders_to_create>0||plan.long_paths>0)&&<p className="plan-notes">{plan.folders_to_create>0&&<>{plan.folders_to_create.toLocaleString()} new {plan.folders_to_create===1?'folder':'folders'} would be created. </>}{plan.long_paths>0&&<span className="warn-text">{plan.long_paths.toLocaleString()} {plan.long_paths===1?'path is':'paths are'} longer than 260 characters, which some emulators can't open.</span>}</p>}
      {!syncMode&&<div className="what-to-copy"><label>What to copy <select value={planCollection} disabled={planLoading!==null||busy} onChange={e=>void showPlan(plan.installation_id, null, choices, e.target.value)}><option value="">Platforms (choose below)</option>{collectionList.map(c=><option key={c.id} value={c.id}>Collection: {c.name} ({c.games.toLocaleString()})</option>)}</select></label></div>}
      {!syncMode&&<><h3>{planCollection?`Platforms in “${collectionName(planCollection)}”`:'Platforms to include'}</h3>
      <div className="table-wrap"><table><thead><tr><th className="pick"><input type="checkbox" aria-label="All platforms" checked={plan.systems.length>0&&plan.systems.every(x=>x.selected)} disabled={planLoading!==null} onChange={e=>{const all=e.target.checked; chooseSystems(sel=>{sel.clear(); if(all) plan.systems.forEach(x=>sel.add(x.system_folder));});}}/></th><th>{plan.adapter_name} folder</th><th>Platforms</th><th>Games</th><th>Size</th></tr></thead><tbody>{plan.systems.map(sp=><tr key={sp.system_folder} className={sp.selected?'':'unpicked'}><td className="pick"><input type="checkbox" aria-label={sp.system_folder} checked={sp.selected} disabled={planLoading!==null} onChange={()=>chooseSystems(sel=>{ if(sel.has(sp.system_folder)) sel.delete(sp.system_folder); else sel.add(sp.system_folder); })}/></td><td><code>{sp.system_folder==='bios'?'bios':plan.systems_prefix+sp.system_folder}</code></td><td>{sp.platforms.join(', ')}</td><td>{sp.games.toLocaleString()}</td><td>{fmtSize(sp.bytes)}</td></tr>)}</tbody></table>{plan.systems.length===0&&<div className="empty">No games can be placed yet.</div>}</div></>}
      {decisionsOpen&&<DecisionsPanel plan={plan} choices={choices} disabled={planLoading!==null||busy||!native} onApply={(n,k)=>void applyChoices(n,k)} onClose={()=>setDecisionsOpen(false)}/>}
      {(plan.format_warnings?.length??0)>0&&<div className="warn-box"><b>{plan.adapter_name} won't show some of these games</b> (it doesn't list their file type for that system, so its menu hides them):<ul>{plan.format_warnings!.map(w=><li key={w.system_folder+w.extension}><span className="mono">{w.system_folder}</span>: {w.games.toLocaleString()} {w.extension} game{w.games===1?'':'s'} (it shows <span className="mono">{w.accepted}</span>)</li>)}</ul><small className="plan-notes">They are still copied. Use a version in one of the listed types, or another emulator for that system.</small></div>}
      {!syncMode&&plan.unsupported.length>0&&<><h3>Not placed automatically</h3><ul className="unsupported">{plan.unsupported.map(u=><li key={u.platform+u.reason}><b>{u.platform}</b> ({u.games.toLocaleString()} {u.games===1?'game':'games'}): {u.reason}</li>)}</ul></>}
      {!syncMode&&plan.sample.length>0&&<><h3>Examples</h3><div className="table-wrap"><table><thead><tr><th>Game</th><th>Would be copied to</th><th>Files</th><th>Size</th></tr></thead><tbody>{plan.sample.map(x=><tr key={x.source}><td><b>{x.title}</b><small title={x.source}>{shortPath(x.source)}</small></td><td className="mono" title={x.destination}>{shortPath(x.destination)}</td><td>{x.files}</td><td>{fmtSize(x.bytes_to_copy)}</td></tr>)}</tbody></table></div></>}
      <small className="plan-id">Plan {plan.fingerprint}{planBuilt?` · built ${planBuilt.toLocaleTimeString()}`:''}. Build it again if the drive changes.</small>
    </section>; })()}
    {copyResult && <section className="panel" ref={resultRef}>
      <h2>Copy result</h2>
      {copyResult.summary ? (()=>{ const r=copyResult.summary; return <>
        <div className={'verdict '+(r.status==='completed'?'ok':r.status==='completed_with_issues'?'warn':'problem')}>{r.status==='completed'?<CheckCircle2 size={18}/>:<AlertTriangle size={18}/>} {copyHeadline[r.status]}</div>
        <div className="plan-counts"><span className="pc copy"><b>{r.games_copied.toLocaleString()}</b> games copied<small>{(r.files_copied+r.files_replaced).toLocaleString()} files · {fmtSize(r.bytes_copied)}</small></span>{r.files_replaced>0&&<span className="pc conflict"><b>{r.files_replaced.toLocaleString()}</b> replaced<small>originals in .retrodrive\backup</small></span>}<span className="pc"><b>{r.games_already_there.toLocaleString()}</b> already there<small>checked (unchanged files aren’t read again)</small></span>{r.games_incomplete>0&&<span className="pc conflict"><b>{r.games_incomplete}</b> incomplete<small>run the copy again to finish</small></span>}{r.games_conflict>0&&<span className="pc conflict"><b>{r.games_conflict}</b> conflicts<small>existing files kept</small></span>}{r.files_not_attempted>0&&<span className="pc"><b>{r.files_not_attempted}</b> files not reached</span>}</div>
        {r.issues.length>0&&<div className="table-wrap"><table><thead><tr><th>Game</th><th>What happened</th></tr></thead><tbody>{r.issues.map((x,n)=><tr key={n}><td><b>{x.game}</b><small title={x.path}>{shortPath(x.path)}</small></td><td>{x.message}</td></tr>)}</tbody></table></div>}
        {r.manifest_path&&<p className="plan-notes">A record of this copy was saved to <span className="mono">{shortPath(r.manifest_path)}</span>.</p>}{r.manifest_error&&<p className="warn-text">{r.manifest_error}</p>}{copyResult.art&&<p className="plan-notes"><b>Game art:</b> {copyResult.art}</p>}{copyResult.controllers&&<p className="plan-notes"><b>Controllers:</b> {copyResult.controllers}</p>}
      </>; })() : <div className="verdict problem"><XCircle size={18}/> {copyResult.error}</div>}
      <div className="actions"><button className="ghost" onClick={()=>setCopyResult(null)}>Close</button></div>
    </section>}
    </>}
    {page==='Controllers' && <Controllers onShowGunGames={showGunGames} onWizard={()=>setWizard({task:'controllers'})} notify={(text, isError=false)=>{ setError(isError); setMessage(text); }}/>}
    {page==='Customize' && <Customize installs={installs} native={native} busy={busy} notify={(text, isError=false)=>{ setError(isError); setMessage(text); }}/>}
    {page==='Theme Studio' && <ThemeStudio native={native} leaveGuard={studioLeave}/>}
    {page==='Settings' && native && <CatalogTools busy={busy} onChanged={()=>void load()} notify={(text,isError)=>{ setError(!!isError); setMessage(text); }}/>}
    {page==='Settings' && native && <BackupSettings notify={(text,isError)=>{ setError(!!isError); setMessage(text); }}/>}
    {page==='Settings' && <section className="panel"><h2>Reference catalog</h2>
      <p>Import your own game catalog (.xlsx or .csv) to show proper titles, years, genres and publishers. RetroDrive matches games by their exact No-Intro or MAME name first, then by title on the same system. Only names and details are imported; <b>art columns are never read</b>. Your files are never changed, and removing the catalog leaves your library as it was.</p>
      {refStatus ? <div className="ref-status"><div><b>{refStatus.name}</b><small>{refStatus.file_name} · {refStatus.entries.toLocaleString()} entries{refStatus.genres_added?` · ${refStatus.genres_added.toLocaleString()} genres added`:''} · imported {sqlTime(refStatus.imported_at)}</small></div>
        <div className="plan-counts"><span className="pc copy"><b>{(refStatus.matched.by_name+refStatus.matched.by_variant+refStatus.matched.by_title).toLocaleString()}</b> games matched</span><span className="pc"><b>{refStatus.matched.by_name.toLocaleString()}</b> by exact name</span><span className="pc"><b>{refStatus.matched.by_variant.toLocaleString()}</b> by MAME clone set</span><span className="pc"><b>{refStatus.matched.by_title.toLocaleString()}</b> by title</span></div>
        <div className="table-wrap"><table><thead><tr><th>System</th><th>Games</th><th>Matched</th></tr></thead><tbody>{refStatus.platforms.map(p=><tr key={p.platform}><td>{p.platform}</td><td>{p.games.toLocaleString()}</td><td>{p.matched.toLocaleString()} ({p.games?Math.round(100*p.matched/p.games):0}%)</td></tr>)}</tbody></table></div></div>
      : <div className="empty">No reference catalog imported.</div>}
      {busy&&<p className="plan-notes warn-text">A scan or copy is running. You can import or remove the catalog when it finishes.</p>}
      {refResult&&refResult.ignored_columns.length>0&&<p className="plan-notes">Not imported (by design): {refResult.ignored_columns.join(', ')}.{refResult.skipped_rows?` ${refResult.skipped_rows.toLocaleString()} rows had no title or system and were skipped.`:''}</p>}
      <div className="actions"><button className="primary" disabled={!native||refBusy||busy} title={busy?'Available when the scan or copy finishes':undefined} onClick={()=>void importReference()}><FolderOpen size={17}/>{refBusy?'Importing…':refStatus?'Import a catalog or genre file':'Import catalog'}</button>{refStatus&&<button className="ghost" disabled={refBusy||busy} onClick={()=>void removeReference()} onBlur={()=>setRefRemoving(false)}>{refRemoving?'Click again to remove':'Remove catalog'}</button>}</div>
      <h2 className="legal-heading">Application settings</h2><p>RetroDrive currently stores its SQLite catalog in the operating system's application data folder. Imports index files in place and never copy, move, or delete them.</p><p>Future settings: metadata providers, library scan schedules, and platform defaults.</p><h2 className="legal-heading">About and legal</h2><p>RetroDrive doesn't include or download games, BIOS or firmware. It only organizes and copies files you choose, so use only files you have the right to use. RetroDrive is for your own personal use; don't use it to prepare drives or machines with games or third-party software for others or for sale.</p><p className="legal-notice">RetroDrive is an independent retro gaming management application. All third-party product names and trademarks belong to their respective owners. Compatibility references do not imply sponsorship, affiliation, or endorsement.</p><small className="legal-draft">Draft wording from the legal assessment (docs/16), pending counsel review.</small><p>RetroDrive is free and open source under the <b>Apache License 2.0</b>. Its source code is on GitHub (gollum10/RetroDrive); questions and problems go to its Issues page.</p>{native&&<p className="actions"><button className="ghost" onClick={()=>void invoke('open_third_party_notices',{which:'license'}).catch(e=>{ setError(true); setMessage(String(e)); })}>License</button><button className="ghost" onClick={()=>void invoke('open_third_party_notices',{which:'software'}).catch(e=>{ setError(true); setMessage(String(e)); })}>Third-party notices</button><button className="ghost" onClick={()=>void invoke('open_third_party_notices',{which:'data'}).catch(e=>{ setError(true); setMessage(String(e)); })}>Data credits</button></p>}</section>}
    <footer>RetroDrive · Early development build · Copies only after you confirm; never overwrites or deletes</footer>
    </main></div>;
}
