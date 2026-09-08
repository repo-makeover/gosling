import { useEffect, useState } from 'react';
import {
  ARTIFACT_TIMESTAMPS_BATCH_LIMIT,
  ARTIFACT_TIMESTAMPS_REFRESH_EVENT,
  type ArtifactFileTimestampMap,
} from '../types/artifactFileTimestamps';

export function useArtifactFileTimestamps(
  files: Array<{ path: string; timestampRevision?: string }>
): ArtifactFileTimestampMap {
  const requestKey = JSON.stringify(files.map((file) => [file.path, file.timestampRevision]));
  const [snapshot, setSnapshot] = useState<{
    requestKey: string;
    timestamps: ArtifactFileTimestampMap;
  } | null>(null);

  useEffect(() => {
    const requests: Array<[string, string | null]> = JSON.parse(requestKey);
    const paths = [...new Set(requests.map(([filePath]) => filePath))];
    let cancelled = false;
    let revision = 0;

    const refresh = async () => {
      const currentRevision = ++revision;
      const timestamps: ArtifactFileTimestampMap = {};
      for (let offset = 0; offset < paths.length; offset += ARTIFACT_TIMESTAMPS_BATCH_LIMIT) {
        const batch = paths.slice(offset, offset + ARTIFACT_TIMESTAMPS_BATCH_LIMIT);
        try {
          const result = await window.electron.getArtifactFileTimestamps(batch);
          for (const filePath of batch) timestamps[filePath] = result[filePath] ?? null;
        } catch {
          for (const filePath of batch) timestamps[filePath] = null;
        }
        if (cancelled || currentRevision !== revision) return;
      }
      if (!cancelled && currentRevision === revision) setSnapshot({ requestKey, timestamps });
    };

    const onFocus = () => void refresh();
    void refresh();
    window.addEventListener('focus', onFocus);
    window.addEventListener(ARTIFACT_TIMESTAMPS_REFRESH_EVENT, onFocus);
    return () => {
      cancelled = true;
      window.removeEventListener('focus', onFocus);
      window.removeEventListener(ARTIFACT_TIMESTAMPS_REFRESH_EVENT, onFocus);
    };
  }, [requestKey]);

  // A late response for another list must never supply this list's timestamps.
  return snapshot?.requestKey === requestKey ? snapshot.timestamps : {};
}
