import { useEffect, useRef, useState } from 'react';
import type { Asset } from './types';
import { previewPng } from './api';
export function safePngPreview(value: unknown): value is string { return typeof value === 'string' && value.length <= 28 * 1024 * 1024 && /^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(value); }
export default function useImagePreviews(projectId: string, assets: Asset[], revision: number) {
    const owner = useRef(projectId);
    const cache = useRef(new Map<string, string>());
    const [, setImages] = useState<Record<string, string>>({});
    const [error, setError] = useState('');
    const [attempt, setAttempt] = useState(0);
    const key = assets.map(asset => `${asset.id}:${asset.sha256}:${asset.bytes}`).join('|');
    useEffect(() => {
        if (owner.current !== projectId) {
            cache.current.clear();
            owner.current = projectId;
        }
        setImages({});
        let cancelled = false;
        const keep = new Set<string>();
        const next: Record<string, string> = {};
        let budget = 32 * 1024 * 1024;
        const selected: Asset[] = [];
        for (const asset of assets) {
            if (selected.length >= 8 || asset.bytes > budget || asset.mediaType !== 'image/png')
                continue;
            budget -= asset.bytes;
            selected.push(asset);
            keep.add(asset.id + ':' + asset.sha256);
        }
        for (const id of cache.current.keys())
            if (!keep.has(id))
                cache.current.delete(id);
        setError(selected.length < assets.length ? 'Preview budget: at most 8 PNGs and 32 MiB per scene.' : '');
        void (async () => { for (const asset of selected) {
            if (cancelled)
                return;
            try {
                const id = asset.id + ':' + asset.sha256;
                let url = cache.current.get(id);
                if (!url) {
                    url = await previewPng(projectId, asset.id);
                    if (!safePngPreview(url))
                        throw new Error('Invalid image preview');
                    if (cancelled)
                        return;
                    cache.current.set(id, url);
                }
                next[asset.id] = url;
            }
            catch {
                if (!cancelled)
                    setError('Some images are unavailable. Wait for autosave, then retry previews.');
            }
        } if (!cancelled)
            setImages({ ...next }); })();
        return () => { cancelled = true; };
        // The bounded declaration key represents assets; revision retries initially unsaved declarations.
    }, [projectId, key, revision, attempt]);
    return { images: owner.current === projectId ? Object.fromEntries(assets.filter(asset => cache.current.has(asset.id + ':' + asset.sha256)).map(asset => [asset.id, cache.current.get(asset.id + ':' + asset.sha256)!])) : {}, error, retry: () => setAttempt(value => value + 1) };
}
