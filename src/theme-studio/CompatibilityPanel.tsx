import { useEffect, useMemo, useState } from 'react';
import { compatibility, targetProfiles, type CompatibilityReport, type ThemeTargetProfile } from './api';
import type { ThemeProject } from './types';
import { errorText } from './document';

export default function CompatibilityPanel({ document }: { document: ThemeProject }) {
  const [targets, setTargets] = useState<ThemeTargetProfile[]>([]);
  const [selected, setSelected] = useState('');
  const [report, setReport] = useState<CompatibilityReport | null>(null);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  const json = useMemo(() => JSON.stringify(document), [document]);
  useEffect(() => {
    let cancelled = false;
    void targetProfiles().then(rows => { if (!cancelled) setTargets(rows); }).catch(e => { if (!cancelled) setError(errorText(e)); });
    return () => { cancelled = true; };
  }, [retry]);
  useEffect(() => {
    let cancelled = false; setReport(null); setError('');
    const timer = window.setTimeout(() => {
      void compatibility(json, selected || null).then(value => { if (!cancelled) setReport(value); }).catch(e => { if (!cancelled) setError(errorText(e)); });
    }, 300);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [json, selected, retry]);
  return <section aria-label="System check"><h3>Will it work on my system?</h3><label>Check against<select value={selected} onChange={e => { setReport(null); setSelected(e.target.value); }}><option value="">The system it's made for</option>{targets.map(target => <option key={target.installationId} value={target.installationId ?? ''}>{target.frontendFamily} · {target.applicationVersion ?? 'version unknown'} · {target.observation}</option>)}</select></label>
    {error ? <div role="alert"><p>Couldn't check: {error}</p><button onClick={() => setRetry(n => n + 1)}>Check again</button></div> : !report ? <p role="status">Checking…</p> : <>
      <p>{report.profile.applicationVersion ? `Found version ${report.profile.applicationVersion}. ` : ''}Saving themes for systems is coming soon; until then this is a preview of what may need attention.</p>
      <p>{report.issues.length + report.omittedIssues} things checked.</p>
      <details><summary>Show details</summary><ul>{report.issues.map(issue => <li key={issue.id}><strong>{issue.feature}: {issue.outcome}</strong>{issue.nodeId && <span> · layer {issue.nodeId}</span>}<p>{issue.reason}</p>{issue.fallback && <p>Instead: {issue.fallback}</p>}<small>{issue.userAction}</small></li>)}</ul>{report.omittedIssues > 0 && <p>{report.omittedIssues} more not shown.</p>}</details>
    </>}
  </section>;
}
