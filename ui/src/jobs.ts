/**
 * The environment fetch queue as Rust last reported it (spec.md 7.7), for
 * the status bar and the track list.
 *
 * A store outside React like `busy.ts`: the shell subscribes to Rust's
 * `env://progress` events and publishes each status here; any component
 * reads it. Nothing here starts or stops a job — that is `api`.
 */
import { useSyncExternalStore } from "react";

import type { EnvJobTrack } from "./generated/EnvJobTrack";
import type { EnvJobsStatus } from "./generated/EnvJobsStatus";

const EMPTY: EnvJobsStatus = { tracks: [], failure: null, warning: null };

let snapshot: EnvJobsStatus = EMPTY;
const listeners = new Set<() => void>();

/** Records the queue as Rust reported it. */
export function setEnvJobs(status: EnvJobsStatus): void {
  snapshot = status;
  for (const listener of listeners) listener();
}

/** The queue now. */
export function currentEnvJobs(): EnvJobsStatus {
  return snapshot;
}

/** Whether a fetch is running or waiting. */
export function envJobsBusy(status: EnvJobsStatus = snapshot): boolean {
  return status.tracks.length > 0;
}

/** The job of one track source, if it has one. */
export function envJobOf(status: EnvJobsStatus, sourceId: number): EnvJobTrack | undefined {
  return status.tracks.find((job) => job.source_id === sourceId);
}

export function useEnvJobs(): EnvJobsStatus {
  return useSyncExternalStore(subscribe, () => snapshot);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** For tests. */
export function resetEnvJobs(): void {
  setEnvJobs(EMPTY);
}
