/** Portable source contract v1. Rust owns validation; no installation paths here. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type Extensions = { [key: string]: Json };
// Intersections retain unknown fields without unsafe `any` or dropping payloads.
export type Metadata = Extensions & {
  name: string; author: string; description: string; version: string; license: string; tags: string[];
};
export type TargetIntent = Extensions & { frontendFamily: string; frontendVersion: string | null };
export type Dimension = Extensions & { value: number; unit: 'px' | '%' };
export type Layout = Extensions & {
  x: Dimension; y: Dimension; width: Dimension; height: Dimension; rotation: number;
};
export type Binding = Extensions & { source: string; kind: 'text' | 'number' | 'image' | 'video' };
export type ThemeNode = Extensions & {
  id: string; type: string; name: string; layout: Layout; style: Extensions;
  children: string[]; assets: string[]; bindings: Record<string, Binding>;
};
export type Scene = Extensions & {
  id: string; role: string; name: string; resolution: Extensions & { width: number; height: number };
  nodes: ThemeNode[];
};
export type Asset = Extensions & {
  id: string; path: string; sha256: string; bytes: number; mediaType: string;
  license: string; attribution: string; source: string;
};
export type ThemeProject = Extensions & {
  format: 'retrodrive.theme'; schemaVersion: '1.0.0'; id: string; metadata: Metadata;
  /** [] is the explicit Universal / No Target Yet choice. */
  targets: TargetIntent[]; scenes: Scene[]; assets: Asset[]; tokens: Extensions; variants: Extensions;
};
