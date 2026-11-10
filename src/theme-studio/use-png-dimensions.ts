import { useEffect, useState } from 'react';
import { safePngPreview } from './use-image-previews';
export default function usePngDimensions(url: string | undefined) {
    const [loaded, setLoaded] = useState<{
        url: string;
        width: number;
        height: number;
        failed: boolean;
    } | null>(null);
    useEffect(() => {
        if (!safePngPreview(url))
            return;
        let cancelled = false;
        const image = new Image();
        image.onload = () => { if (cancelled)
            return; const valid = image.naturalWidth > 0 && image.naturalHeight > 0 && image.naturalWidth <= 4096 && image.naturalHeight <= 4096 && image.naturalWidth * image.naturalHeight <= 4194304; setLoaded({ url, width: valid ? image.naturalWidth : 0, height: valid ? image.naturalHeight : 0, failed: !valid }); };
        image.onerror = () => { if (!cancelled)
            setLoaded({ url, width: 0, height: 0, failed: true }); };
        image.src = url;
        return () => { cancelled = true; image.onload = null; image.onerror = null; };
    }, [url]);
    return loaded?.url === url ? loaded : null;
}
