import { useEffect, useRef, useState } from 'react';
import type React from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { AlertTriangle, Check, Eye, Palette, Pencil, Plus, Trash2, Undo2, X } from 'lucide-react';
import { baseName, fmtSize, localTime, shortPath } from './format';
import BatoceraSettings from './BatoceraSettings';
import RetroPieSettings from './RetroPieSettings';

// ---------- data from the backend ----------
type ThemeVar = { name: string; value: string; kind: 'color'|'image'; path: string|null };
type ThemeInfo = { folder: string; path: string; active: boolean; credits: string[]; variables: ThemeVar[]; fonts: string[]; bytes: number; files: number; remixed_from: string|null; problem: string|null };
type ThemeList = { installation_path: string; themes_path: string; active: string|null; themes: ThemeInfo[] };
type IniOption = { section: string; key: string; label: string; help: string; kind: 'bool'|'choice'|'number'; choices: { label: string; value: string }[] };
type LookOptions = { options: IniOption[]; values: Record<string,string>; videos: string[]; videos_path: string; default_video_folder: boolean; music: string[]; music_path: string; tracks: string[]; favorites: string[]|null; file_sha: string; blocked: string|null };
type EmuChoice = { label: string; value: string };
type SystemSettings = { system: string; name: string; has_games: boolean; emulators: { emulator: string; cores: string[] }[]; current: Record<string,string> };
type SettingsOptions = { installation_path: string; systems: SystemSettings[]; shaders: EmuChoice[]; bezels: EmuChoice[]; ratios: EmuChoice[]; integer_scaling: EmuChoice[]; file_sha: string; blocked: string|null };
type SettingChange = { system: string; key: string; before: string|null; after: string|null };
type SettingsHistory = { id: number; when: string; state: string; changes: SettingChange[]; message: string|null; blocked: string|null };
type MusicSetting = { key: string; kind: 'bool'|'int'; label: string; current: string|null };
type ThemeChoice = { kind: 'existing'; folder: string } | { kind: 'remix'; base: string; name: string; colors: Record<string,string>; images: Record<string,string>; fonts: Record<string,string> };
type OwnMusic = { path: string; system: string|null };
type DesignSpec = { theme: ThemeChoice|null; ini: Record<string,string>; own_video: string|null; music: Record<string,string>; own_music: OwnMusic[]; systems: Record<string,string|null>; play_tracks: string[]|null };
type Design = { id: string; name: string; platform: string; spec: DesignSpec|null; draft: DesignSpec|null; updated_at: string; finished_at: string|null };
type DesignPreview = { design: string; theme_action: string|null; theme_folder: string|null; theme_bytes: number; files: { name: string; folder: string; action: string }[]; es_changes: SettingChange[]; ini_changes: SettingChange[]; es_sha: string; ini_sha: string; tracks_playing: number|null; tracks_left_out: number|null; nothing: boolean };
type ArtProgress = { phase: 'checking'|'copying'; done: number; total: number };
type SystemArt = { system: string; games_updated: number; games_added: number; images: number; kept: number };
type ArtPreview = { systems: SystemArt[]; images: number; image_bytes: number; games_updated: number; art_unavailable: number; list_shas: Record<string,string>; nothing: boolean; lists_not_created: string[] };
type Application = { id: number; design_id: string|null; design_name: string; state: string; added: string[]; when: string };
export type CustomizeInstall = { id: string; adapter_id: string; adapter_name: string; path: string; presence: string };

// ---------- labels ----------
const settingLabel: Record<string,string> = { emulator:'Emulator', core:'Core', shaderset:'Shader', bezel:'Bezel', ratio:'Aspect ratio', integerscale:'Integer scaling', ThemeSet:'Theme', smooth:'Smooth games', rewind:'Rewind', autosave:'Auto save/load', bgmusic:'Menu music',
  EnableIntro:'Boot video on', RandomVideo:'Random video', FileName:'Boot video', WaitForVideoEnd:'Wait for the video to end', GamepadVideoKill:'Skip with a gamepad', KillVideoWhenESReady:'Stop the video when ready', VideoDelay:'Video delay (ms)',
  InterfaceMode:'Interface mode', Fullscreen:'Fullscreen', FullscreenBorderless:'Borderless fullscreen', RandomTheme:'Random theme', NoExitMenu:'Hide the Quit option', VSync:'VSync in menus', DrawFramerate:'Show frame rate', LanguageDetection:"Use Windows' language", DisableWindowsInterface:'Hide Windows while playing',
  'audio.bgmusic':'Frontend music', 'audio.persystem':'Only system-specific music folder', 'audio.thememusics':'System-specific music (theme)', 'audio.useFavoriteMusic':'Play only favorite songs', favorites:'Tracks that play', 'audio.display_titles':'Song titles', 'audio.display_titles_time':'Title time (s)', MusicVolume:'Music volume', StopMusicOnScreenSaver:'Stop music in screensaver', VideoLowersMusic:'Lower music for videos' };
const INI_SECTIONS=['SplashScreen','EmulationStation','RetroBat'];
const MUSIC_KEYS=['audio.useFavoriteMusic','audio.bgmusic','audio.persystem','audio.thememusics','audio.display_titles','audio.display_titles_time','MusicVolume','StopMusicOnScreenSaver','VideoLowersMusic'];
const ON_OFF_KEYS=['EnableIntro','RandomVideo','WaitForVideoEnd','GamepadVideoKill','KillVideoWhenESReady','Fullscreen','FullscreenBorderless','RandomTheme','NoExitMenu','VSync','DrawFramerate','LanguageDetection','integerscale'];
const showSetting=(key:string, v:string|null)=> v===null ? 'AUTO' : ON_OFF_KEYS.includes(key) ? (v==='1'?'On':v==='0'?'Off':v) : v==='true' ? 'On' : v==='false' ? 'Off' : key==='InterfaceMode' ? ({'0':'Normal','1':'Kiosk','2':'Kid'}[v]??v) : key==='DisableWindowsInterface' ? ({'0':'No','1':'Notifications','2':'Notifications and desktop'}[v]??v) : v;
/** One change in words. Music settings are whole keys (e.g. audio.persystem); others are system + key. */
const changeText=(c:SettingChange)=>{ const full=c.system?`${c.system}.${c.key}`:c.key; const music=MUSIC_KEYS.includes(full); const key=music?full:c.key;
  const show=(v:string|null)=> v===null&&music ? 'default' : showSetting(key,v);
  return `${!music&&c.system&&!INI_SECTIONS.includes(c.system)?c.system+' ':''}${settingLabel[key]??key}: ${show(c.before)} → ${show(c.after)}`; };
