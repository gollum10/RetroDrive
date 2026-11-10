// AI helpers for Theme Studio (ADR-118): connect Claude with the user's own key (kept in
// Windows Credential Manager), another online AI service with its key (ADR-121), or a private AI
// on this PC or network. Nothing is sent anywhere until the user connects and asks.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { CheckCircle2, ExternalLink, Search } from 'lucide-react';
import { savedLevel } from './theme-studio/skill';

type Status = { claudeConnected: boolean; claudeModel: string; claudeModels: [string, string][]; privateUrl: string | null; privateModel: string | null; services: Service[] };
type Found = { url: string; models: string[] };

// Other AI services (ADR-121): a pick-list of online services with built-in addresses, so a
// key only goes to its own service. A custom address is offered in Pro only.
type Service = { id: string; name: string; keysPage: string | null; connected: boolean; model: string | null; url: string | null };
function ModelChoice({ models, value, onChange }: { models: string[]; value: string; onChange: (m: string) => void }) {
  // Long lists (OpenRouter has hundreds) get a type-to-find box.
  if (models.length <= 30) return <select value={value} onChange={e => onChange(e.target.value)}>{models.map(m => <option key={m}>{m}</option>)}</select>;
  return <><input className="ai-address" list="ai-model-list" value={value} placeholder="Type to find a model" onChange={e => onChange(e.target.value)}/><datalist id="ai-model-list">{models.map(m => <option key={m} value={m}/>)}</datalist></>;
}
function ServicesCard({ services, busy, run, say, children }: { services: Service[]; children?: React.ReactNode; busy: string; run: (label: string, task: () => Promise<void>) => Promise<void>; say: (text: string, error?: boolean) => void }) {
  const pro = savedLevel() === 'pro';
  const [pick, setPick] = useState('');
  const [key, setKey] = useState('');
  const [url, setUrl] = useState('https://');
  const [models, setModels] = useState<string[] | null>(null);
  const [model, setModel] = useState('');
  const [changing, setChanging] = useState<{ id: string; models: string[]; model: string } | null>(null);
  const chosen = services.find(s => s.id === pick);
  const connected = services.filter(s => s.connected);
  const offered = services.filter(s => !s.connected && (s.id !== 'custom' || pro));
  const reset = () => { setKey(''); setModels(null); setModel(''); };
  return <div className="ai-card">
    <h3>Other AI services {connected.length > 0 && <span className="ok-text"><CheckCircle2 size={15}/> {connected.length} connected</span>}</h3>
    <p className="plan-notes">Already pay for ChatGPT's API, Google Gemini, OpenRouter or another service? Connect it with a key from that company. Prices and any free allowance are set by each company. Keys are kept in Windows Credential Manager and only ever sent to their own service.</p>
    {connected.map(s => <div key={s.id} className="actions ai-service-row">
      <b>{s.name}</b>{s.url && <span className="mono">{s.url}</span>}
      {changing?.id === s.id ? <><ModelChoice models={changing.models} value={changing.model} onChange={m => setChanging({ ...changing, model: m })}/><button className="primary" disabled={!!busy || !changing.models.includes(changing.model)} onClick={() => void run('svc-model', async () => { await invoke('ai_set_service_model', { id: s.id, model: changing.model }); setChanging(null); say(`${s.name} now uses ${changing.model}.`); })}>Use this model</button></>
        : <><span>using <b>{s.model}</b></span><button className="ghost" disabled={!!busy} onClick={() => void run('svc-list', async () => { const list = await invoke<string[]>('ai_service_models', { id: s.id }); setChanging({ id: s.id, models: list, model: s.model ?? list[0] }); })}>{busy === 'svc-list' ? 'Loading…' : 'Change model'}</button></>}
      <button className="ghost" disabled={!!busy} onClick={() => void run('svc-off', async () => { await invoke('ai_disconnect_service', { id: s.id }); say(`${s.name} disconnected. The key was removed from this PC.`); })}>Disconnect</button>
    </div>)}
    <div className="actions"><label>Add a service <select value={pick} disabled={!!busy} onChange={e => { setPick(e.target.value); reset(); }}><option value="">Choose…</option>{offered.map(s => <option key={s.id} value={s.id}>{s.id === 'custom' ? 'Another service (custom address)' : s.name}</option>)}</select></label></div>
    {chosen && <ol className="ai-steps">
      {chosen.id === 'custom' ? <li>Its internet address (OpenAI-style API): <input className="ai-address" value={url} spellCheck={false} onChange={e => { setUrl(e.target.value); setModels(null); }}/><p className="notice warn">Only use an address you trust: your key and the screen you ask about go to it.</p></li>
        : <li>Sign in to {chosen.name} and create a key. <button className="link" onClick={() => { void invoke('ai_open_service_keys', { id: chosen.id }).catch(e => say(String(e), true)); }}><ExternalLink size={14}/> Open the {chosen.name} key page</button></li>}
      <li>Paste the key here: <span className="ai-paste"><input type="password" autoComplete="off" spellCheck={false} value={key} onChange={e => { setKey(e.target.value); setModels(null); }}/><button className="ghost" disabled={!key.trim() || !!busy} onClick={() => void run('svc-check', async () => { const list = await invoke<string[]>('ai_check_service', { id: chosen.id, key, url: chosen.id === 'custom' ? url : null }); setModels(list); setModel(list[0]); })}>{busy === 'svc-check' ? 'Checking…' : 'Check key'}</button></span></li>
      {models && <li>Choose a model ({models.length} available): <span className="ai-paste"><ModelChoice models={models} value={model} onChange={setModel}/><button className="primary" disabled={!!busy || !models.includes(model)} onClick={() => void run('svc-connect', async () => { await invoke('ai_connect_service', { id: chosen.id, key, model, url: chosen.id === 'custom' ? url : null }); say(`${chosen.name} connected with ${model}.`); setPick(''); reset(); })}>{busy === 'svc-connect' ? 'Checking…' : 'Connect'}</button></span></li>}
    </ol>}
    {children}
  </div>;
}

