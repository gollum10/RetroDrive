// Ask AI for design suggestions (ADR-120). The suggestion is shown on the canvas straight away
// as one undoable step; Keep leaves it, Discard takes it back. Nothing is sent until Ask.
import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ProjectSession } from './session';
import type { Scene } from './types';
import { applyEdits, describeScreen } from './ai-suggest';

type Status = { claudeConnected: boolean; claudeModel: string; claudeModels: [string, string][]; privateUrl: string | null; privateModel: string | null; services: { id: string; name: string; connected: boolean; model: string | null }[] };
/** Every connected AI, Claude first (ADR-121). */
function choices(s: Status): [string, string][] {
  return [
    ...(s.claudeConnected ? [['claude', 'Claude'] as [string, string]] : []),
    ...s.services.filter(v => v.connected).map(v => [`service:${v.id}`, `${v.name} (${v.model})`] as [string, string]),
    ...(s.privateUrl ? [['private', `${s.privateModel} (private)`] as [string, string]] : []),
  ];
}
type Answer = { explanation: string; edits: unknown[]; answeredBy: string; costUsd: number | null; requestsLeft: number };
export const AI_LABEL = 'AI suggestion';
const IDEAS = ['Choose a colour scheme', 'Make the text easier to read', 'Tidy up the layout', 'Give it an arcade feel'];

export default function AskAi({ session, scene, showExactly, onReview, onClose }: { session: ProjectSession; scene: Scene; showExactly: boolean; onReview: (reviewing: boolean) => void; onClose: () => void }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [provider, setProvider] = useState('claude');
  const [request, setRequest] = useState('');
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState('');
  const [result, setResult] = useState<{ answer: Answer; done: string[]; skipped: string[] } | null>(null);
  const [exact, setExact] = useState<[string, string] | null>(null);
  const ticket = useRef(0);
  const box = useRef<HTMLElement>(null);
  // Beside the canvas, so the suggestion and the screen are seen together.
  useEffect(() => { box.current?.scrollIntoView?.({ block: 'nearest', behavior: 'smooth' }); }, []);
  useEffect(() => { invoke<Status>('ai_status').then(s => { setStatus(s); const first = choices(s)[0]; if (first) setProvider(first[0]); }).catch(e => setError(String(e))); }, []);
  useEffect(() => () => { ticket.current++; }, []);
  const reviewing = result !== null && result.done.length > 0;
  useEffect(() => { onReview(reviewing); }, [reviewing]);

  const options = status ? choices(status) : [];
  const connected = options.length > 0;
  const layers = scene.nodes.length;
  const sent = `Sent: this screen's ${layers} layer${layers === 1 ? '' : 's'} (positions, sizes, colours and text) and what you type. Not sent: pictures, file names, your games or your settings.`;

  const ask = async () => {
    const mine = ++ticket.current;
    setAsking(true); setError(''); setResult(null);
    try {
      const answer = await invoke<Answer>('ai_suggest_design', { provider, screen: describeScreen(scene), request });
      if (mine !== ticket.current) return; // stopped waiting
      const applied = applyEdits(session.document, scene.id, answer.edits);
      if (applied.done.length) session.edit(applied.project, AI_LABEL);
      session.endEditGroup();
      setResult({ answer, done: applied.done, skipped: applied.skipped });
    } catch (e) { if (mine === ticket.current) setError(String(e)); }
    finally { if (mine === ticket.current) setAsking(false); }
  };
  const keep = () => { setResult(null); setRequest(''); };
  const discard = () => { if (session.history.undoLabel === AI_LABEL) session.undo(); setResult(null); };

  return <section ref={box} className="ts-ask-ai" aria-label="Ask AI">
    <h3>Ask AI for ideas</h3>
    {!status && !error && <p>Checking your AI helpers…</p>}
    {status && !connected && <p>Connect Claude, another AI service or your own private AI in <b>Settings → AI helpers</b> first. Nothing is sent anywhere until you do.</p>}
    {connected && !reviewing && <>
      {options.length > 1 && <label>Ask <select value={provider} disabled={asking} onChange={e => setProvider(e.target.value)}>{options.map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select></label>}
      <div className="ts-ai-ideas">{IDEAS.map(idea => <button key={idea} type="button" disabled={asking} onClick={() => setRequest(idea)}>{idea}</button>)}</div>
      <textarea aria-label="What would you like?" maxLength={1000} rows={3} disabled={asking} placeholder={`What would you like for “${scene.name}”? For example: dark blue with gold text, and a bigger title.`} value={request} onChange={e => setRequest(e.target.value)}/>
      <p className="ts-field-help">{sent}{showExactly && <> <button type="button" className="link" onClick={() => { if (exact) setExact(null); else invoke<[string, string]>('ai_design_preview', { screen: describeScreen(scene), request }).then(setExact).catch(e => setError(String(e))); }}>{exact ? 'Hide' : 'Show exactly'}</button></>}</p>
      {exact && <pre className="ts-ai-exact">{`Instructions:\n${exact[0]}\n\n${exact[1]}`}</pre>}
      <div className="actions">
        <button className="primary" disabled={asking || !request.trim()} onClick={() => { void ask(); }}>{asking ? 'Thinking… (up to a minute)' : 'Ask'}</button>
        {asking ? <button className="ghost" onClick={() => { ticket.current++; setAsking(false); }}>Stop waiting</button> : <button className="ghost" onClick={onClose}>Close</button>}
      </div>
      {asking && <p className="ts-field-help">If you stop waiting, the answer is ignored; the request may still count toward your account.</p>}
    </>}
    {error && <p className="notice warn" role="alert">{error}</p>}
    {result && <div role="status">
      <p><b>{result.answer.explanation}</b></p>
      {reviewing ? <><p className="ts-field-help">Shown on the screen now. Keep it, or Discard to put everything back.</p><div className="actions"><button className="primary" onClick={keep}>Keep</button><button className="ghost" onClick={discard}>Discard</button></div></>
        : <><p>No changes could be used this time. Try asking another way.</p><div className="actions"><button className="ghost" onClick={() => setResult(null)}>Ask again</button></div></>}
      {result.done.length > 0 && <details><summary>What changed ({result.done.length})</summary><ul className="ts-ai-done">{result.done.map((d, i) => <li key={i}>{d}</li>)}</ul></details>}
      {result.skipped.length > 0 && <details><summary>{result.skipped.length} suggestion{result.skipped.length === 1 ? '' : 's'} left out</summary><ul>{result.skipped.map((s, i) => <li key={i}>{s}</li>)}</ul></details>}
      <p className="ts-field-help">Answered by {result.answer.answeredBy}{result.answer.costUsd != null && <> · about {result.answer.costUsd < 0.01 ? 'less than 1¢' : `$${result.answer.costUsd.toFixed(2)}`}</>} · {result.answer.requestsLeft} requests left today</p>
    </div>}
  </section>;
}
