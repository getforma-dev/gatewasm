import { h, createSignal, createShow, onCleanup } from 'formajs';
import { PLANES, type Plane, type ConsoleView, type PlatformUser, type Tenant } from '../store';

// ── Accent colors per plane ────────────────────────────────────────────────

const PLANE_ACCENTS: Record<Plane, string> = {
  operator: '#F05A3A',
  organization: '#2E9BFF',
  enduser: '#1EDC82',
};

// ── Helpers ─────────────────────────────────────────────────────────────────

function userDisplayName(name: string | null | undefined, email: string): string {
  const trimmed = (name || '').trim();
  if (trimmed) return trimmed;
  const local = email.split('@')[0] || email;
  return local.replace(/[._-]+/g, ' ');
}

function userInitials(name: string | null | undefined, email: string): string {
  const source = userDisplayName(name, email)
    .split(/\s+/)
    .map((part) => part.trim())
    .filter(Boolean);

  if (source.length === 0) return email.slice(0, 2).toUpperCase();

  return source
    .slice(0, 2)
    .map((part) => part[0] || '')
    .join('')
    .toUpperCase() || email.slice(0, 2).toUpperCase();
}

// ── Props ───────────────────────────────────────────────────────────────────

export interface PlaneNavProps {
  plane: () => Plane;
  view: () => ConsoleView;
  onPlaneSwitch: (plane: Plane) => void;
  onTabSwitch: (view: ConsoleView) => void;
  onSettings: () => void;
  onSignOut: () => void;
  user: () => PlatformUser | null;
  tenant: () => Tenant | null;
}

// ── Component ───────────────────────────────────────────────────────────────

