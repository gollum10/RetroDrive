import { useEffect, useState } from 'react';
export default function NumberField({ label, value, min, max, onChange, onEnd }: {
    label: string;
    value: number;
    min: number;
    max: number;
    onChange: (value: number) => void;
    onEnd: () => void;
}) {
    const [text, setText] = useState(String(value));
    useEffect(() => { setText(String(value)); }, [value]);
    const valid = text.trim() !== '' && Number.isFinite(Number(text)) && Number(text) >= min && Number(text) <= max;
    return <label>{label}<input type="number" step="any" min={min} max={max} value={text} aria-invalid={!valid} onChange={event => { setText(event.target.value); const n = event.target.valueAsNumber; if (Number.isFinite(n) && n >= min && n <= max)
        onChange(n); }} onBlur={() => { setText(String(value)); onEnd(); }}/></label>;
}
