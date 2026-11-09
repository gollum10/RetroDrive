export type Box = { x: number; y: number; width: number; height: number; rotation: number };
export function bounds(box: Box) {
  const a = box.rotation * Math.PI / 180; const c = Math.cos(a); const s = Math.sin(a);
  const xs = [0, box.width*c, -box.height*s, box.width*c-box.height*s];
  const ys = [0, box.width*s, box.height*c, box.width*s+box.height*c];
  return { left: box.x+Math.min(...xs), right: box.x+Math.max(...xs), top: box.y+Math.min(...ys), bottom: box.y+Math.max(...ys) };
}
export function snapSize(width: number, height: number, spacing: number) {
  return { width: Math.max(1, Math.round(width/spacing)*spacing), height: Math.max(1, Math.round(height/spacing)*spacing) };
}
function adjustment(points: number[], targets: number[], tolerance: number) {
  let delta = Infinity;
  for (const point of points) for (const target of targets) if (Math.abs(target-point) <= tolerance && Math.abs(target-point) < Math.abs(delta)) delta = target-point;
  return Number.isFinite(delta) ? delta : 0;
}
export function snapPosition(box: Box, width: number, height: number, spacing: number, tolerance: number) {
  const grid = { ...box, x: Math.round(box.x/spacing)*spacing, y: Math.round(box.y/spacing)*spacing };
  const b = bounds(grid);
  return { x: grid.x+adjustment([b.left,(b.left+b.right)/2,b.right], [0,width/2,width], tolerance),
    y: grid.y+adjustment([b.top,(b.top+b.bottom)/2,b.bottom], [0,height/2,height], tolerance) };
}
export function guides(box: Box, width: number, height: number, tolerance = 0.01) {
  const b = bounds(box);
  return { x: [0,width/2,width].filter(target => [b.left,(b.left+b.right)/2,b.right].some(value => Math.abs(value-target) <= tolerance)),
    y: [0,height/2,height].filter(target => [b.top,(b.top+b.bottom)/2,b.bottom].some(value => Math.abs(value-target) <= tolerance)) };
}
