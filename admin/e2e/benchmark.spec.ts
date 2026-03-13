import { test, expect } from '@playwright/test';
import {
  collectErrors,
  collectWarnings,
  expectNoConsoleErrors,
  expectNoHydrationWarnings,
  expectNoLayoutOverflow,
} from './helpers';

// --- SSR Correctness ---

test('SSR renders static shell with island containers (JS disabled)', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto('/platform/benchmark');

  // Static shell elements are SSR-rendered
  await expect(page.locator('.brand-header')).toBeVisible();
  await expect(page.locator('.benchmark-layout')).toBeVisible();
  await expect(page.locator('.page-footer')).toBeVisible();

  // Island containers exist with correct attributes (3 main + sub-islands)
  const islands = await page.locator('[data-forma-island]').count();
  expect(islands).toBeGreaterThanOrEqual(3);

  // Named islands: FilterBar, PerfPanel, BenchmarkDataTable
  const names = await page.locator('[data-forma-component]').evaluateAll(
    els => els.map(e => e.getAttribute('data-forma-component'))
  );
  expect(names).toContain('FilterBar');
  expect(names).toContain('PerfPanel');
  expect(names).toContain('BenchmarkDataTable');

  await context.close();
});

// --- Hydration Integrity ---

test('initial hydration: no console errors', async ({ page }) => {
  const errors = collectErrors(page);
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(2000);
  await expectNoConsoleErrors(errors);
});

test('initial hydration: no FormaJS warnings for registered islands', async ({ page }) => {
  const warnings = collectWarnings(page);
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(2000);
  // Sub-islands (PerfStat, island_1) emit expected warnings since they are
  // inline helpers not registered in activateIslands. Filter to only check
  // warnings for the 3 registered islands.
  const registeredWarnings = warnings.filter(w =>
    !w.includes('PerfStat') && !w.includes('island_1')
  );
  await expectNoHydrationWarnings(registeredWarnings);
});

test('initial hydration: 1000 rows visible after JS loads', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(1000);
  const rows = await page.locator('tbody tr').count();
  expect(rows).toBe(1000);
});

// --- Island activation ---

test('three islands reach status="active"', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(2000);
  const active = await page.locator('[data-forma-status="active"]').count();
  expect(active).toBeGreaterThanOrEqual(3);
  const pending = await page.locator('[data-forma-status="pending"]').count();
  expect(pending).toBe(0);
});

// --- FilterBar interactions (client-side, no server fetch) ---

test('search input filters table rows client-side', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(500);

  await page.fill('#bench-search', 'admin');
  // Client-side filter — no server fetch needed
  await page.waitForTimeout(300);

  const rows = await page.locator('tbody tr').count();
  expect(rows).toBeGreaterThan(0);
  expect(rows).toBeLessThan(1000);
});

test('sort select reorders table client-side', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(500);

  // Get first name before sort
  const firstBefore = await page.locator('tbody tr:first-child td:nth-child(2)').textContent();

  await page.selectOption('#bench-sort', 'name');
  // Client-side sort — instant, no server fetch
  await page.waitForTimeout(200);

  const firstAfter = await page.locator('tbody tr:first-child td:nth-child(2)').textContent();
  // After sorting by name, first row should change
  expect(firstAfter).not.toBe(firstBefore);
});

test('direction toggle reverses order client-side', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(500);

  const firstIdBefore = await page.locator('tbody tr:first-child td:first-child').textContent();

  await page.locator('button:has-text("ASC")').click();
  // Client-side direction toggle — instant, no server fetch
  await page.waitForTimeout(200);

  const firstIdAfter = await page.locator('tbody tr:first-child td:first-child').textContent();
  expect(firstIdAfter).not.toBe(firstIdBefore);
});

test('row count decrease is client-side (no fetch)', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(500);

  // 1000 → 100: we already have 1000 rows, no fetch needed
  await page.selectOption('#bench-rows', '100');
  await page.waitForTimeout(300);

  const rows = await page.locator('tbody tr').count();
  expect(rows).toBe(100);
});

test('row count increase fetches from server', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(500);

  // 1000 → 5000: need more data, triggers fetch
  const [resp] = await Promise.all([
    page.waitForResponse(r => r.url().includes('/api/benchmark')),
    page.selectOption('#bench-rows', '5000'),
  ]);
  await page.waitForTimeout(1000);

  const rows = await page.locator('tbody tr').count();
  expect(rows).toBe(5000);
});

// --- DataTable interactions ---

test('click column header sorts client-side', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(500);

  const firstBefore = await page.locator('tbody tr:first-child td:nth-child(2)').textContent();

  await page.locator('th:has-text("Name")').click();
  // No server fetch — client-side sort
  await page.waitForTimeout(200);

  const firstAfter = await page.locator('tbody tr:first-child td:nth-child(2)').textContent();
  expect(firstAfter).not.toBe(firstBefore);
});

// --- PerfPanel ---

test('perf panel shows row count', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(1000);
  const perfText = await page.evaluate(() => {
    const stats = document.querySelectorAll('.perf-stat-value');
    return Array.from(stats).map(s => s.textContent);
  });
  // Should contain "1000" or "1,000" somewhere
  expect(perfText.some(t => t?.includes('1000') || t?.includes('1,000'))).toBe(true);
});

// --- Layout ---

test('no layout overflow', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(1000);
  await expectNoLayoutOverflow(page);
});

test('no DOM doubling after hydration', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(2000);

  // Each key element appears exactly once (no duplication)
  const filterBars = await page.locator('.benchmark-filter-bar').count();
  const dataTables = await page.locator('.benchmark-data-table').count();
  const perfPanels = await page.locator('.benchmark-perf-panel').count();
  expect(filterBars).toBe(1);
  expect(dataTables).toBe(1);
  expect(perfPanels).toBe(1);

  // Each island container appears exactly once (3 main + sub-islands)
  const islands = await page.locator('[data-forma-island]').count();
  expect(islands).toBeGreaterThanOrEqual(3);
});