export function PlaneNav(props: PlaneNavProps) {
  const [collapsed, setCollapsed] = createSignal(false);
  const [menuOpen, setMenuOpen] = createSignal(false);
  const [flashing, setFlashing] = createSignal(false);
  const [flashAccent, setFlashAccent] = createSignal(PLANE_ACCENTS.operator);

  // ── Uptime ticker ──────────────────────────────────────────────────────

  const startTime = Date.now();
  const [uptime, setUptime] = createSignal('00:00:00');
  const uptimeTimer = setInterval(() => {
    const elapsed = Date.now() - startTime;
    const hrs = Math.floor(elapsed / 3600000);
    const min = Math.floor((elapsed % 3600000) / 60000);
    const sec = Math.floor((elapsed % 60000) / 1000);
    setUptime(
      String(hrs).padStart(2, '0') + ':' +
      String(min).padStart(2, '0') + ':' +
      String(sec).padStart(2, '0'),
    );
  }, 1000);
  onCleanup(() => clearInterval(uptimeTimer));

  // ── Derived state ──────────────────────────────────────────────────────

  const accent = () => PLANE_ACCENTS[props.plane()];
  const planeDef = () => PLANES.find((p) => p.key === props.plane()) || PLANES[0];
  const currentTabDef = () => {
    const pd = planeDef();
    return pd.tabs.find((t) => t.id === props.view()) || pd.tabs[0];
  };

  // ── Plane switch with flash ────────────────────────────────────────────

  function switchPlane(nextPlane: Plane) {
    if (nextPlane === props.plane()) return;
    setFlashAccent(PLANE_ACCENTS[nextPlane]);
    setFlashing(true);
    setTimeout(() => setFlashing(false), 200);
    props.onPlaneSwitch(nextPlane);
  }

  // ── Keyboard shortcuts ─────────────────────────────────────────────────

  function handleKeyDown(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;

    if (!e.altKey && !e.ctrlKey && !e.metaKey) {
      if (e.key === '1') { switchPlane('operator'); e.preventDefault(); }
      if (e.key === '2') { switchPlane('organization'); e.preventDefault(); }
      if (e.key === '3') { switchPlane('enduser'); e.preventDefault(); }
      if (e.key === '`' || e.key === 'Escape') { setCollapsed(!collapsed()); e.preventDefault(); }
    }

    if (e.altKey && !e.ctrlKey && !e.metaKey) {
      const num = parseInt(e.key);
      if (num >= 1 && num <= 4) {
        const pd = planeDef();
        if (pd.tabs[num - 1]) {
          props.onTabSwitch(pd.tabs[num - 1].id as ConsoleView);
        }
        e.preventDefault();
      }
    }
  }

  document.addEventListener('keydown', handleKeyDown);
  onCleanup(() => document.removeEventListener('keydown', handleKeyDown));

  // ── Close avatar menu on outside click ─────────────────────────────────

  function handleDocClick(e: Event) {
    if (!menuOpen()) return;
    const target = e.target as Node;
    if (avatarShell && !avatarShell.contains(target)) {
      setMenuOpen(false);
    }
  }

  document.addEventListener('click', handleDocClick);
  onCleanup(() => document.removeEventListener('click', handleDocClick));

  // ── Header bar (56px) ──────────────────────────────────────────────────

  const headerLeft = h('div', { class: 'plane-header-left' },
    h('div', {
      class: 'plane-logo-mark',
      style: () => 'background:linear-gradient(135deg,' + accent() + ',' + accent() + '88)',
    }),
    h('span', { class: 'plane-product-name' }, 'GateWASM'),
    h('span', { class: 'plane-domain' }, () => window.location.hostname),
  );

  const headerCenter = h('div', { class: 'plane-header-center' },
    h('span', { class: 'plane-status-dot' }),
    h('span', { class: 'plane-status' }, 'ONLINE'),
    h('span', { class: 'plane-status' }, uptime),
  );

  const avatarMenu = createShow(
    menuOpen,
    () => h('div', { class: 'plane-avatar-menu', role: 'menu' },
      h('div', { class: 'plane-avatar-menu-header' },
        h('div', { class: 'plane-avatar-menu-name' },
          () => {
            const u = props.user();
            return u ? userDisplayName(u.name, u.email) : '';
          },
        ),
        h('div', { class: 'plane-avatar-menu-email' },
          () => props.user()?.email || '',
        ),
      ),
      h('div', { class: 'plane-avatar-menu-divider' }),
      h('button', {
        type: 'button',
        class: 'plane-avatar-menu-item hv',
        role: 'menuitem',
        onClick: () => {
          setMenuOpen(false);
          props.onSettings();
        },
      }, 'Settings'),
      h('button', {
        type: 'button',
        class: 'plane-avatar-menu-item plane-avatar-menu-item-danger hv',
        role: 'menuitem',
        onClick: () => {
          setMenuOpen(false);
          props.onSignOut();
        },
      }, 'Sign Out'),
    ),
  );

  const avatarShell = h('div', { class: 'plane-avatar-shell' },
    h('button', {
      type: 'button',
      class: 'plane-avatar hv',
      'aria-haspopup': 'menu',
      'aria-expanded': () => String(menuOpen()),
      onClick: () => setMenuOpen((v: boolean) => !v),
    },
      () => {
        const u = props.user();
        return u ? userInitials(u.name, u.email) : '--';
      },
    ),
    avatarMenu,
  ) as HTMLElement;

  const headerRight = h('div', { class: 'plane-header-right' },
    createShow(
      () => props.tenant() != null,
      () => h('span', { class: 'plane-tenant-indicator' },
        () => props.tenant()?.name || '',
      ),
    ),
    h('span', { class: 'plane-version' }, 'v0.44'),
    avatarShell,
  );

  const header = h('div', { class: 'plane-header' },
    headerLeft,
    headerCenter,
    headerRight,
  );

  // ── Tier 1 — Plane switcher ────────────────────────────────────────────

  const tier1 = h('div', { class: 'plane-tier1' },
    ...PLANES.map((pd) =>
      h('button', {
        type: 'button',
        class: () => 'plane-btn hv' + (props.plane() === pd.key ? ' active' : ''),
        style: () => props.plane() === pd.key
          ? 'border-bottom-color:' + PLANE_ACCENTS[pd.key] + ';background:' + PLANE_ACCENTS[pd.key] + '14'
          : '',
        onClick: () => switchPlane(pd.key),
      },
        h('span', {
          class: 'plane-num',
          style: () => props.plane() === pd.key
            ? 'background:' + PLANE_ACCENTS[pd.key]
            : '',
        }, String(pd.num)),
        h('span', { class: 'plane-label' },
          h('span', null, pd.label),
          h('span', { class: 'plane-sector' }, pd.sector),
        ),
      ),
    ),
    h('button', {
      type: 'button',
      class: 'plane-collapse-toggle hv',
      title: 'Toggle navigation (` or Esc)',
      onClick: () => setCollapsed(!collapsed()),
    },
      h('span', { class: 'plane-collapse-chevron' }, () => collapsed() ? '\u25B8' : '\u25BE'),
      h('kbd', null, '`'),
    ),
  );

  // ── Tier 2 — Page tabs ─────────────────────────────────────────────────

  const tier2 = h('div', { class: 'plane-tier2' },
    () => {
      const pd = planeDef();
      const tabs = pd.tabs.map((tab) =>
        h('button', {
          type: 'button',
          class: () => 'plane-tab hv' + (props.view() === tab.id ? ' active' : ''),
          style: () => props.view() === tab.id
            ? 'border-bottom-color:' + accent() + ';background:' + accent() + '0F'
            : '',
          onClick: () => props.onTabSwitch(tab.id as ConsoleView),
        },
          h('span', null, tab.label),
          h('span', { class: 'plane-tab-shortcut' }, tab.shortcut),
        ),
      );
      return h('div', { class: 'plane-tier2-inner' },
        ...tabs,
        h('span', {
          class: 'plane-accent-bar',
          style: () => 'background:linear-gradient(90deg,' + accent() + ',' + accent() + '00)',
        }),
      );
    },
  );

  // ── Collapsed mini-bar (42px) ──────────────────────────────────────────

  const collapsedBar = h('div', { class: 'plane-collapsed-bar' },
    h('div', { class: 'plane-readout' },
      h('span', { class: 'plane-version' },
        () => planeDef().sector,
      ),
      h('span', null, () => planeDef().label),
      h('span', { class: 'plane-readout-sep' }, '\u25B8'),
      h('span', null, () => currentTabDef().label),
    ),
    h('div', { class: 'plane-collapsed-right' },
      ...PLANES.map((pd) =>
        h('button', {
          type: 'button',
          class: () => 'plane-pip hv' + (props.plane() === pd.key ? ' active' : ''),
          style: () => props.plane() === pd.key
            ? 'background:' + PLANE_ACCENTS[pd.key]
            : '',
          onClick: () => switchPlane(pd.key),
        }, String(pd.num)),
      ),
      h('button', {
        type: 'button',
        class: 'plane-collapse-toggle hv',
        title: 'Expand navigation (` or Esc)',
        onClick: () => setCollapsed(false),
      },
        h('span', { class: 'plane-collapse-chevron' }, '\u25B4'),
        h('kbd', null, '`'),
      ),
    ),
  );

  // ── Flash overlay ──────────────────────────────────────────────────────

  const flashOverlay = createShow(
    flashing,
    () => h('div', {
      class: 'flash-overlay',
      style: () => 'background:' + flashAccent(),
    }),
  );

  // ── Root ───────────────────────────────────────────────────────────────

  return h('nav', {
    class: () => 'plane-nav' + (collapsed() ? ' plane-nav-collapsed' : ''),
  },
    header,
    createShow(
      () => !collapsed(),
      () => h('div', { class: 'plane-nav-expanded' }, tier1, tier2),
    ),
    createShow(
      collapsed,
      () => collapsedBar,
    ),
    flashOverlay,
  );
}
