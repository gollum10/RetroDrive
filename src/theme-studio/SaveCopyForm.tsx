export default function SaveCopyForm({ name, sourceName, busy, onName, onCreate, onCancel }: {
  name: string; sourceName: string; busy: boolean;
  onName: (name: string) => void; onCreate: (name: string) => void; onCancel: () => void;
}) {
  return <form id="ts-save-copy" className="panel ts-copy-form" aria-labelledby="ts-copy-heading" onSubmit={event => { event.preventDefault(); if (!busy && name.trim()) onCreate(name.trim()); }}>
    <h3 id="ts-copy-heading">Save a separate copy</h3>
    <p id="ts-copy-help">You are copying “{sourceName}”. Enter the new copy's name below, then choose Create copy. This does not rename the original project.</p>
    <label>New copy name<input autoFocus required maxLength={120} value={name} aria-describedby="ts-copy-help" onChange={event => onName(event.target.value)} /></label>
    <div className="actions"><button className="primary" type="submit" disabled={busy || !name.trim()}>Create copy</button><button className="ghost" type="button" disabled={busy} onClick={onCancel}>Cancel copy</button></div>
  </form>;
}
