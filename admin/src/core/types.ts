// admin/src/core/types.ts
// ─────────────────────────────────────────────────────────────────────────────
// Shared types for the Forma Auth admin frontend.
// ─────────────────────────────────────────────────────────────────────────────

// ── Pagination ──────────────────────────────────────────────────────────────

export interface PageState {
  limit: number;
  offset: number;
}

// ── Filters ─────────────────────────────────────────────────────────────────

export interface CustomerFilters {
  q: string;
  status: string;
  category: string;
  plan: string;
  created_from: string;
  created_to: string;
}

export interface TenantFilters {
  q: string;
  status: string;
  plan: string;
  owner_user_id: string;
}

export interface PaymentFilters {
  status: string;
  customer_user_id: string;
  from: string;
  to: string;
}

export interface TimelineFilters {
  actor: string;
  action: string;
  resource: string;
  request_id: string;
  from: string;
  to: string;
}

// ── Data Models ─────────────────────────────────────────────────────────────

export interface Stats {
  customers: number;
  active_customers: number;
  suspended_customers: number;
  tenants: number;
  trial_expiring_7d: number;
  revenue_30d_cents: number;
}

export interface Analytics {
  customer_signups: { day: string; count: number }[];
  revenue_paid_cents: { day: string; amount_cents: number }[];
}

export interface Customer {
  user_id: string;
  email: string;
  status: string;
  category: string;
  company_name: string;
  tenant_count: number;
  lifetime_revenue_cents: number;
  trial_expiring_tenants: number;
  failed_logins_24h: number;
  payment_failed_30d: boolean;
  created_at: string;
}

export interface CustomerInsights {
  lifecycle_history: AuditEvent[];
  recent_auth_events: AuthEvent[];
  recent_payments: Payment[];
  payment_summary: PaymentSummary;
}

export interface AuditEvent {
  created_at: string;
  action: string;
  actor_email?: string;
  actor_user_id?: string;
  request_id?: string;
}

export interface AuthEvent {
  created_at: string;
  source: string;
  action: string;
  result: string;
  failure_reason?: string;
}

export interface Payment {
  id: string;
  customer_user_id: string;
  customer_email?: string;
  tenant_id?: string;
  tenant_slug?: string;
  amount_cents: number;
  currency: string;
  status: string;
  external_invoice_id?: string;
  receipt_url?: string;
  notes?: string;
  created_at: string;
}

export interface PaymentSummary {
  paid: number;
  pending: number;
  failed: number;
  refunded: number;
}

export interface Tenant {
  id: string;
  name: string;
  slug: string;
  plan: string;
  status: string;
  trial_ends_at: string | null;
  owner_user_id: string;
  created_at: string;
  max_users: number | null;
}

export interface TenantDetail extends Tenant {
  management_api_key?: string;
  allowed_providers?: string[];
  allowed_origins?: string[];
  branding?: {
    app_name?: string;
    primary_color?: string;
    logo_url?: string;
    login_redirect_url?: string;
  };
}

export interface TenantInventoryItem extends Tenant {
  owner_email: string;
  owner_company: string;
  user_count: number;
  active_users_7d: number;
  active_sessions: number;
  failed_logins_24h: number;
  payment_failed_30d: boolean;
  last_login_at: string | null;
}

export interface TenantStats {
  total_users: number;
  active_sessions: number;
  logins_24h: number;
}

export interface Invite {
  id: string;
  email: string;
  status: string;
  expires_at: string;
  accepted_at: string | null;
}

export interface TimelineEvent {
  created_at: string;
  actor: string;
  action: string;
  resource: string;
  request_id: string;
  source: string;
  result: string;
}

export interface Facets {
  status: string[];
  category?: string[];
  plan: string[];
}

export interface Toast {
  message: string;
  kind: 'success' | 'error' | 'info';
}

// ── View Names ──────────────────────────────────────────────────────────────

export type OwnerView = 'overview' | 'customers' | 'billing' | 'timeline' | 'invites' | 'security';
export type TenantDashboardView = 'profile' | 'history' | 'integration';
export type TenantAdminView = 'stats' | 'users' | 'roles' | 'webhooks' | 'audit';

// ── Component Props ─────────────────────────────────────────────────────────

export interface SidebarProps {
  brand: { name: string; mark: string; subtitle: string };
  items: NavItem[] | (() => NavItem[]);
  footer: NavItem[];
  activeView: () => string;
  onNavigate: (view: string) => void;
  env?: string;
}

export interface NavItem {
  id: string;
  label: string;
  icon: string;  // SVG string
  danger?: boolean;
}

export interface HeaderProps {
  title: string;
  subtitle?: string;
  loading?: () => boolean;
  actions?: HTMLElement[];
}

export interface DataTableProps<T> {
  headers: string[];
  rows: () => T[];
  keyFn: (row: T) => string;
  renderRow: (row: T) => HTMLElement;
  emptyText?: string;
}

export interface PagerProps {
  total: () => number;
  page: () => PageState;
  onNavigate: (dir: 'prev' | 'next') => void;
}

export interface FilterBarProps {
  id: string;
  fields: FilterField[];
  onApply: () => void;
  onClear: () => void;
  onSave?: () => void;
  onLoad?: () => void;
  onExport?: () => void;
  total?: () => number;
}

export interface FilterField {
  name: string;
  label: string;
  type: 'text' | 'select' | 'datetime';
  value: () => string;
  onChange: (value: string) => void;
  placeholder?: string;
  options?: () => { value: string; label: string }[];
}

export interface TenantConfig {
  slug: string;
  name: string;
  primary_color: string;
  logo_url: string | null;
  providers: string[];
  server_url: string;
  signup_mode?: string;
  login_redirect_url?: string;
  client_id?: string;
}

// ── Introspect (Plane 3 auth data) ────────────────────────────────────────

export interface IntrospectData {
  user: { id: string; email: string; name: string | null; roles: string[]; created_at: string };
  session: { id: string; created_at: string; expires_at: string } | null;
  tenant: { id: string; slug: string };
  token: { issuer: string; audience: string; issued_at: number; expires_at: number; algorithm: string };
}

// ── Utility ─────────────────────────────────────────────────────────────────

export interface CsvColumn {
  key: string;
  label: string;
}
