// admin/e2e/selectors.ts
/** Centralized selector map — update here when UI class names change. */

export const Account = {
  shell: '.account-shell',
  tabs: '.account-tabs',
  tenantCard: '.tenant-card',
} as const;

export const Workspace = {
  planeNav: '.plane-nav',
  planeNavCollapsed: '.plane-nav-collapsed',
  tier1: '.plane-tier1',
  tier2: '.plane-tier2',
  activePlane: '.plane-btn.active',
  tenantTrigger: '.plane-tenant-trigger',
  tenantMenu: '.plane-tenant-menu',
  tenantMenuItem: '.plane-tenant-menu-item',
  tenantMenuBack: '.plane-tenant-menu-back',
  tenantName: '.plane-tenant-name',
  avatar: '.plane-avatar',
} as const;

export const Modal = {
  content: '.modal-content',
  slugAvailable: '.slug-status-available',
  slugTaken: '.slug-status-taken',
  slugInvalid: '.slug-status-invalid',
} as const;

export const TenantLogin = {
  card: '.tl-card',
  appName: '.tl-app-name',
  subtitle: '.tl-subtitle',
  passwordField: '#tl-password',
  passwordToggle: '.tl-password-toggle',
  toggleLink: '.tl-toggle .tl-link',
  footer: '.secured-badge',
} as const;