type Card = 'claude' | 'private' | 'services';

export default function AiConnect({ notify }: { notify: (text: string, error?: boolean) => void }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState('');
  const [address, setAddress] = useState('http://localhost:11434');
  const [found, setFound] = useState<Found | null>(null);
  const [model, setModel] = useState('');
  // Results show inside the card that was used, so nobody has to scroll to see them.
  const [note, setNote] = useState<{ card: Card; text: string; error: boolean } | null>(null);
  const refresh = () => invoke<Status>('ai_status').then(setStatus).catch(e => notify(String(e), true));
  useEffect(() => { void refresh(); }, []);
  const say = (card: Card, text: string, error = false) => { setNote({ card, text, error }); notify(text, error); };
  const run = async (card: Card, label: string, task: () => Promise<void>) => { setBusy(label); setNote(null); try { await task(); } catch (e) { say(card, String(e), true); } finally { setBusy(''); void refresh(); } };
  const noteFor = (card: Card) => note?.card === card && <p className={note.error ? 'notice warn' : 'notice'} role={note.error ? 'alert' : 'status'}>{note.text}</p>;

  return <section className="panel ai-connect">
    <h2>AI helpers (optional)</h2>
    <p className="plan-notes">Let an AI help design themes in Theme Studio. Nothing is sent anywhere until you connect one here and ask it something. Your games, folders and settings are never sent.</p>

    <div className="ai-card">
      <h3>Claude {status?.claudeConnected && <span className="ok-text"><CheckCircle2 size={15}/> Connected</span>}</h3>
      {status?.claudeConnected ? <>
        <label>Model <select value={status.claudeModel} disabled={!!busy} onChange={e => void run('claude', 'model', async () => { await invoke('ai_set_claude_model', { model: e.target.value }); })}>{status.claudeModels.map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select></label>
        <p className="plan-notes">Uses your own Claude account; Anthropic charges it per request (a design suggestion usually costs a few cents). Your key is kept in Windows Credential Manager.</p>
        <div className="actions"><button className="ghost" disabled={!!busy} onClick={() => void run('claude', 'disconnect', async () => { await invoke('ai_disconnect_claude'); say('claude', 'Claude disconnected. The key was removed from this PC.'); })}>Disconnect</button></div>
      </> : <>
        <ol className="ai-steps">
          <li>Sign in to your Claude account (or create one and add a payment method). <button className="link" onClick={() => { void invoke('ai_open_keys_page').catch(e => say('claude', String(e), true)); }}><ExternalLink size={14}/> Open the Claude Console</button></li>
          <li>On the API keys page, click <b>Create key</b>, then <b>Copy</b>.</li>
          <li>Paste it here: <span className="ai-paste"><input type="password" autoComplete="off" spellCheck={false} placeholder="sk-ant-…" value={key} onChange={e => setKey(e.target.value)}/><button className="primary" disabled={!key.trim() || !!busy} onClick={() => void run('claude', 'connect', async () => { await invoke('ai_connect_claude', { key }); setKey(''); say('claude', 'Claude connected.'); })}>{busy === 'connect' ? 'Checking…' : 'Connect'}</button></span></li>
        </ol>
        <p className="plan-notes">A Claude.ai subscription doesn't include this; the Claude Console bills API use separately. RetroDrive checks the key once and keeps it in Windows Credential Manager, never in a file.</p>
      </>}
      {noteFor('claude')}
    </div>

    <div className="ai-card">
      <h3>Private AI on this PC or network {status?.privateUrl && <span className="ok-text"><CheckCircle2 size={15}/> Connected</span>}</h3>
      {status?.privateUrl ? <>
        <p>Using <b>{status.privateModel}</b> at <span className="mono">{status.privateUrl}</span>. Free; requests stay on your PC or network.</p>
        <div className="actions"><button className="ghost" disabled={!!busy} onClick={() => void run('private', 'forget', async () => { await invoke('ai_disconnect_private'); setFound(null); })}>Disconnect</button></div>
      </> : <>
        <p className="plan-notes">Free and private: run an AI program such as Ollama or LM Studio, download a model in it, then connect it here.</p>
        <div className="actions">
          <button disabled={!!busy} onClick={() => void run('private', 'find', async () => { const hit = await invoke<Found | null>('ai_find_private'); if (hit) { setFound(hit); setAddress(hit.url); setModel(hit.models[0]); } else say('private', 'No AI program answered at the usual addresses. Start it, or type its address.', true); })}><Search size={15}/> {busy === 'find' ? 'Looking…' : 'Find it on this PC'}</button>
          <input className="ai-address" value={address} onChange={e => { setAddress(e.target.value); setFound(null); }} placeholder="http://localhost:11434"/>
          <button className="ghost" disabled={!!busy || !address.trim()} onClick={() => void run('private', 'check', async () => { const hit = await invoke<Found>('ai_private_models', { url: address }); setFound(hit); setModel(hit.models[0]); })}>{busy === 'check' ? 'Checking…' : 'Check address'}</button>
        </div>
        {found && <div className="actions">
          <label>Model <select value={model} onChange={e => setModel(e.target.value)}>{found.models.map(m => <option key={m}>{m}</option>)}</select></label>
          <button className="primary" disabled={!!busy || !model} onClick={() => void run('private', 'private', async () => { await invoke('ai_connect_private', { url: found.url, model }); say('private', `Connected ${model}.`); })}>Connect</button>
        </div>}
      </>}
      {noteFor('private')}
    </div>

    {status && <><ServicesCard services={status.services} busy={busy} run={(label, task) => run('services', label, task)} say={(text, error) => say('services', text, error)}>{noteFor('services')}</ServicesCard></>}
  </section>;
}
