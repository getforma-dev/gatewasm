# formajs Reactive Disposal Gap: Branch Roots Outlive Their Parents

**Date:** 2026-04-10
**Severity:** Framework bug
**Affects:** `createShow`, `createSwitch`, `createList`
**Discovered in:** `gatewasm/admin` onboarding wizard, `feat/auth-step-redesign` branch

---

## 1. The Error

```
OnboardingWizard.ts:2016 Uncaught TypeError: Cannot read properties of null (reading 'name')
    at OnboardingWizard.ts:2016:36
    at Object.fn (element.ts:569:18)
    at run (index.mjs:196:15)
    at flush (index.mjs:213:13)
    at endBatch (index.mjs:62:9)
    at batch (batch.ts:33:5)
    at switchToAccountInternal (store.ts:1876:3)
    at backToTenants (store.ts:1931:3)
    at Object.onBackToAccount (ConsoleApp.ts:536:30)
    at HTMLButtonElement.onClick (PlaneNav.ts:142:60)
```

The crash happened when clicking "All Applications" from the workspace shell. A reactive text child `() => selectedTenant()!.name` fired with `selectedTenant === null` even though it was guarded by `createShow(() => selectedTenant() !== null, ...)`.

The guard should have unmounted the text child before the signal changed. It didn't.

---

## 2. The Root Cause

**formajs's reactive primitives (`createShow`, `createSwitch`, `createList`) create inner `createRoot` scopes for their branches, but those inner roots are NOT registered with the parent root. When the parent root is disposed, the inner branch roots are orphaned — their effects stay subscribed to signals and keep firing.**

### 2.1 How the Disposal Chain Should Work

```
Parent createRoot (e.g., ConsoleApp's wizard createShow)
  |
  +-- internalEffect (createShow's condition watcher) ... registered via registerDisposer() OK
  |
  +-- Branch createRoot (created when condition is true)    ... NOT REGISTERED
        |
        +-- OnboardingWizard() render tree
              |
              +-- Inner createShow condition effect         ... registered with Branch root
              +-- Inner Branch createRoot                   ... NOT REGISTERED with Branch root
                    |
                    +-- h("span", {}, () => selectedTenant()!.name)
                          |
                          +-- text internalEffect           ... registered with Inner Branch root
```

When the outer wizard `createShow` swaps (condition true -> false), it calls `currentDispose()` which disposes the Branch createRoot. That disposes its registered children:

- The Inner createShow's condition EFFECT is stopped (it was registered via `internalEffect` -> `registerDisposer`).

But the Inner Branch createRoot (the one that owns the text child effect) was **never registered** with any parent. Its effects survive the disposal and continue reacting to signal changes.

### 2.2 Why createRoot Doesn't Auto-Register

```javascript
// chunk-TBWZZ3SI.js — createRoot
function createRoot(fn) {
  const scope = { disposers: [], scopeDispose: null };
  rootStack.push(currentRoot);   // save parent
  currentRoot = scope;            // become current
  // ...
  currentRoot = rootStack.pop();  // restore parent
  return result;
  // NOTE: scope.dispose is NEVER registered with the parent
}
```

