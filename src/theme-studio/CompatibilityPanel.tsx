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
  return <section aria-label="Target compatibility"><h3>Compatibility</h3><label>Check target<select value={selected} onChange={e => { setReport(null); setSelected(e.target.value); }}><option value="">Project intent / no verified target</option>{targets.map(target => <option key={target.installationId} value={target.installationId ?? ''}>{target.frontendFamily} · {target.applicationVersion ?? 'release unknown'} · {target.observation}</option>)}</select></label>
    {error ? <div role="alert"><p>Compatibility could not be checked: {error}</p><button onClick={() => setRetry(n => n + 1)}>Retry compatibility check</button></div> : !report ? <p role="status">Native support is unverified. Checking profile…</p> : <>
      <p>Native support is unverified. {report.profile.applicationVersion ? `Detected application release: ${report.profile.applicationVersion}. ` : ''}A verified frontend engine/version profile is still required.</p>
      <p>{report.issues.length + report.omittedIssues} feature checks. Export and installation are not enabled.</p>
      <details><summary>Feature compatibility details</summary><ul>{report.issues.map(issue => <li key={issue.id}><strong>{issue.feature}: {issue.outcome}</strong>{issue.nodeId && <span> · layer {issue.nodeId}</span>}<p>{issue.reason}</p>{issue.fallback && <p>Fallback: {issue.fallback}</p>}<small>{issue.userAction}</small></li>)}</ul>{report.omittedIssues > 0 && <p>{report.omittedIssues} additional issues exceed the report display limit. No build is allowed.</p>}</details>
    </>}
  </section>;
}