const varLabel=(n:string)=>{ const w=n.replace(/([a-z])([A-Z])/g,'$1 $2').toLowerCase(); return w.charAt(0).toUpperCase()+w.slice(1); };
const cssColor=(v:string|undefined, fallback:string)=> v&&/^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(v) ? `#${v}` : fallback;
const emptySpec=():DesignSpec=>({ theme:null, ini:{}, own_video:null, music:{}, own_music:[], systems:{}, play_tracks:null });
const LEAVE='__leave';

// ---------- theme preview ----------
const previewFonts=new Map<string,string>();
function previewFamily(url:string|null):string|null { if(!url) return null; let family=previewFonts.get(url); if(!family){ family=`RDThemeFont${previewFonts.size+1}`; previewFonts.set(url,family); try { const ff=new FontFace(family,`url(${url})`); void ff.load().then(f=>{ document.fonts.add(f); }).catch(()=>undefined); } catch { /* preview only */ } } return family; }
const SAMPLE_GAMES=['Adventure Island','Battle Racer','Castle Quest','Dragon Valley','Echo Run','Final Lap','Galaxy Force','Hero Tales'];
/** An approximate RetroBat screen in the theme's colors, picture and font (only RetroBat draws the real thing). */
function ThemeMockup({ vars, background, font }: { vars: Record<string,string>; background: string|null; font: string|null }) {
  const accent=cssColor(vars.baseColor??Object.values(vars).find(v=>/^[0-9a-f]{6}/i.test(v)), '#3675ca');
  const bg=cssColor(vars.backgroundColor,'#101820'); const bar=cssColor(vars.bottomBarColor,accent); const glow=cssColor(vars.glowColor,'#ffffff20'); const fade=cssColor(vars.centerFadeColor,'#00000000'); const info=cssColor(vars.systemInfoColor,'#ffffffcc');
  const screen:React.CSSProperties={ background:bg, backgroundImage:background?`radial-gradient(circle, ${fade}, transparent 70%), url("${background}")`:`radial-gradient(circle, ${fade}, transparent 70%)`, backgroundSize:'cover', fontFamily:font?`${font}, sans-serif`:'sans-serif' };
  return <div className="mockups">
    <div className="mock" style={screen}><div className="mock-title" style={{color:info}}>SYSTEMS</div>
      <div className="mock-carousel">{['NES','Super Nintendo','Genesis'].map((n,i)=><div key={n} className="mock-tile" style={i===1?{borderColor:accent, boxShadow:`0 0 18px ${glow}`, color:'#fff'}:{color:info}}>{n}</div>)}</div>
      <div className="mock-info" style={{color:info}}>214 games · 48 favorites</div>
      <div className="mock-bar" style={{background:bar}}>A select · B back · START menu</div></div>
    <div className="mock" style={screen}><div className="mock-title" style={{color:info}}>SUPER NINTENDO</div>
      <div className="mock-list">{SAMPLE_GAMES.map((g,i)=><div key={g} style={i===2?{background:accent, color:'#fff'}:{color:info}}>{g}</div>)}</div>
      <div className="mock-art" style={{borderColor:accent}}>box art</div>
      <div className="mock-bar" style={{background:bar}}>A launch · Y favorite · SELECT options</div></div>
    <small className="plan-notes">Approximate preview: RetroBat lays the theme out its own way.</small>
  </div>;
}

/** What a design changes, in a few words. */
function summary(spec:DesignSpec|null):string[] {
  if(!spec) return ['Empty'];
  const out:string[]=[];
  if(spec.theme) out.push(spec.theme.kind==='existing'?`Theme: ${spec.theme.folder}`:`Theme: "${spec.theme.name}" (remix of ${spec.theme.base})`);
  if(spec.own_video) out.push(`Boot video: ${baseName(spec.own_video)} (yours)`); else if(spec.ini['SplashScreen.EnableIntro']==='0') out.push('Boot video: off'); else if(spec.ini['SplashScreen.RandomVideo']==='1') out.push('Boot video: random'); else if(spec.ini['SplashScreen.FileName']) out.push(`Boot video: ${spec.ini['SplashScreen.FileName']}`);
  const music=Object.keys(spec.music).length; if(music||spec.own_music.length) out.push(`Music: ${music} setting${music===1?'':'s'}${spec.own_music.length?`, ${spec.own_music.length} track${spec.own_music.length===1?'':'s'} added`:''}`);
  if(spec.play_tracks) out.push(`Plays only ${spec.play_tracks.length} chosen track${spec.play_tracks.length===1?'':'s'}`);
  const behavior=Object.keys(spec.ini).filter(k=>!k.startsWith('SplashScreen.')).length; if(behavior) out.push(`Behavior: ${behavior}`);
  const emu=new Set(Object.keys(spec.systems).map(k=>k.split('.')[0])).size; if(emu) out.push(`Emulators: ${emu} system${emu===1?'':'s'}`);
  return out.length?out:['No changes yet'];
}