`createRoot` tracks the parent root on a stack (so `registerDisposer` inside the root's body registers with the right scope), but it never registers its OWN dispose with the parent. This is by design — `createRoot` is the "escape hatch" for manual ownership. But `createShow`, `createSwitch`, and `createList` create roots that SHOULD be owned by their parent, and they don't bridge this gap.

### 2.3 The `__showDispose` / `__switchDispose` Methods Exist But Are Dead Code

Both `createShow` and `createSwitch` expose manual cascade-disposal methods on their returned fragments:

```javascript
// createShow
fragment.__showDispose = () => {
  showDispose();               // stop the condition effect
  if (currentDispose) {
    currentDispose();           // dispose the branch root
    currentDispose = null;
  }
};

// createSwitch
fragment.__switchDispose = () => {
  switchDispose();             // stop the match effect
  for (const entry of cache2.values()) {
    entry.dispose();            // dispose ALL cached branch roots
  }
  cache2.clear();
};
```

These methods correctly cascade disposal. But **nothing in the framework calls them.** They're only useful if the caller manually retrieves the fragment and calls `__showDispose()` / `__switchDispose()`, which is undocumented and nobody does.

---

## 3. Affected Primitives

| Primitive | Branch Roots Auto-Disposed? | Manual Cascade? | Orphaned Effects |
|---|---|---|---|
| `createShow` | No | `__showDispose` (1 branch) | Yes — text children, nested createShows, any reactive binding inside the branch |
| `createSwitch` | No | `__switchDispose` (all cached) | Yes — ALL cached branches' effects survive parent disposal |
| `createList` | No | None | Yes — all item render roots are orphaned |

---

## 4. Reproduction

### Minimal reproduction pattern

```typescript
import { createSignal, createShow, createRoot, h, mount } from "formajs";

const [visible, setVisible] = createSignal(true);
const [name, setName] = createSignal<string | null>("Alice");

const app = () =>
  createShow(
    visible,
    () => createShow(
      () => name() !== null,
      () => h("span", null, () => name()!.toUpperCase()),  // <-- will crash
    ),
  );

mount(app, "#app");

// Step 1: Unmount the outer createShow
setVisible(false);
// The outer branch root is disposed. The inner createShow's effect is stopped.
// But the text child's internalEffect is NOT disposed — it stays subscribed to `name`.

// Step 2: Set name to null
setName(null);
// The text child effect fires: name()!.toUpperCase() → TypeError: null.toUpperCase()
```

### How it manifested in gatewasm

1. Operator completes the onboarding wizard. `completeOnboarding()` sets `selectedTenant` to a completed tenant, flipping `appRoute()` from `'wizard'` to `'workspace-shell'`. The wizard's `createShow` swaps out the wizard branch.

2. Operator clicks "All Applications". `backToTenants()` batches `setSelectedTenant(null)` + `setMode('account')`.

3. During the batch flush, the orphaned text child effect `() => selectedTenant()!.name` fires with `null` and crashes. The thrown error interrupts `endBatch` → `flush`, leaving other effects in a stale state, which causes the "click Continue twice" symptom on the next wizard run.

---

## 5. The Workaround (Current)

In `OnboardingWizard.ts`, the reactive text child was made null-safe:

```typescript
// Before (crashes)
() => selectedTenant()!.name

// After (fires harmlessly with null)
() => selectedTenant()?.name ?? ""
```

This prevents the crash but doesn't fix the leak. The orphaned effect still fires on every `selectedTenant` change for the rest of the session.

---

## 6. The Fix: Two Options

### Option A — Minimal: Register Branch Dispose as Effect Cleanup (Surgical)

Change `createShow` and `createSwitch` to use `createEffect` (which supports `onCleanup`) instead of `internalEffect` (which doesn't), and register `currentDispose` as a cleanup:

```javascript
// createShow — patched
function createShow(when, thenFn, elseFn = () => null) {
  // ...
  const showDispose = createEffect(() => {  // was: internalEffect
    onCleanup(() => {
      // When this effect is disposed (parent root torn down),
      // also dispose the current branch root.
      if (currentDispose) {
        currentDispose();
        currentDispose = null;
      }
    });
    const truthy = !!when();
    // ... rest unchanged ...
  });
  // ...
}
```

Same pattern for `createSwitch` (clean up `cache2` in the cleanup) and `createList`.

**Pros:** Surgical fix, only changes the three affected primitives. No changes to `createRoot`.
**Cons:** Each primitive needs its own cleanup registration. New primitives could forget it. Doesn't solve the underlying `createRoot` ownership model.

### Option B — Architectural: Auto-Register createRoot with Parent (Solid-Style)

Make `createRoot` automatically register its dispose function with the parent root (if one exists):

```javascript
function createRoot(fn) {
  const scope = { disposers: [], scopeDispose: null };
  const parentRoot = currentRoot;       // capture parent
  rootStack.push(currentRoot);
  currentRoot = scope;
  const dispose = () => { /* ... existing ... */ };

  // AUTO-REGISTER with parent root (the Solid.js approach)
  if (parentRoot) {
    parentRoot.disposers.push(dispose);
  }

  scope.scopeDispose = effectScope(() => { result = fn(dispose); });
  currentRoot = rootStack.pop() ?? null;
  return result;
}
```

**Pros:** Fixes the problem universally. Any `createRoot` inside another root is automatically owned. `createShow`, `createSwitch`, `createList`, and any future primitive get correct disposal for free. Matches Solid.js's behavior.
**Cons:** Broader change. Code that deliberately uses `createRoot` as an "escape hatch" for manual ownership would need to be audited — a root that auto-registers can't be "moved" to a different owner later. If any code creates a root inside one owner but expects it to outlive that owner, this would break it.

### Recommendation

**Do both.** Option B is the architectural fix — it makes the ownership model correct by default. Option A is defense-in-depth for the specific primitives that also hold closure state (`currentDispose`, `cache2`) that needs explicit cleanup beyond just disposing child effects.

1. Implement Option B in `createRoot` — auto-register with parent.
2. Then implement Option A's cleanup in `createShow`/`createSwitch`/`createList` — the `onCleanup` calls explicitly nullify closure references (`currentDispose`, `cache2`) so GC can reclaim them even if somehow the auto-registration missed.
3. Add a `createRoot.unowned()` escape hatch for code that deliberately needs a root without a parent (e.g., top-level `mount()` calls, test harnesses).

---

## 7. Scope of Audit Needed Before Fix

Before implementing in formajs, audit these call sites for `createRoot` that may intentionally outlive their parent:

- `mount()` — already top-level, no parent. Unaffected.
- `hydrateIsland()` — top-level SSR hydration. Unaffected.
- Any user-land `createRoot` for manual lifecycle management.

The auto-registration should be safe for all framework-internal `createRoot` calls (inside `createShow`, `createSwitch`, `createList`), since those roots are always logically owned by the primitive that created them.

---

## 8. Secondary Issue: `createSwitch` Cache Never Disposes

Even when `createSwitch` swaps branches, it caches old branches without disposing their effects:

```javascript
// index.js:91-103
const matchedCase = cases.find((c) => c.match === val);
if (matchedCase) {
  let entry = cache2.get(val);
  if (!entry) {
    // Fresh render — cache it
    entry = { node, dispose: branchDispose };
    cache2.set(val, entry);
  } else {
    // Reuse cached — NO dispose of old, just reinsert
  }
  currentNode = entry.node;
}
```

Cached branches' effects keep running even when their DOM is detached. This caused the original "wizard flash" bug in gatewasm: `backToTenants()` swapped from the `'workspace'` createSwitch case to `'account'`, but the cached workspace branch's inner `createShow` kept reacting to `selectedTenant` changes and transiently mounted the OnboardingWizard off-DOM.

**This was fixed in gatewasm by replacing `createSwitch` with flat `createShow` bindings at the top level.** But the underlying `createSwitch` caching issue remains in formajs and could affect other consumers.

---

## 9. Files Referenced

| File | Role |
|---|---|
| `admin/node_modules/@getforma/core/dist/chunk-7Q7LIV23.js:581-652` | `createShow` implementation |
| `admin/node_modules/@getforma/core/dist/index.js:50-122` | `createSwitch` implementation |
| `admin/node_modules/@getforma/core/dist/chunk-TBWZZ3SI.js:7-41` | `createRoot`, `registerDisposer` |
| `admin/node_modules/@getforma/core/dist/chunk-TBWZZ3SI.js:47-55` | `onCleanup`, `setCleanupCollector` |
| `admin/node_modules/@getforma/core/dist/chunk-TBWZZ3SI.js:111-117` | `internalEffect` |
| `admin/node_modules/@getforma/core/dist/chunk-TBWZZ3SI.js:118-235` | `createEffect` (with cleanup support) |
| `admin/node_modules/alien-signals/esm/index.mjs:57-64` | `startBatch`, `endBatch` |
| `admin/src/platform/console/OnboardingWizard.ts:2016` | Crash site |
| `admin/src/platform/console/ConsoleApp.ts:489-572` | Flat routing fix (workaround) |
| `admin/src/platform/console/store.ts:458-479` | `startNewWorkspaceWizard` reorder |
