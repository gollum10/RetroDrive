// AI helpers for Theme Studio (ADR-118): connect Claude with the user's own key (kept in
// Windows Credential Manager) or a private AI on this PC or network. Nothing is sent anywhere
// until the user connects; design requests come in a later step.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { CheckCircle2, ExternalLink, Search } from 'lucide-react';

type Status = { claudeConnected: boolean; claudeModel: string; claudeModels: [string, string][]; privateUrl: string | null; privateModel: string | null };
type Found = { url: string; models: string[] };

export default function AiConnect({ notify }: { notify: (text: string, error?: boolean) => void }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState('');
  const [address, setAddress] = useState('http://localhost:11434');
  const [found, setFound] = useState<Found | null>(null);
  const [model, setModel] = useState('');
  // Results show inside the card that was used, so nobody has to scroll to see them.
  const [note, setNote] = useState<{ card: 'claude' | 'private'; text: string; error: boolean } | null>(null);
  const refresh = () => invoke<Status>('ai_status').then(setStatus).catch(e => notify(String(e), true));
  useEffect(() => { void refresh(); }, []);
  const say = (card: 'claude' | 'private', text: string, error = false) => { setNote({ card, text, error }); notify(text, error); };
  const run = async (card: 'claude' | 'private', label: string, task: () => Promise<void>) => { setBusy(label); setNote(null); try { await task(); } catch (e) { say(card, String(e), true); } finally { setBusy(''); void refresh(); } };
  const noteFor = (card: 'claude' | 'private') => note?.card === card && <p className={note.error ? 'notice warn' : 'notice'} role={note.error ? 'alert' : 'status'}>{note.text}</p>;

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
  </section>;
}
