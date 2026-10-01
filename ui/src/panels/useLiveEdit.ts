import { useEffect, useRef, useState } from "react";
import { reportFailure } from "../errors";

/** Keep rapid edits together while serialising writes. Intermediate server
 * echoes cannot replace a newer draft, and only the latest queued value is sent. */
export function useLiveEdit<T>(value: T, save: (next: T) => Promise<unknown>, resetKey?: unknown) {
  const [draft, setDraft] = useState(value);
  const current = useRef(value);
  const confirmed = useRef(value);
  confirmed.current = value;
  const busy = useRef(false);
  const writer = useRef(save);
  writer.current = save;
  const waiters = useRef<(() => void)[]>([]);
  useEffect(() => {
    if (!busy.current) { current.current = value; setDraft(value); }
  }, [value, resetKey]);
  const change = (update: (previous: T) => T): Promise<void> => {
    current.current = update(current.current);
    setDraft(current.current);
    const done = new Promise<void>((resolve) => waiters.current.push(resolve));
    if (!busy.current) {
      busy.current = true;
      void (async () => {
        let next: T;
        do {
          next = current.current;
          try { await writer.current(next); }
          catch (error) {
            reportFailure(error);
            if (current.current === next) {
              current.current = confirmed.current;
              setDraft(confirmed.current);
              break;
            }
          }
        } while (current.current !== next);
        busy.current = false;
        for (const resolve of waiters.current.splice(0)) resolve();
      })();
    }
    return done;
  };
  return [draft, change] as const;
}
