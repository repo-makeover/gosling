export const ARTIFACT_TIMESTAMPS_BATCH_LIMIT = 200;
export const ARTIFACT_TIMESTAMPS_REFRESH_EVENT = 'artifact-file-timestamps-refresh';

export interface ArtifactFileTimestamps {
  createdAt: string | null;
  modifiedAt: string;
}

export type ArtifactFileTimestampMap = Record<string, ArtifactFileTimestamps | null>;
