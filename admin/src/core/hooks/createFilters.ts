import { createSignal } from 'formajs';

export function createFilters<T extends Record<string, string>>(
  storageKey: string,
  defaults: T,
) {
  const [filters, setFilters] = createSignal<T>({ ...defaults });

  function update(partial: Partial<T>): void {
    setFilters((prev: T) => ({ ...prev, ...partial }));
  }

  function clear(): void {
    setFilters(() => ({ ...defaults }));
  }

  function save(): void {
    try {
      localStorage.setItem('filters:' + storageKey, JSON.stringify(filters()));
    } catch {}
  }

  function load(): T {
    try {
      const raw = localStorage.getItem('filters:' + storageKey);
      if (raw) {
        const parsed = JSON.parse(raw);
        const merged = { ...defaults, ...parsed };
        setFilters(() => merged);
        return merged;
      }
    } catch {}
    return filters();
  }

  return { filters, update, clear, save, load };
}
