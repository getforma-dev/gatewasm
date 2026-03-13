import { createSignal } from 'formajs';
import type { PageState } from '../types';

export function createPager(initialLimit = 50) {
  const [page, setPage] = createSignal<PageState>({ limit: initialLimit, offset: 0 });

  function next(total: number): void {
    const p = page();
    if (p.offset + p.limit < total) {
      setPage({ ...p, offset: p.offset + p.limit });
    }
  }

  function prev(): void {
    const p = page();
    if (p.offset > 0) {
      setPage({ ...p, offset: Math.max(0, p.offset - p.limit) });
    }
  }

  function reset(): void {
    setPage((p: PageState) => ({ ...p, offset: 0 }));
  }

  function navigate(dir: 'prev' | 'next', total: number): void {
    if (dir === 'next') next(total);
    else prev();
  }

  return { page, setPage, next, prev, reset, navigate };
}
