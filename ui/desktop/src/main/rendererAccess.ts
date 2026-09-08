/**
 * Owns renderer directory grants, artifact grants, and artifact-routing validation.
 *
 * Extracted from ui/desktop/src/main.ts during behavior-preserving modularization.
 * The current Electron entrypoint retains its own authorization wiring in main.ts.
 * Changes to this controller alone do not change the running Desktop's file access.
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import type { ArtifactRoutingConfig } from '../types/artifactRouter';
import {
  assertArtifactFileAccess,
  resolveArtifactFileCapability,
} from '../utils/artifactFileAccess';
import { ArtifactRoutingRegistry } from '../utils/artifactRoutingRegistry';
import { loadRecentDirs } from '../utils/recentDirs';
import { RendererDirectoryGrantRegistry } from '../utils/rendererDirectoryGrants';
import { assertPathWithinRoots, canonicalizePotentialPath } from '../utils/rendererFileAccess';
import { expandTilde } from '../utils/pathUtils';

const ARTIFACT_PRODUCT_TYPES = new Set([
  'code',
  'data',
  'document',
  'export',
  'image',
  'other',
  'presentation',
  'spreadsheet',
  'video',
]);

export function resolveRendererPath(filePath: string): string {
  return path.resolve(expandTilde(filePath));
}

export class RendererAccessController {
  private readonly directoryGrants: RendererDirectoryGrantRegistry;
  private readonly artifactFileGrants = new Map<number, Set<string>>();
  private readonly artifactRouting = new ArtifactRoutingRegistry();

  constructor(directoryGrantsFile: string) {
    this.directoryGrants = new RendererDirectoryGrantRegistry(directoryGrantsFile);
    try {
      this.directoryGrants.load();
    } catch (error) {
      console.error('Failed to load renderer directory grants; starting with no grants:', error);
    }
  }

  firstGrantedRecentDirectory(webContentsId = 0): string | undefined {
    return loadRecentDirs().find((dir) =>
      this.directoryGrants.isGrantedDirectory(webContentsId, dir)
    );
  }

  isGrantedDirectory(webContentsId: number, directory: string): boolean {
    return this.directoryGrants.isGrantedDirectory(webContentsId, directory);
  }

  grantSelectedPath(webContentsId: number, selectedPath: string, persist = true): void {
    this.directoryGrants.grantSelectedPath(webContentsId, selectedPath, persist);
  }

  async grantArtifactFile(webContentsId: number, filePath: string): Promise<string> {
    const selectedPath = await canonicalizePotentialPath(resolveRendererPath(filePath));
    const grants = this.artifactFileGrants.get(webContentsId) ?? new Set<string>();
    grants.add(selectedPath);
    this.artifactFileGrants.set(webContentsId, grants);
    return selectedPath;
  }

  clearWebContents(webContentsId: number): void {
    this.artifactRouting.clear(webContentsId);
    this.artifactFileGrants.delete(webContentsId);
    this.directoryGrants.clearTransient(webContentsId);
  }

  getArtifactRouting(webContentsId: number): ArtifactRoutingConfig | undefined {
    return this.artifactRouting.get(webContentsId);
  }

  async updateArtifactRouting(
    webContentsId: number,
    config: ArtifactRoutingConfig | null
  ): Promise<boolean> {
    return this.artifactRouting.update(webContentsId, config, (candidate) =>
      this.validateArtifactRoutingConfig(webContentsId, candidate)
    );
  }

  async assertFileAccess(webContentsId: number, filePath: string): Promise<string> {
    const resolvedPath = resolveRendererPath(filePath);
    return assertPathWithinRoots(resolvedPath, this.directoryGrants.rootsFor(webContentsId));
  }

  async assertArtifactFileAccess(
    webContentsId: number,
    filePath: string,
    baseDirectory?: string
  ): Promise<string> {
    const routingConfig = this.artifactRouting.get(webContentsId);
    const routedOutputRoots = routingConfig?.outputs.map((output) => output.path) ?? [];
    const routedArtifactFiles = routingConfig?.artifactFiles ?? [];
    const expandedPath = expandTilde(filePath);
    const candidatePath = path.isAbsolute(expandedPath) ? resolveRendererPath(filePath) : filePath;
    return assertArtifactFileAccess(
      candidatePath,
      baseDirectory ? resolveRendererPath(baseDirectory) : undefined,
      this.directoryGrants.rootsFor(webContentsId),
      routedOutputRoots,
      new Set([...(this.artifactFileGrants.get(webContentsId) ?? []), ...routedArtifactFiles])
    );
  }

  private async assertArtifactOutputRootAccess(
    webContentsId: number,
    outputPath: string
  ): Promise<string> {
    return this.assertFileAccess(webContentsId, outputPath);
  }

  private async validateArtifactRoutingConfig(
    webContentsId: number,
    config: ArtifactRoutingConfig
  ): Promise<ArtifactRoutingConfig | null> {
    if (
      (config.workspaceId !== undefined && typeof config.workspaceId !== 'string') ||
      (config.workspaceName !== undefined && typeof config.workspaceName !== 'string') ||
      (config.workspaceId === undefined) !== (config.workspaceName === undefined) ||
      !Array.isArray(config.outputs) ||
      config.outputs.length > 64 ||
      (config.artifactFiles !== undefined &&
        (!Array.isArray(config.artifactFiles) || config.artifactFiles.length > 256))
    ) {
      return null;
    }

    const outputs = [];
    for (const output of config.outputs) {
      if (
        typeof output.id !== 'string' ||
        typeof output.path !== 'string' ||
        typeof output.isDefault !== 'boolean' ||
        !Array.isArray(output.productTypes) ||
        output.productTypes.length === 0 ||
        !output.productTypes.every((productType) => ARTIFACT_PRODUCT_TYPES.has(productType))
      ) {
        continue;
      }
      try {
        const outputPath = await this.assertArtifactOutputRootAccess(webContentsId, output.path);
        const stats = await fs.stat(outputPath);
        if (stats.isDirectory()) outputs.push({ ...output, path: outputPath });
      } catch {
        continue;
      }
    }

    const artifactFiles = [];
    for (const artifactFile of config.artifactFiles ?? []) {
      if (
        typeof artifactFile !== 'string' ||
        artifactFile.length === 0 ||
        artifactFile.length > 4096
      ) {
        continue;
      }
      try {
        const artifactPath = await resolveArtifactFileCapability(resolveRendererPath(artifactFile));
        if (artifactPath) artifactFiles.push(artifactPath);
      } catch {
        continue;
      }
    }

    return outputs.length > 0 || artifactFiles.length > 0
      ? { ...config, artifactFiles: [...new Set(artifactFiles)], outputs }
      : null;
  }
}
