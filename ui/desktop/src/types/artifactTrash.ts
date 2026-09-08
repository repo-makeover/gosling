export interface ArtifactTrashResult {
  path: string;
  status: 'trashed' | 'missing' | 'failed';
  error?: string;
}

export const ARTIFACT_TRASH_BATCH_LIMIT = 500;