export default function Customize({ installs, native, busy, notify }: { installs: CustomizeInstall[]; native: boolean; busy: boolean; notify: (text:string, error?:boolean)=>void }) {
  // Designs, themes and settings are RetroBat's; Batocera gets game art and details (settings and themes are next).
  const targets=installs.filter(i=>i.adapter_id==='retrobat'||i.adapter_id==='batocera'||i.adapter_id==='retropie'||i.adapter_id==='esde'||i.adapter_id==='recalbox');
  const [targetId,setTargetId]=useState(''); const target=targets.find(i=>i.id===targetId)??targets[0];
  const [designs,setDesigns]=useState<Design[]>([]);
  const [themes,setThemes]=useState<ThemeList|null>(null); const [look,setLook]=useState<LookOptions|null>(null); const [emu,setEmu]=useState<SettingsOptions|null>(null); const [music,setMusic]=useState<MusicSetting[]|null>(null);
  const [apps,setApps]=useState<Application[]>([]); const [history,setHistory]=useState<SettingsHistory[]>([]); const [showDetails,setShowDetails]=useState(false);
  const [editing,setEditing]=useState<{id:string; name:string; spec:DesignSpec; finishedBefore:boolean}|null>(null); const [tab,setTab]=useState<'theme'|'boot'|'music'|'behavior'|'emulators'>('theme');
  const [newName,setNewName]=useState(''); const [working,setWorking]=useState(false); const [confirm,setConfirm]=useState<string|null>(null);
  const [applyFor,setApplyFor]=useState<{design:Design; preview:DesignPreview}|null>(null); const [emuAll,setEmuAll]=useState(false);
  const [assets,setAssets]=useState<Record<string,string>>({}); const saveTimer=useRef<number|undefined>(undefined);
  const [art,setArt]=useState<ArtPreview|null>(null); const [createLists,setCreateLists]=useState(false); const [artProgress,setArtProgress]=useState<ArtProgress|null>(null); const [artChecking,setArtChecking]=useState(false); const [artError,setArtError]=useState('');
  useEffect(()=>{ const un=listen<ArtProgress>('art-progress', e=>setArtProgress(e.payload)); return ()=>{ void un.then(f=>f()); }; },[]);
  const connected=!!target&&target.presence==='connected'; const isRetroBat=target?.adapter_id==='retrobat'; const sysName=target?.adapter_name??'RetroBat';
  const blocked=look?.blocked??null;

  const fail=(what:string)=>(e:unknown)=>notify(`${what}: ${String(e)}`, true);
  const loadDesigns=async()=>setDesigns(await invoke<Design[]>('list_designs'));
  // Each read is independent: one failing (e.g. RetroBat never started, so es_systems.cfg is missing) doesn't hide the rest.
  const loadTarget=async(id:string)=>{ const args={installationId:id}; const errors:unknown[]=[];
    const one=<T,>(cmd:string, set:(v:T)=>void)=>invoke<T>(cmd,args).then(set,(e)=>{ errors.push(e); });
    if(installs.find(i=>i.id===id)?.adapter_id!=='retrobat'){ setThemes(null); setLook(null); setEmu(null); setMusic(null); setHistory([]); await Promise.all([one<Application[]>('design_applications',setApps), one<SettingsHistory[]>('emulator_settings_history',setHistory)]); if(errors.length) throw errors[0]; return; }
    await Promise.all([one<ThemeList>('list_themes',setThemes), one<LookOptions>('look_options',setLook), one<SettingsOptions>('emulator_settings',setEmu), one<MusicSetting[]>('music_settings',setMusic), one<Application[]>('design_applications',setApps), one<SettingsHistory[]>('emulator_settings_history',setHistory)]);
    if(errors.length) throw errors[0]; };
  useEffect(()=>{ if(native) void loadDesigns().catch(fail('Could not load your designs')); },[native]);
  useEffect(()=>{ if(native&&connected&&target) void loadTarget(target.id).catch(fail(`Could not read ${target.adapter_name}'s settings`)); },[native, target?.id, connected]);
  const asset=async(path:string|null|undefined)=>{ if(!path) return null; if(assets[path]) return assets[path]; try { const url=await invoke<string>('theme_preview_asset',{path}); setAssets(a=>({...a,[path]:url})); return url; } catch { return null; } };

  // ----- designs list -----
  const startDesign=async(fromCurrent:boolean)=>{ const name=newName.trim()||(fromCurrent?'My RetroBat setup':'New design'); setWorking(true);
    try { const id=await invoke<string>('create_design',{name, fromInstallation: fromCurrent&&target?target.id:null}); setNewName(''); const list=await invoke<Design[]>('list_designs'); setDesigns(list); const d=list.find(x=>x.id===id); if(d) openStudio(d); }
    catch(e){ fail('Could not start the design')(e); } finally { setWorking(false); } };
  const openStudio=(d:Design)=>{ setApplyFor(null); setConfirm(null); setTab('theme'); setEditing({id:d.id, name:d.name, spec:{...emptySpec(), ...(d.draft??d.spec??{})}, finishedBefore:!!d.spec}); };
  const deleteDesign=async(d:Design)=>{ if(confirm!==`delete:${d.id}`){ setConfirm(`delete:${d.id}`); return; } try { await invoke('delete_design',{id:d.id}); setConfirm(null); notify(`Design "${d.name}" deleted. Nothing on your drives changed.`); await loadDesigns(); } catch(e){ fail('Could not delete the design')(e); } };

  // ----- studio -----
  const save=(next:{id:string; name:string; spec:DesignSpec})=>{ window.clearTimeout(saveTimer.current); saveTimer.current=window.setTimeout(()=>{ void invoke('save_design_draft',{id:next.id, name:next.name, spec:next.spec}).catch(fail('Could not save your draft')); },400); };
  const update=(fn:(s:DesignSpec)=>DesignSpec)=>setEditing(ed=>{ if(!ed) return ed; const next={...ed, spec:fn(ed.spec)}; save(next); return next; });
  const rename=(name:string)=>setEditing(ed=>{ if(!ed) return ed; const next={...ed, name}; if(name.trim()) save(next); return next; });
  const finish=async()=>{ if(!editing) return; window.clearTimeout(saveTimer.current); setWorking(true);
    try { await invoke('save_design_draft',{id:editing.id, name:editing.name, spec:editing.spec}); await invoke('finish_design',{id:editing.id}); notify(`Design "${editing.name}" finished. Apply it to RetroBat from My designs when you're ready.`); setEditing(null); await loadDesigns(); }
    catch(e){ fail('Could not finish the design')(e); } finally { setWorking(false); } };
  const abandon=async()=>{ if(!editing) return; if(confirm!=='abandon'){ setConfirm('abandon'); return; } window.clearTimeout(saveTimer.current);
    try { const kept=await invoke<boolean>('abandon_design',{id:editing.id}); notify(kept?`Your edits to "${editing.name}" were thrown away; the finished design is unchanged.`:`Design "${editing.name}" abandoned.`); setEditing(null); setConfirm(null); await loadDesigns(); }
    catch(e){ fail('Could not abandon the design')(e); } };
  const setIni=(key:string, value:string)=>update(s=>{ const ini={...s.ini}; if(value===LEAVE) delete ini[key]; else ini[key]=value; return {...s, ini}; });
  const setMusicSetting=(key:string, value:string)=>update(s=>{ const m={...s.music}; if(value===LEAVE||value==='') delete m[key]; else m[key]=value; return {...s, music:m}; });
  const setSystem=(sys:string, values:Record<string,string|null|undefined>)=>update(s=>{ const systems={...s.systems}; for(const [k,v] of Object.entries(values)){ const key=`${sys}.${k}`; if(v===undefined) delete systems[key]; else systems[key]=v; } return {...s, systems}; });
  const chooseFile=async(title:string, name:string, extensions:string[])=>{ const path=await open({multiple:false,directory:false,title,filters:[{name,extensions}]}); return path&&typeof path==='string'?path:null; };

  // ----- apply -----
  const reviewApply=async(d:Design)=>{ if(!target) return; setConfirm(null); try { setApplyFor({design:d, preview: await invoke<DesignPreview>('preview_design',{id:d.id, installationId:target.id})}); } catch(e){ fail(`"${d.name}" can't be applied to this RetroBat`)(e); } };
  const doApply=async()=>{ if(!applyFor||!target) return; if(confirm!=='apply'){ setConfirm('apply'); return; } setWorking(true);
    notify(applyFor.preview.theme_action==='create'?`Applying "${applyFor.design.name}": copying ${fmtSize(applyFor.preview.theme_bytes)} for the new theme. This can take a few minutes on a USB drive…`:`Applying "${applyFor.design.name}"…`);
    try { const m=await invoke<string>('apply_design',{id:applyFor.design.id, installationId:target.id, esSha:applyFor.preview.es_sha, iniSha:applyFor.preview.ini_sha}); notify(`${m} Start RetroBat to see it; undo it under Applied designs.`); setApplyFor(null); await loadTarget(target.id); }
    catch(e){ fail('Could not apply the design')(e); } finally { setWorking(false); setConfirm(null); } };
  const undoApp=async(a:Application)=>{ if(!target) return; if(confirm!==`undo:${a.id}`){ setConfirm(`undo:${a.id}`); return; } setWorking(true);
    try { notify(await invoke<string>('undo_design_application',{id:a.id})); await loadTarget(target.id); } catch(e){ fail('Could not undo')(e); } finally { setWorking(false); setConfirm(null); } };
  const undoChange=async(h:SettingsHistory)=>{ if(!target) return; if(confirm!==`change:${h.id}`){ setConfirm(`change:${h.id}`); return; } setWorking(true);
    try { notify(`RetroBat settings: ${await invoke<string>('undo_emulator_settings',{changeId:h.id})}`); await loadTarget(target.id); } catch(e){ fail('Could not undo')(e); } finally { setWorking(false); setConfirm(null); } };

  // ----- game art -----
  const reviewArt=async(create:boolean=createLists)=>{ if(!target||artChecking) return; setConfirm(null); setArtError(''); setArtChecking(true); setArtProgress(null); setCreateLists(create);
    try { setArt(await invoke<ArtPreview>('preview_retrobat_art',{installationId:target.id, createLists:create})); } catch(e){ setArtError(String(e)); fail('Could not check the game lists')(e); } finally { setArtChecking(false); setArtProgress(null); } };
  const applyArt=async()=>{ if(!target||!art) return; if(confirm!=='art'){ setConfirm('art'); return; } setWorking(true); setArtProgress(null);
    notify(`Adding art and details: copying ${art.images.toLocaleString()} images (${fmtSize(art.image_bytes)}). This can take a while on a USB drive…`);
    try { notify(await invoke<string>('apply_retrobat_art',{installationId:target.id, listShas:art.list_shas, createLists})); setArt(null); await loadTarget(target.id); }
    catch(e){ fail('Could not add the art')(e); } finally { setWorking(false); setConfirm(null); setArtProgress(null); } };

  // ----- controls -----
  /** A retrobat.ini option in the studio: "Leave as is" or a value. */
  const iniPick=(o:IniOption, s:DesignSpec)=>{ const key=`${o.section}.${o.key}`; const now=look?.values[key]; const v=key in s.ini?s.ini[key]:LEAVE;
    if(o.kind==='number') return <span className="num-pick"><select className="choice" value={v===LEAVE?LEAVE:'set'} onChange={e=>setIni(key, e.target.value===LEAVE?LEAVE:(now??'1000'))}><option value={LEAVE}>Leave as is{now?` (${now})`:''}</option><option value="set">Set to…</option></select>{v!==LEAVE&&<input className="num" type="number" min={0} max={60000} step={100} value={v} onChange={e=>setIni(key,e.target.value)}/>}</span>;
    const choices=o.key==='FileName'?(look?.videos??[]).map(x=>({label:x,value:x})):o.kind==='bool'?[{label:'On',value:'1'},{label:'Off',value:'0'}]:o.choices;
    if(o.key==='FileName'&&s.own_video) return <select className="choice" value="own" disabled><option value="own">Your video: {baseName(s.own_video)}</option></select>;
    return <select className="choice" value={v} onChange={e=>setIni(key,e.target.value)}><option value={LEAVE}>Leave as is{now!==undefined?` (${showSetting(o.key,now)})`:''}</option>{choices.map(c=><option key={c.value} value={c.value}>{c.label}</option>)}</select>; };
  const iniGrid=(s:DesignSpec, section:(o:IniOption)=>boolean)=> <div className="option-grid">{(look?.options??[]).filter(section).map(o=><label key={o.key} title={o.help}><span>{o.label}{o.help&&<small>{o.help}</small>}</span>{iniPick(o,s)}</label>)}</div>;

  if(!native) return null;
  if(!target) return <section className="panel customize"><h2>Customize</h2><div className="empty">Add a RetroBat or Batocera installation under Emulation Systems first. RetroPie and other systems are planned.</div></section>;

  // ================= studio =================
  if(editing){ const s=editing.spec; const theme=s.theme;
    const remixBase=theme?.kind==='remix'?themes?.themes.find(t=>t.folder===theme.base)??null:null;
    return <section className="panel customize studio">
      <div className="studio-bar"><div><small className="eyebrow">Design studio</small><input className="design-name" aria-label="Design name" value={editing.name} maxLength={60} onChange={e=>rename(e.target.value)}/>
        <small className="plan-notes">Nothing changes on RetroBat while you design. Your draft is saved as you go; you can leave and come back. Settings you don't touch are left as they are.</small></div>
        <div className="actions"><button className={confirm==='abandon'?'danger':'ghost'} disabled={working} onClick={()=>void abandon()} onBlur={()=>{ if(confirm==='abandon') setConfirm(null); }}><X size={16}/>{confirm==='abandon'?(editing.finishedBefore?'Click again to throw away these edits':'Click again to abandon'):'Abandon design'}</button>
          <button className="primary" disabled={working||!editing.name.trim()||(editing.spec.play_tracks!==null&&editing.spec.play_tracks.length===0)} onClick={()=>void finish()}><Check size={16}/> Finish design</button></div></div>
      <div className="tabs" role="tablist">{([['theme','Theme'],['boot','Boot video'],['music','Music'],['behavior','Behavior'],['emulators','Emulators']] as const).map(([k,l])=><button key={k} role="tab" aria-selected={tab===k} className={tab===k?'on':''} onClick={()=>setTab(k)}>{l}</button>)}</div>
      {!connected&&<p className="warn-text">The drive for {target.path} isn't connected, so its themes and choices can't be listed. You can still edit what's already in the design.</p>}

      {tab==='theme'&&<div className="backups">
        <div className="radio-row">
          <label><input type="radio" checked={!theme} onChange={()=>update(x=>({...x, theme:null}))}/> Leave RetroBat's theme as is{themes?.active?` (${themes.active})`:''}</label>
          <label><input type="radio" checked={theme?.kind==='existing'} onChange={()=>update(x=>({...x, theme:{kind:'existing', folder:themes?.active??themes?.themes[0]?.folder??''}}))}/> Use a theme on the drive</label>
          <label><input type="radio" checked={theme?.kind==='remix'} onChange={()=>{ const base=themes?.themes.find(t=>!t.problem); if(!base) return; void asset(base.variables.find(v=>v.kind==='image')?.path); update(x=>({...x, theme:{kind:'remix', base:base.folder, name:`${editing.name} theme`, colors:{}, images:{}, fonts:{}}})); }}/> Remix a theme (your colors, picture and fonts)</label>
        </div>
        {theme?.kind==='existing'&&<div className="theme-grid">{(themes?.themes??[]).map(t=><article key={t.folder} className={'theme-card'+(theme.folder===t.folder?' active':'')} onClick={()=>update(x=>({...x, theme:{kind:'existing', folder:t.folder}}))}>
          <div className="swatches">{t.variables.filter(v=>v.kind==='color').slice(0,6).map(v=><span key={v.name} style={{background:cssColor(v.value,'#000')}}/>)}</div><b>{t.folder}</b>{theme.folder===t.folder&&<span className="tag identical">Chosen</span>}<small>{t.remixed_from?`Remix of ${t.remixed_from} · `:''}{fmtSize(t.bytes)}</small></article>)}</div>}
        {theme?.kind==='remix'&&(()=>{ const base=remixBase; const vars:Record<string,string>={}; base?.variables.forEach(v=>{ vars[v.name]=theme.colors[v.name]??v.value; });
          const bgVar=base?.variables.find(v=>v.kind==='image'); const bgPath=bgVar?(theme.images[bgVar.name]??bgVar.path):null; const regular=base?.fonts.find(x=>/regular/i.test(x))??base?.fonts[0];
          const fontPath=regular&&base?(theme.fonts[regular]??`${base.path}\\${regular.replace(/\//g,'\\')}`):null; if(bgPath&&!assets[bgPath]) void asset(bgPath); if(fontPath&&!assets[fontPath]) void asset(fontPath);
          const setTheme=(patch:Partial<Extract<ThemeChoice,{kind:'remix'}>>)=>update(x=>x.theme?.kind==='remix'?{...x, theme:{...x.theme, ...patch}}:x);
          return <div className="remix"><div className="remix-form">
            <label>New theme name <input value={theme.name} maxLength={60} onChange={e=>setTheme({name:e.target.value})}/></label>
            <label>Based on <select className="choice" value={theme.base} onChange={e=>setTheme({base:e.target.value, colors:{}, images:{}, fonts:{}})}>{(themes?.themes??[]).filter(t=>!t.problem).map(t=><option key={t.folder} value={t.folder}>{t.folder}</option>)}</select></label>
            {base&&<small className="plan-notes">The theme is created when you apply the design (it copies {base.folder}, {fmtSize(base.bytes)}); {base.folder} itself is never changed.</small>}
            <h4>Colors</h4>{base?.variables.filter(v=>v.kind==='color').map(v=>{ const value=theme.colors[v.name]??v.value; const alpha=value.length===8?value.slice(6):'';
              return <label key={v.name} className="color-row"><input type="color" value={`#${value.slice(0,6)}`} onChange={e=>setTheme({colors:{...theme.colors,[v.name]:e.target.value.slice(1)+alpha}})}/>{varLabel(v.name)}<span className="mono">{value}</span>{theme.colors[v.name]&&<button className="link" onClick={()=>{ const c={...theme.colors}; delete c[v.name]; setTheme({colors:c}); }}>reset</button>}</label>; })}
            {base?.variables.filter(v=>v.kind==='image').map(v=><div key={v.name}><h4>{varLabel(v.name)}</h4><button className="ghost" onClick={()=>void chooseFile('Choose your picture','Pictures',['png','jpg','jpeg']).then(p=>{ if(p){ void asset(p); setTheme({images:{...theme.images,[v.name]:p}}); } })}>Choose your picture…</button> <small className="mono">{theme.images[v.name]?shortPath(theme.images[v.name]):v.value}</small></div>)}
            {base&&base.fonts.length>0&&<><h4>Fonts</h4>{base.fonts.map(f=><div key={f} className="font-row"><span className="mono">{f.split('/').pop()}</span><button className="ghost" onClick={()=>void chooseFile('Choose your font','Fonts',['ttf','otf']).then(p=>{ if(p){ void asset(p); setTheme({fonts:{...theme.fonts,[f]:p}}); } })}>Replace with my font…</button>{theme.fonts[f]&&<small className="mono">{shortPath(theme.fonts[f])}</small>}</div>)}</>}
            <small className="plan-notes">Use only pictures and fonts you have the right to use. Theme credits and licenses stay with the remix.</small>
          </div><ThemeMockup vars={vars} background={bgPath?assets[bgPath]??null:null} font={previewFamily(fontPath?assets[fontPath]??null:null)}/></div>; })()}
      </div>}

      {tab==='boot'&&<div className="backups"><h3>Boot video</h3><p className="plan-notes">The video RetroBat plays when it starts.</p>
        {iniGrid(s, o=>o.section==='SplashScreen')}
        <div className="own-file"><b>Your own video:</b> {s.own_video?<><span className="mono">{shortPath(s.own_video)}</span> <button className="link" onClick={()=>update(x=>({...x, own_video:null}))}>remove</button></>:<span className="plan-notes">none</span>}
          <button className="ghost" onClick={()=>void chooseFile('Choose your boot video (.mp4)','Videos',['mp4']).then(p=>{ if(p) update(x=>({...x, own_video:p, ini:{...x.ini, 'SplashScreen.EnableIntro':'1', 'SplashScreen.RandomVideo':'0'}})); })}><Plus size={15}/> Use my own video…</button></div>
        <small className="plan-notes">Your video is copied to RetroBat's video folder when you apply the design (never over another file) and becomes the boot video.</small></div>}

      {tab==='music'&&<div className="backups"><h3>Music</h3><p className="plan-notes">When and where RetroBat plays music in its menus. With <b>Music per system</b>, tracks in a system's own folder play while you browse that system.</p>
        <div className="option-grid">{(music??[]).filter(m=>m.key!=='audio.useFavoriteMusic').map(m=>{ const v=s.music[m.key]; const now=m.current;
          return <label key={m.key}><span>{m.label}</span>{m.kind==='bool'
            ?<select className="choice" value={v??LEAVE} onChange={e=>setMusicSetting(m.key,e.target.value)}><option value={LEAVE}>Leave as is{now?` (${showSetting(m.key,now)})`:' (default)'}</option><option value="true">On</option><option value="false">Off</option></select>
            :<span className="num-pick"><select className="choice" value={v===undefined?LEAVE:'set'} onChange={e=>setMusicSetting(m.key, e.target.value===LEAVE?LEAVE:(now??(m.key==='MusicVolume'?'100':'10')))}><option value={LEAVE}>Leave as is{now?` (${now})`:' (default)'}</option><option value="set">Set to…</option></select>{v!==undefined&&<input className="num" type="number" min={0} max={m.key==='MusicVolume'?100:60} value={v} onChange={e=>setMusicSetting(m.key,e.target.value)}/>}</span>}</label>; })}</div>
        <h4>Your own music</h4>
        {s.own_music.length===0?<p className="plan-notes">None yet. RetroBat already has {look?.music.length??0} track{look?.music.length===1?'':'s'}.</p>:<ul className="own-music">{s.own_music.map((m,i)=><li key={i}><span className="mono">{baseName(m.path)}</span>
          <select className="choice" value={m.system??''} onChange={e=>update(x=>({...x, own_music:x.own_music.map((y,j)=>j===i?{...y, system:e.target.value||null}:y)}))}><option value="">All systems (main music folder)</option>{(emu?.systems??[]).filter(x=>x.has_games||x.system===m.system).map(x=><option key={x.system} value={x.system}>{x.name} only</option>)}</select>
          <button className="link" onClick={()=>update(x=>({...x, own_music:x.own_music.filter((_,j)=>j!==i)}))}>remove</button></li>)}</ul>}
        <button className="ghost" onClick={()=>void chooseFile('Choose your music (.ogg or .mp3)','Music',['ogg','mp3']).then(p=>{ if(p) update(x=>({...x, own_music:[...x.own_music, {path:p, system:null}]})); })}><Plus size={15}/> Add my own music…</button>
        <small className="plan-notes">Tracks are copied when you apply the design, as new files. A track for one system plays there only when Music per system is on. Use only music you have the right to use.</small>
        {(()=>{ const added=s.own_music.map(m=>m.system?`${m.system}/${baseName(m.path)}`:baseName(m.path)); const all=[...(look?.tracks??[]), ...added.filter(a=>!(look?.tracks??[]).some(t=>t.toLowerCase()===a.toLowerCase()))];
          const fav=look?.favorites; const usingFav=(music??[]).find(m=>m.key==='audio.useFavoriteMusic')?.current==='true';
          const picking=s.play_tracks!==null; const chosen=new Set((s.play_tracks??[]).map(t=>t.toLowerCase()));
          const toggle=(t:string)=>update(x=>{ const list=x.play_tracks??[]; const has=list.some(y=>y.toLowerCase()===t.toLowerCase()); return {...x, play_tracks: has?list.filter(y=>y.toLowerCase()!==t.toLowerCase()):[...list, t]}; });
          return <div className="track-picks"><h4>Which tracks play</h4>
            <p className="plan-notes">Untick the songs you don't want to hear. They aren't deleted; RetroBat just won't play them (this uses RetroBat's own <b>Play only songs from your favorites playlist</b>).</p>
            <div className="radio-row">
              <label><input type="radio" checked={!picking} onChange={()=>update(x=>({...x, play_tracks:null}))}/> Leave as is ({usingFav&&fav?`RetroBat plays its favorites list: ${fav.length} track${fav.length===1?'':'s'}`:`all ${all.length} tracks can play`})</label>
              <label><input type="radio" checked={picking} onChange={()=>update(x=>({...x, play_tracks: usingFav&&fav&&fav.length?fav:all}))}/> Play only the tracks I tick</label>
            </div>
            {picking&&<><div className="track-actions"><small>{chosen.size} playing · {Math.max(0, all.length-chosen.size)} left out</small><button className="link" onClick={()=>update(x=>({...x, play_tracks:all}))}>tick all</button></div>
              <ul className="track-list">{all.map(t=><li key={t}><label><input type="checkbox" checked={chosen.has(t.toLowerCase())} onChange={()=>toggle(t)}/> <span className="mono">{t.replace(/\//g,' › ')}</span>{added.includes(t)&&<small className="tag identical">yours</small>}</label></li>)}</ul>
              {chosen.size===0&&<p className="warn-text">Tick at least one track, or turn Frontend music off above.</p>}</>}
          </div>; })()}</div>}

      {tab==='behavior'&&<div className="backups"><h3>Behavior</h3><p className="plan-notes">How RetroBat starts and what players can do.</p>{iniGrid(s, o=>o.section!=='SplashScreen')}</div>}

      {tab==='emulators'&&emu&&<div className="backups emu"><h3>Emulators</h3><p className="plan-notes">Per-system choices. <b>AUTO</b> hands the choice back to RetroBat; <b>Leave as is</b> doesn't change it. A display setting is used only when the chosen emulator supports it.</p>
        <label className="check"><input type="checkbox" checked={emuAll} onChange={e=>setEmuAll(e.target.checked)}/> Show all {emu.systems.length} systems</label>
        <div className="table-wrap"><table><thead><tr><th>System</th><th>Emulator</th><th>Shader</th><th>Bezel</th><th>Aspect ratio</th><th>Integer scaling</th></tr></thead><tbody>{emu.systems.filter(x=>emuAll||x.has_games||Object.keys(x.current).length>0||Object.keys(s.systems).some(k=>k.startsWith(x.system+'.'))).map(sys=>{
          const get=(k:string)=>{ const key=`${sys.system}.${k}`; return key in s.systems?(s.systems[key]??''):LEAVE; };
          const pick=(k:string, choices:EmuChoice[])=> <select className="choice" value={get(k)} onChange={e=>setSystem(sys.system,{[k]:e.target.value===LEAVE?undefined:e.target.value||null})}><option value={LEAVE}>Leave{sys.current[k]?` (${showSetting(k,sys.current[k])})`:''}</option><option value="">AUTO</option>{choices.map(c=><option key={c.value} value={c.value}>{c.label}</option>)}</select>;
          const ev=get('emulator'); const cv=get('core'); const value=ev===LEAVE?LEAVE:ev===''?'':`${ev}|${cv===LEAVE?'':cv}`;
          return <tr key={sys.system}><td><b>{sys.name}</b><small className="mono">{sys.system}</small></td>
            <td><select className="choice" value={value} onChange={e=>{ const v=e.target.value; if(v===LEAVE) setSystem(sys.system,{emulator:undefined, core:undefined}); else if(v==='') setSystem(sys.system,{emulator:null, core:null}); else { const [em,co]=v.split('|'); setSystem(sys.system,{emulator:em, core:co||null}); } }}>
              <option value={LEAVE}>Leave{sys.current.emulator?` (${sys.current.emulator}${sys.current.core?`: ${sys.current.core}`:''})`:''}</option><option value="">AUTO</option>{sys.emulators.flatMap(em=>em.cores.length?em.cores.map(co=><option key={`${em.emulator}|${co}`} value={`${em.emulator}|${co}`}>{em.emulator}: {co}</option>):[<option key={`${em.emulator}|`} value={`${em.emulator}|`}>{em.emulator}</option>])}</select></td>
            <td>{pick('shaderset',emu.shaders)}</td><td>{pick('bezel',emu.bezels)}</td><td>{pick('ratio',emu.ratios)}</td><td>{pick('integerscale',emu.integer_scaling)}</td></tr>; })}</tbody></table></div></div>}
    </section>; }

  // ================= designs and applying =================
  const lastApplied=apps.find(a=>a.state!=='undone');
  return <section className="panel customize">
    <div className="section-heading"><div><h2>Customize</h2><p>{isRetroBat?'Design how RetroBat looks and behaves (theme, boot video, music, behavior, emulators), then apply a finished design. Designs are kept in RetroDrive, so you can reuse them on other drives.':target.adapter_id==='batocera'?`Batocera's settings and theme, and your box art and catalog details in its menus.`:target.adapter_id==='esde'||target.adapter_id==='recalbox'?`Your box art and catalog details in ${sysName}.`:`${sysName}'s emulator choices, and your box art and catalog details in its menus.`}</p></div>
      {targets.length>1&&<label className="cust-pick">Apply to <select value={target.id} onChange={e=>setTargetId(e.target.value)}>{targets.map(i=><option key={i.id} value={i.id}>{i.adapter_name} · {i.path}</option>)}</select></label>}</div>
    <p className="mono cust-target">{target.adapter_name} · {target.path}{lastApplied?` · last applied: "${lastApplied.design_name}"`:''}</p>
    {!connected&&<p className="warn-text">{isRetroBat?"This RetroBat's drive isn't connected. You can still design; applying needs the drive.":`This ${sysName} isn't connected. Plug in its game drive, or start it so its network share is reachable.`}</p>}
    {blocked&&<p className="warn-text"><AlertTriangle size={15}/> {blocked} <button className="ghost" onClick={()=>void loadTarget(target.id)}>Check again</button></p>}

    {isRetroBat&&<div className="backups"><div className="designs-head"><h3>My designs</h3>
      <div className="actions"><input className="design-new" placeholder="Name for a new design" value={newName} maxLength={60} onChange={e=>setNewName(e.target.value)}/><button className="primary" disabled={working} onClick={()=>void startDesign(false)}><Plus size={16}/> New design</button><button className="ghost" disabled={working||!connected} title="Start from this RetroBat's current theme and settings" onClick={()=>void startDesign(true)}><Palette size={16}/> New from this RetroBat</button></div></div>
      {designs.length===0?<p className="plan-notes">No designs yet. Start one: nothing changes on RetroBat until you finish a design and apply it.</p>
      :<div className="theme-grid">{designs.map(d=><article key={d.id} className="theme-card design-card">
        <b>{d.name}</b>{!d.spec?<span className="tag conflict">Unfinished draft</span>:d.draft?<span className="tag conflict">Has unfinished edits</span>:<span className="tag identical">Finished</span>}
        <ul className="design-summary">{summary(d.spec??d.draft).map(x=><li key={x}>{x}</li>)}</ul>
        <div className="actions">{d.spec&&<button className="primary" disabled={working||busy||!connected} onClick={()=>void reviewApply(d)}>Apply to RetroBat…</button>}
          <button className="ghost" onClick={()=>openStudio(d)}><Pencil size={15}/>{d.draft?'Continue':'Edit'}</button>
          <button className={confirm===`delete:${d.id}`?'danger':'ghost'} onClick={()=>void deleteDesign(d)} onBlur={()=>{ if(confirm===`delete:${d.id}`) setConfirm(null); }}><Trash2 size={15}/>{confirm===`delete:${d.id}`?'Click again':'Delete'}</button></div></article>)}</div>}</div>}

    {applyFor&&<div className="confirm apply-design"><p><b>Apply "{applyFor.design.name}" to {target.path}?</b></p>
      {applyFor.preview.nothing?<p className="plan-notes">This RetroBat already looks like this design; nothing would change.</p>:<ul>
        {applyFor.preview.theme_action==='create'&&<li>Create the theme <b>{applyFor.preview.theme_folder}</b> (copies {fmtSize(applyFor.preview.theme_bytes)}; the original theme isn't changed).</li>}
        {applyFor.preview.theme_action==='reuse'&&<li>Use the theme <b>{applyFor.preview.theme_folder}</b> this design made before.</li>}
        {applyFor.preview.files.filter(f=>f.action==='add').map(f=><li key={f.folder+f.name}>Add <span className="mono">{f.name}</span> to <span className="mono">{f.folder.replace(/\//g,'\\')}</span></li>)}
        {applyFor.preview.tracks_playing!==null&&<li>Play only {applyFor.preview.tracks_playing} chosen track{applyFor.preview.tracks_playing===1?'':'s'}{applyFor.preview.tracks_left_out?` (${applyFor.preview.tracks_left_out} left out, not deleted)`:''}</li>}
        {applyFor.preview.es_changes.map(c=><li key={'e'+c.system+c.key}>{changeText(c)}</li>)}
        {applyFor.preview.ini_changes.map(c=><li key={'i'+c.system+c.key}>{changeText(c)}</li>)}
        <li className="plan-notes">Everything else stays as it is. RetroBat's settings files are backed up first; Undo puts them back in one step. Added files are kept.</li></ul>}
      <div className="actions"><button className="primary" disabled={working||!!blocked||applyFor.preview.nothing} onClick={()=>void doApply()}>{working?'Applying…':confirm==='apply'?'Click again to apply':'Apply design'}</button><button className="ghost" disabled={working} onClick={()=>{ setApplyFor(null); setConfirm(null); }}>Not now</button></div></div>}

    {target.adapter_id==='retropie'&&connected&&<RetroPieSettings installationId={target.id} disabled={busy} notify={notify} onSaved={()=>void loadTarget(target.id).catch(()=>{})}/>}
    {target.adapter_id==='batocera'&&connected&&<BatoceraSettings installationId={target.id} disabled={busy} notify={notify} onSaved={()=>void loadTarget(target.id).catch(()=>{})}/>}
    <div className="backups"><h3>Game art and details</h3>
      <p className="plan-notes">Show your own box art, logos and catalog details (title, year, genre, developer, publisher) in {sysName}'s menus for the games RetroDrive copied. {target.adapter_id!=='esde'&&<>Images are copied to each system's <span className="mono">images</span> folder as new files; {sysName}'s game lists only get what they're missing, so art {sysName} already shows is kept. </>}New copies get this automatically when you tick it while copying.{target.adapter_id==='batocera'&&' If Batocera is running, press Start → Game Settings → Update Gamelists afterwards to see the new art.'}{target.adapter_id==='recalbox'&&" If Recalbox is running, update the game lists from its menu (or restart it) afterwards to see the new art."}{target.adapter_id==='esde'&&" On ES-DE, details go into ES-DE's own game lists and pictures into its downloaded_media folder as covers and marquees, named after each game; pictures ES-DE already has are kept. Close ES-DE first."}{target.adapter_id==='retropie'&&" On RetroPie, only game lists already in its rom folders are filled in (RetroPie would otherwise stop using the list in its home folder, where any scraped art, favorites and play counts are). Restart RetroPie afterwards to see the new art."}</p>
      {art&&(art.lists_not_created.length>0||createLists)&&<div className="warn-box"><label className="check"><input type="checkbox" checked={createLists} disabled={artChecking||working} onChange={e=>void reviewArt(e.target.checked)}/> Also create game lists for {createLists?'systems without one':`${art.lists_not_created.length} system${art.lists_not_created.length===1?'':'s'} without one (${art.lists_not_created.join(', ')})`}</label>
        <small className="plan-notes">RetroPie will then use the new list instead of the one in its home folder, so any art, favorites or play counts it scraped or saved there won't show (they aren't deleted; Undo brings the old behavior back). Leave this off if you scraped on this RetroPie.</small></div>}
      {!art?<><div className="actions"><button className="ghost" disabled={working||busy||!connected||artChecking} onClick={()=>void reviewArt(false)}><Eye size={15}/> {artChecking?'Checking…':'Check what can be added'}</button></div>
        {artChecking&&<p className="plan-notes">{artProgress?.phase==='checking'?`Checking your games and their images: ${artProgress.done.toLocaleString()} of ${artProgress.total.toLocaleString()}…`:'Reading the catalog…'} A big library on a network drive can take a minute.</p>}
        {!artChecking&&artError&&<p className="warn-text">{artError}</p>}
        {!connected&&<p className="plan-notes">Connect {sysName} to check.</p>}
        {busy&&<p className="plan-notes">Available when the current scan or copy has finished.</p>}</>
      :art.nothing?<p className="plan-notes">{art.lists_not_created.length>0?<>Nothing to add without a game list (see above)</>:<>Nothing to add: the games RetroDrive copied already have art and details in {sysName}</>}{art.art_unavailable?`, or their images aren't available (${art.art_unavailable.toLocaleString()} games' images are on a drive that isn't connected)`:''}. <button className="link" onClick={()=>setArt(null)}>close</button></p>
      :<div className="confirm"><p><b>Add art and details for {art.games_updated.toLocaleString()} game{art.games_updated===1?'':'s'}</b> ({art.images.toLocaleString()} images, {fmtSize(art.image_bytes)}):</p>
        <div className="table-wrap"><table><thead><tr><th>System</th><th>Updated</th><th>Added to the list</th><th>Images</th><th>{sysName}'s art kept</th></tr></thead><tbody>{art.systems.map(x=><tr key={x.system}><td className="mono">{x.system}</td><td>{x.games_updated.toLocaleString()}</td><td>{x.games_added.toLocaleString()}</td><td>{x.images.toLocaleString()}</td><td>{x.kept.toLocaleString()}</td></tr>)}</tbody></table></div>
        {art.art_unavailable>0&&<p className="warn-text">{art.art_unavailable.toLocaleString()} games' images are on a drive that isn't connected; connect it and check again to include them.</p>}
        {working&&artProgress&&<p className="plan-notes">{artProgress.phase==='checking'?'Checking games again':'Copying images'}: {artProgress.done.toLocaleString()} of {artProgress.total.toLocaleString()}…</p>}
        <div className="actions"><button className="primary" disabled={working||!!blocked} onClick={()=>void applyArt()}>{working?'Adding…':confirm==='art'?'Click again to add':'Add art and details'}</button><button className="ghost" disabled={working} onClick={()=>{ setArt(null); setConfirm(null); }}>Not now</button></div></div>}
    </div>

    {apps.length>0&&<div className="backups"><h3>Applied designs</h3><div className="table-wrap"><table><thead><tr><th>When</th><th>Design</th><th>Added</th><th></th></tr></thead><tbody>{apps.map(a=><tr key={a.id}><td>{localTime(a.when)}</td><td><b>{a.design_name}</b></td><td>{a.added.length?a.added.map(x=><small key={x} className="mono">{x.replace(/\//g,'\\')}</small>):<small className="plan-notes">—</small>}</td>
      <td>{a.state==='undone'?<small>Undone</small>:<button className={confirm===`undo:${a.id}`?'primary':'ghost'} disabled={working||!!blocked} onClick={()=>void undoApp(a)} onBlur={()=>{ if(confirm===`undo:${a.id}`) setConfirm(null); }}><Undo2 size={15}/>{confirm===`undo:${a.id}`?'Click again to undo':'Undo'}</button>}</td></tr>)}</tbody></table></div></div>}

    {history.length>0&&<div className="backups"><button className="link" onClick={()=>setShowDetails(x=>!x)}><Eye size={14}/> {showDetails?'Hide':'Show'} every settings change ({history.length})</button>
      {showDetails&&<div className="table-wrap"><table><thead><tr><th>When</th><th>Changes</th><th></th></tr></thead><tbody>{history.map(h=><tr key={h.id}><td>{localTime(h.when)}</td><td>{h.changes.map(c=><small key={c.system+c.key}>{changeText(c)}</small>)}</td>
        <td>{h.state==='applied'&&!h.blocked?<button className={confirm===`change:${h.id}`?'primary':'ghost'} disabled={working} onClick={()=>void undoChange(h)} onBlur={()=>{ if(confirm===`change:${h.id}`) setConfirm(null); }}><Undo2 size={15}/>{confirm===`change:${h.id}`?'Click again':'Undo'}</button>:<small className={h.state==='applied'?'warn-text':''}>{h.state==='undone'?'Undone':h.state==='failed'?'Not applied':h.blocked}</small>}</td></tr>)}</tbody></table></div>}</div>}
  </section>;
}
