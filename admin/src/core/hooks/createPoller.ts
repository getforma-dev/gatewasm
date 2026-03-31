import { createSignal } from 'formajs';

export interface PollerOptions {
  /** Initial interval in ms */
  initialMs: number;
  /** Interval after backoff kicks in */
  backoffMs: number;
  /** Time in ms before switching to backoff interval */
  backoffAfterMs: number;
  /** Polling function — return true to stop polling */
  fn: () => Promise<boolean>;
}

export function createPoller(opts: PollerOptions) {
  const [active, setActive] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | null = null;
  let startTime = 0;

  function currentInterval(): number {
    const elapsed = Date.now() - startTime;
    return elapsed > opts.backoffAfterMs ? opts.backoffMs : opts.initialMs;
  }

  async function tick() {
    if (!active()) return;
    try {
      const done = await opts.fn();
      if (done) {
        stop();
        return;
      }
    } catch {
      // Swallow errors — keep polling
    }
    if (active()) {
      timer = setTimeout(tick, currentInterval());
    }
  }

  function start() {
    if (active()) return;
    startTime = Date.now();
    setActive(true);
    tick();
  }

  function stop() {
    setActive(false);
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  return { start, stop, active };
}
