import { h, createSignal, createShow, onCleanup } from 'formajs';
import { PLANES, type Plane, type ConsoleView, type PlatformUser, type Tenant } from '../store';
import { userDisplayName, userInitials, FAuthMark } from '../helpers';

// ── Accent colors per plane ────────────────────────────────────────────────

const PLANE_ACCENTS: Record<Plane, string> = {
  operator: '#c4a265',
  organization: '#2E9BFF',
  enduser: '#1EDC82',
};

// ── Props ───────────────────────────────────────────────────────────────────

export interface PlaneNavProps {
  plane: () => Plane;
  view: () => ConsoleView;
  onPlaneSwitch: (plane: Plane) => void;
  onTabSwitch: (view: ConsoleView) => void;
  onSettings: () => void;
  onSignOut: () => void;
  onThemeToggle: () => void;
  themeLabel: () => string;
  user: () => PlatformUser | null;
  tenant: () => Tenant | null;
  tenants: () => Tenant[];
  onTenantSwitch: (tenant: Tenant) => void;
  onBackToAccount: () => void;
}

// ── Component ───────────────────────────────────────────────────────────────

export function PlaneNav(props: PlaneNavProps) {
  const [collapsed, setCollapsed] = createSignal(false);
  const [menuOpen, setMenuOpen] = createSignal(false);
  const [tenantMenuOpen, setTenantMenuOpen] = createSignal(false);
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

  let flashTimer: ReturnType<typeof setTimeout> | undefined;

  function switchPlane(nextPlane: Plane) {
    if (nextPlane === props.plane()) return;
    setFlashAccent(PLANE_ACCENTS[nextPlane]);
    setFlashing(true);
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => setFlashing(false), 200);
    props.onPlaneSwitch(nextPlane);
  }

  onCleanup(() => clearTimeout(flashTimer));

  // ── Keyboard shortcuts ─────────────────────────────────────────────────

  function handleKeyDown(e: KeyboardEvent) {
    const el = e.target as HTMLElement;
    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable) return;

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

  // ── Close tenant menu on outside click ─────────────────────────────────

  function handleTenantMenuClick(e: Event) {
    if (!tenantMenuOpen()) return;
    const target = e.target as Node;
    if (tenantSwitcherEl && !tenantSwitcherEl.contains(target)) {
      setTenantMenuOpen(false);
    }
  }

  document.addEventListener('click', handleTenantMenuClick);
  onCleanup(() => document.removeEventListener('click', handleTenantMenuClick));

  // ── Header bar (56px) ──────────────────────────────────────────────────

  const tenantSwitcher = h('div', { class: 'plane-tenant-switcher' },
    h('button', {
      type: 'button',
      class: 'plane-tenant-trigger hv',
      onClick: () => setTenantMenuOpen(!tenantMenuOpen()),
    },
      h('span', { class: 'plane-tenant-name' }, () => props.tenant()?.name || ''),
      h('span', { class: 'plane-tenant-chevron' }, '\u25BE'),
    ),
    createShow(tenantMenuOpen, () =>
      h('div', { class: 'plane-tenant-menu' },
        h('button', {
          type: 'button',
          class: 'plane-tenant-menu-item plane-tenant-menu-back hv',
          onClick: () => { setTenantMenuOpen(false); props.onBackToAccount(); },
        }, '\u2190 All Tenants'),
        h('div', { class: 'plane-avatar-menu-divider' }),
        () => props.tenants().map((t) =>
          h('button', {
            type: 'button',
            class: () => 'plane-tenant-menu-item hv' + (props.tenant()?.id === t.id ? ' active' : ''),
            onClick: () => { setTenantMenuOpen(false); props.onTenantSwitch(t); },
          },
            h('span', null, t.name),
            h('span', { class: 'plane-tenant-menu-slug' }, '/t/' + t.slug),
          ),
        ),
      ),
    ),
  );

  const tenantSwitcherEl = tenantSwitcher as HTMLElement;

  const headerLeft = h('div', { class: 'plane-header-left' },
    h('div', { class: 'plane-logo-mark' }, FAuthMark(28)),
    h('span', { class: 'plane-product-name' }, 'GateWASM'),
    h('span', { class: 'plane-domain' }, () => window.location.hostname),
    tenantSwitcher,
  );

  const headerCenter = h('div', { class: 'plane-header-center' },
    h('span', { class: 'plane-status-dot', 'aria-label': 'System status: online' }),
    h('span', { class: 'plane-status' }, 'ONLINE'),
    h('span', { class: 'plane-status', 'aria-live': 'off', 'aria-label': 'Session uptime' }, uptime),
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
        class: 'plane-avatar-menu-item hv',
        role: 'menuitem',
        onClick: () => {
          setMenuOpen(false);
          props.onThemeToggle();
        },
      }, () => props.themeLabel()),
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
    () => PLANES.map((pd) =>
      h('button', {
        type: 'button',
        class: () => 'plane-btn hv' + (props.plane() === pd.key ? ' active' : ''),
        onClick: () => switchPlane(pd.key),
      },
        h('span', { class: 'plane-num' }, String(pd.num)),
        h('span', { class: 'plane-label' },
          h('span', null, pd.label),
          h('span', { class: 'plane-sector' }, pd.sector),
        ),
      ),
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
          onClick: () => props.onTabSwitch(tab.id as ConsoleView),
        },
          h('span', null, tab.label),
          h('span', { class: 'plane-tab-shortcut' }, tab.shortcut),
        ),
      );
      return h('div', { class: 'plane-tier2-inner' },
        ...tabs,
        h('span', { class: 'plane-accent-bar' }),
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
          onClick: () => switchPlane(pd.key),
        }, String(pd.num)),
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
    // Drawer handle — always visible, centered
    h('button', {
      type: 'button',
      class: 'plane-collapse-toggle',
      title: () => collapsed() ? 'Expand navigation (`)' : 'Collapse navigation (`)',
      onClick: () => setCollapsed(!collapsed()),
    },
      h('span', { class: 'plane-collapse-chevron' }, () => collapsed() ? '\u25BC' : '\u25B2'),
    ),
    createShow(
      collapsed,
      () => collapsedBar,
    ),
    flashOverlay,
  );
}
