import { useEffect, useState } from "react";

import type { AppInfo } from "./generated/AppInfo";
import { api } from "./ipc";

/**
 * Placeholder start screen (plan.md M0): the product name and version, read
 * from the Rust side so the IPC round trip is exercised from the first build.
 * The real start screen arrives with M2.
 *
 * No interface text of its own yet: the name and version come from Rust, and
 * an error shows the backend's message. Translated strings arrive with i18n
 * in M2.
 */
export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.appInfo().then(setInfo, (err: unknown) => setError(err instanceof Error ? err.message : String(err)));
  }, []);

  return (
    <main className="start">
      {info !== null && (
        <>
          <h1>{info.name}</h1>
          <p className="version">{info.version}</p>
        </>
      )}
      {error !== null && <p role="alert">{error}</p>}
    </main>
  );
}
