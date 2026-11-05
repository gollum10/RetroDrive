import { useEffect, useMemo, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { Activity, AlertTriangle, Database, FolderOpen, Gamepad2, HardDrive, LayoutDashboard, LibraryBig, RefreshCcw, Search, Settings2, ShieldCheck } from 'lucide-react';

type Availability = 'online'|'offline'|'missing';
type Game = { id: string; game_id: string; title: string; platform: string; path: string; file_size: number; availability: Availability };
type ScanIssue = { path: string; message: string };
type ScanSummary = { indexed: number; new_files: number; marked_missing: number; skipped_unsupported: number; skipped_links: number; complete: boolean; issue_count: number; issues: ScanIssue[] };
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
  const [games,setGames]=useState<Game[]>([]); const [drives,setDrives]=useState<Drive[]>([]);
  const [search,setSearch]=useState(''); const [busy,setBusy]=useState(false); const [message,setMessage]=useState('Ready. Connect a drive or import your existing ROM folder.');
  const native=isTauri(); const [issues,setIssues]=useState<ScanIssue[]>([]); const [error,setError]=useState(false);
  const load=async()=>{
    if(!native){setError(true);setMessage('Desktop commands require the Tauri application. Use npm run tauri dev, not the browser preview.');return;}
    try { const [g,d]=await Promise.all([invoke<Game[]>('list_games'),invoke<Drive[]>('list_drives')]); setGames(g);setDrives(d); }
    catch(e){setError(true);setMessage(`Could not load the catalog: ${String(e)}`);}
  };
  useEffect(()=>{void load();},[]);
  const importFolder=async()=>{
    try { const folder=await open({directory:true,multiple:false,title:'Select a folder containing games'});
      if(!folder || typeof folder!=='string')return;
      setBusy(true);setError(false);setIssues([]);setMessage('Scanning '+folder+'...');
      const r=await invoke<ScanSummary>('scan_library',{folder});
      const parts=[`Indexed ${r.indexed} game files (${r.new_files} new)`, `${r.skipped_unsupported} non-game files ignored`];
      if(r.marked_missing) parts.push(`${r.marked_missing} previously indexed files not found`);
      if(r.issue_count) parts.push(`${r.issue_count} item(s) could not be read`);
      if(!r.complete) parts.push('missing-file check skipped because the scan was incomplete');
      setError(r.issue_count>r.skipped_links); setIssues(r.issues);
      setMessage(parts.join(' · ')+'. No files were moved or modified.'); await load();
    } catch(e){setError(true);setMessage(`Import failed: ${String(e)}`)} finally{setBusy(false)}
  };
  const filtered=useMemo(()=>games.filter(g=>(g.title+' '+g.platform).toLowerCase().includes(search.toLowerCase())),[games,search]);
  return <div className="app">
    <aside className="sidebar"><div className="brand"><span className="brand-icon"><Gamepad2 size={25}/></span><div><strong>RETRODRIVE</strong><small>STUDIO <span className="brand-labs">/ an earlier publisher name</span></small></div></div>
      <div className="nav-label">WORKSPACE</div><nav>{menus.map(item=><button key={item.title} className={'nav-item '+(page===item.title?'selected':'')} onClick={()=>setPage(item.title)}><item.icon size={19}/>{item.title}</button>)}</nav>
      <div className="aside-bottom"><ShieldCheck size={17}/> Read-only disk discovery <small>v0.1.0 · dev build</small></div>
    </aside>
    <main><header><div><div className="eyebrow">RETRO GAMING COMMAND CENTER</div><h1>{page}</h1></div><button className="ghost" onClick={()=>void load()}><RefreshCcw size={16}/> Refresh</button></header>
    <div className={'notice '+(error?'warn':'')}><Activity size={17}/>{message}</div>
    {issues.length>0 && <details className="issues"><summary><AlertTriangle size={15}/> {issues.length} scan issue(s){issues.length>=200?' (first 200 shown)':''}</summary><ul>{issues.map((i,n)=><li key={n}><code>{i.path}</code> — {i.message}</li>)}</ul></details>}
    {page==='Dashboard' && <><section className="stats"><article><span>Cataloged games</span><strong>{games.length.toLocaleString()}</strong><small>Local database</small></article><article><span>Detected volumes</span><strong>{drives.length}</strong><small>Read-only discovery</small></article><article><span>Available titles</span><strong>{games.filter(g=>g.availability==='online').length}</strong><small>Files currently accessible</small></article></section><div className="twocol"><section className="panel"><h2>Quick actions</h2><p>Get started with your existing library. Scanning never changes game files.</p><button className="primary" disabled={!native||busy} onClick={importFolder}><FolderOpen size={18}/>{busy?'Scanning...':'Import ROM folder'}</button><button className="subtle" onClick={()=>setPage('Game Library')}><LibraryBig size={18}/> Browse library</button></section><section className="panel"><h2>Platform roadmap</h2><div className="platform-row"><span>Batocera</span><em>Adapter planned</em></div><div className="platform-row"><span>RetroBat</span><em>Adapter planned</em></div><div className="platform-row"><span>More systems</span><em>Future milestone</em></div></section></div></>}
    {page==='Game Library' && <section className="panel"><div className="section-heading"><div><h2>Master game catalog</h2><p>A single searchable view across imported folders.</p></div><button className="primary" disabled={!native||busy} onClick={importFolder}><FolderOpen size={17}/> Import folder</button></div><label className="search"><Search size={18}/><input placeholder="Search games or systems" value={search} onChange={e=>setSearch(e.target.value)}/></label><div className="table-wrap"><table><thead><tr><th>Game</th><th>Platform</th><th>Size</th><th>Availability</th></tr></thead><tbody>{filtered.map(g=><tr key={g.id}><td><b>{g.title}</b><small title={g.path}>{g.path}</small></td><td>{g.platform}</td><td>{(g.file_size/1048576).toFixed(1)} MB</td><td><span className={'state '+g.availability}>{availabilityLabel[g.availability]}</span></td></tr>)}</tbody></table>{filtered.length===0&&<div className="empty"><Database size={26}/>No games found. Import a ROM folder to begin.</div>}</div></section>}
    {page==='Drive Manager' && <section className="panel"><h2>Connected volumes</h2><p>Detected storage volumes are displayed for inspection. Formatting and image-writing are intentionally not implemented in this starter.</p><div className="drive-grid">{drives.map(d=><article className="drive" key={d.mount_point}><div className="drive-title"><HardDrive size={23}/><div><b>{d.name||d.mount_point}</b><small>{d.mount_point} {d.removable?'• Removable':''}</small></div></div><div className="bar"><div style={{width:`${d.total_bytes ? 100*(1-d.available_bytes/d.total_bytes):0}%`}}/></div><small>{bytes(Math.max(0,d.total_bytes-d.available_bytes))} used of {bytes(d.total_bytes)}</small></article>)}</div>{drives.length===0&&<div className="empty">No volumes reported by the desktop backend.</div>}</section>}
    {page==='Emulation Systems' && <section className="panel"><h2>Emulation systems</h2><p>Batocera and RetroBat adapters are the next milestone. This screen will manage verified installers, device compatibility, and installed versions.</p><div className="platform-row"><span>Batocera</span><em>Not yet connected</em></div><div className="platform-row"><span>RetroBat</span><em>Not yet connected</em></div></section>}
    {page==='Settings' && <section className="panel"><h2>Application settings</h2><p>RetroDrive Studio currently stores its SQLite catalog in the operating system's application data folder. Imports index files in place and never copy, move, or delete them.</p><p>Future settings: metadata providers, library scan schedules, backup locations, and platform defaults.</p></section>}
    <footer>RetroDrive Studio · Early development scaffold · No disk writes or platform installation features</footer>
    </main></div>;
}
