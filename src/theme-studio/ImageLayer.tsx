import { useId } from 'react';
import type { ThemeNode } from './types';
import usePngDimensions from './use-png-dimensions';
import { imagePresentation, cropPixels } from './image-presentation';
export default function ImageLayer({ node, url, width, height, opacity }: {
    node: ThemeNode;
    url: string;
    width: number;
    height: number;
    opacity: number;
}) {
    const dimensions = usePngDimensions(url);
    const clipId = `crop-${useId().replace(/:/g, '')}`;
    if (!dimensions)
        return <text x={4} y={20} fill="#abc0d9" fontSize={18} pointerEvents="none">Loading image?</text>;
    if (dimensions.failed)
        return <text x={4} y={20} fill="#abc0d9" fontSize={18} pointerEvents="none">Image decode unavailable</text>;
    const state = imagePresentation(node);
    const crop = cropPixels(state.crop, dimensions.width, dimensions.height);
    return <svg data-image-viewport="true" width={width} height={height} viewBox={`${crop.x} ${crop.y} ${crop.width} ${crop.height}`} preserveAspectRatio={state.fit === 'cover' ? 'xMidYMid slice' : 'xMidYMid meet'} overflow="hidden" opacity={opacity} pointerEvents="none">
    <defs><clipPath id={clipId} clipPathUnits="userSpaceOnUse"><rect x={crop.x} y={crop.y} width={crop.width} height={crop.height}/></clipPath></defs>
    <image href={url} width={dimensions.width} height={dimensions.height} preserveAspectRatio="none" clipPath={`url(#${clipId})`}/>
  </svg>;
}
