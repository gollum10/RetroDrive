import { useEffect, useRef, useState } from 'react';
import type { Asset } from './types';
import { previewPng } from './api';
import { errorText } from './document';

export function safePngPreview(value: unknown): value is string {
    return typeof value === 'string' && value.length <= 28 * 1024 * 1024 && /^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(value);
}
type Entry = { key: string; url: string };
const imageKey = (projectId: string, asset: Asset) => `${projectId}:${asset.id}:${asset.sha256}:${asset.bytes}`;

export default function useImagePreviews(projectId: string, assets: Asset[], revision: number) {
    const cache = useRef(new Map<string, string>());
    // Retain requests across effect cleanup/setup, but never publish cancelled effects.
    const requests = useRef(new Map<string, Promise<string>>());
    const [loaded, setLoaded] = useState<{ projectId: string; entries: Record<string, Entry> }>({ projectId, entries: {} });
    const [error, setError] = useState('');
    const [attempt, setAttempt] = useState(0);
    const key = assets.map(asset => imageKey(projectId, asset)).join('|');
    useEffect(() => {
        let cancelled = false;
        let budget = 32 * 1024 * 1024;
        const selected: Asset[] = [];
        for (const asset of assets) {
            if (selected.length >= 8 || asset.bytes > budget || asset.mediaType !== 'image/png') continue;
            budget -= asset.bytes;
            selected.push(asset);
        }
        const keep = new Set(selected.map(asset => imageKey(projectId, asset)));
        for (const id of cache.current.keys()) if (!keep.has(id)) cache.current.delete(id);
        const next: Record<string, Entry> = {};
        for (const asset of selected) {
            const id = imageKey(projectId, asset);
            const url = cache.current.get(id);
            if (url) next[asset.id] = { key: id, url };
        }
        setLoaded({ projectId, entries: { ...next } });
        setError(selected.length < assets.length ? 'Preview budget: at most 8 PNGs and 32 MiB per scene.' : '');
        const load = (asset: Asset) => {
            const id = imageKey(projectId, asset);
            const existing = requests.current.get(id);
            if (existing) return existing;
            const pending = (async () => {
                for (let retry = 0; ; retry++) {
                    try {
                        const url = await previewPng(projectId, asset.id);
                        if (!safePngPreview(url)) throw new Error('Invalid PNG preview response.');
                        return url;
                    } catch (error) {
                        const busy = typeof error === 'object' && error !== null && 'code' in error && error.code === 'image_busy';
                        if (!busy || retry >= 3) throw error;
                        await new Promise(resolve => setTimeout(resolve, 100 * (retry + 1)));
                    }
                }
            })().finally(() => { if (requests.current.get(id) === pending) requests.current.delete(id); });
            requests.current.set(id, pending);
            return pending;
        };
        void (async () => {
            for (const asset of selected) {
                if (cancelled) return;
                const id = imageKey(projectId, asset);
                try {
                    const url = cache.current.get(id) ?? await load(asset);
                    if (cancelled) return;
                    cache.current.set(id, url);
                    next[asset.id] = { key: id, url };
                    setLoaded({ projectId, entries: { ...next } });
                } catch (error) {
                    if (!cancelled) setError(`Image preview unavailable: ${errorText(error)} Use Retry previews to try again.`);
                }
            }
        })();
        return () => { cancelled = true; };
        // Declaration keys avoid restarting for equivalent arrays; revision retries unsaved declarations.
    }, [projectId, key, revision, attempt]);
    const images: Record<string, string> = {};
    if (loaded.projectId === projectId) for (const asset of assets) {
        const entry = loaded.entries[asset.id];
        if (entry?.key === imageKey(projectId, asset)) images[asset.id] = entry.url;
    }
    return { images, error, retry: () => setAttempt(value => value + 1) };
}
