import { createSignal } from 'formajs';
import type { Auth } from './createAuth';

export type SocketState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

export interface Socket {
  connect(ticketUrl: string): void;
  disconnect(): void;
  subscribe(topic: string, callback: (hint?: string) => void): () => void;
  state: () => SocketState;
}

const BACKOFF_BASE = 1000;
const BACKOFF_CAP = 30000;
const BACKOFF_FACTOR = 2;

function debounce(fn: (hint?: string) => void, ms: number): (hint?: string) => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  return (hint?: string) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => fn(hint), ms);
  };
}

export function createSocket(auth: Auth): Socket {
  const [state, setState] = createSignal<SocketState>('disconnected');
  const subscribers = new Map<string, Set<(hint?: string) => void>>();
  const debouncedCallbacks = new WeakMap<Function, (hint?: string) => void>();

  let ws: WebSocket | null = null;
  let ticketUrl = '';
  let attempt = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let intentionalClose = false;

  function getWsUrl(base: string): string {
    return base.replace(/^http/, 'ws');
  }

  function allTopics(): string[] {
    return Array.from(subscribers.keys());
  }

  function sendSubscribe(): void {
    const topics = allTopics();
    if (ws && ws.readyState === WebSocket.OPEN && topics.length > 0) {
      ws.send(JSON.stringify({ type: 'subscribe', topics }));
    }
  }

  async function openConnection(): Promise<void> {
    setState('connecting');

    let ticket: string;
    try {
      const res = await fetch(ticketUrl, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(auth.accessToken() ? { Authorization: 'Bearer ' + auth.accessToken() } : {}),
        },
        credentials: 'include',
      });

      if (res.status === 401) {
        try {
          await auth.silentRefresh();
        } catch {
          setState('disconnected');
          return;
        }
        const retry = await fetch(ticketUrl, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            Authorization: 'Bearer ' + auth.accessToken(),
          },
          credentials: 'include',
        });
        if (!retry.ok) {
          scheduleReconnect();
          return;
        }
        const data = await retry.json();
        ticket = data.ticket;
      } else if (!res.ok) {
        scheduleReconnect();
        return;
      } else {
        const data = await res.json();
        ticket = data.ticket;
      }
    } catch {
      scheduleReconnect();
      return;
    }

    const url = new URL(ticketUrl);
    const wsBase = getWsUrl(url.origin);
    ws = new WebSocket(wsBase + '/ws?ticket=' + ticket);

    ws.onopen = () => {
      setState('connected');
      attempt = 0;
      sendSubscribe();
    };

    ws.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data);
        if (msg.type === 'event') {
          const callbacks = subscribers.get(msg.topic);
          if (callbacks) {
            for (const cb of callbacks) {
              const debounced = debouncedCallbacks.get(cb);
              if (debounced) debounced(msg.hint ?? undefined);
            }
          }
        } else if (msg.type === 'auth_expired') {
          if (ws) ws.close();
          auth.silentRefresh()
            .then(() => openConnection())
            .catch(() => setState('disconnected'));
          return;
        }
      } catch {
        // Ignore unparseable messages
      }
    };

    ws.onclose = () => {
      if (!intentionalClose) {
        scheduleReconnect();
      }
    };

    ws.onerror = () => {
      // onerror is always followed by onclose
    };
  }

  function scheduleReconnect(): void {
    setState('reconnecting');
    const delay = Math.min(BACKOFF_BASE * (BACKOFF_FACTOR ** attempt), BACKOFF_CAP);
    const jitter = delay * (0.75 + Math.random() * 0.5);
    attempt++;
    reconnectTimer = setTimeout(() => openConnection(), jitter);
  }

  function connect(url: string): void {
    ticketUrl = url;
    intentionalClose = false;
    attempt = 0;
    openConnection();
  }

  function disconnect(): void {
    intentionalClose = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (ws) {
      ws.close();
      ws = null;
    }
    setState('disconnected');
  }

  function subscribe(topic: string, callback: (hint?: string) => void): () => void {
    if (!subscribers.has(topic)) {
      subscribers.set(topic, new Set());
    }
    const debounced = debounce(callback, 500);
    debouncedCallbacks.set(callback, debounced);
    subscribers.get(topic)!.add(callback);

    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: 'subscribe', topics: [topic] }));
    }

    return () => {
      const set = subscribers.get(topic);
      if (set) {
        set.delete(callback);
        debouncedCallbacks.delete(callback);
        if (set.size === 0) {
          subscribers.delete(topic);
          if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ type: 'unsubscribe', topics: [topic] }));
          }
        }
      }
    };
  }

  return { connect, disconnect, subscribe, state };
}
