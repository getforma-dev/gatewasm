import { createSignal, batch } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createApi } from '../../core/hooks/createApi';
import { createToast } from '../../core/hooks/createToast';
import { createSocket } from '../../core/hooks/createSocket';
import type { IntrospectData } from '../../core/types';

// ── Module-level singletons (shared across ConsoleApp island) ───────────

export const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const auth = createAuth({ serverUrl, tenant: 'platform' });
const api = createApi(auth);
export const toast = createToast();
const socket = createSocket(auth);
export { socket };

// ── Types ───────────────────────────────────────────────────────────────

export interface Tenant {
  id: string;
  name: string;
  slug: string;
  plan: string;
  status: string;
  onboarding_completed: boolean;
  onboarding_step: number;
  preferred_tab: string | null;
  primary_color: string | null;
  login_redirect_url: string | null;
  management_api_key?: string;
  access?: 'own' | 'team';
  team_role?: string; // "admin" | "editor" | "viewer" — only for access === 'team'
  created_at: string;
}

export interface TenantStats {
  total_users: number;
  active_sessions: number;
  logins_24h: number;
  failed_logins_24h: number;
  window: OverviewWindow;
  logins_in_window: number;
  failed_logins_in_window: number;
  series: TenantStatsSeriesPoint[];
  activity: TenantActivityItem[];
  providers: Record<string, number>;
  latest_user: { email: string; provider: string; created_at: string } | null;
}

export interface TenantStatsSeriesPoint {
  bucket_start: string;
  label: string;
  logins: number;
  failed_logins: number;
  sessions_started: number;
}

export interface TenantActivityItem {
  id: string;
  kind: 'user_created' | 'login_succeeded' | 'login_failed' | 'session_started';
  email: string;
  provider: string | null;
  description: string;
  created_at: string;
}

interface TenantUser {
  id: string;
  email: string;
  name: string | null;
  email_verified: boolean;
  is_locked: boolean;
  provider: string | null;
  roles: string[];
  active_sessions: number;
  created_at: string;
}

export interface UserSession {
  id: string;
  created_at: string;
  expires_at: string;
  auth_method: string;
  auth_provider: string | null;
  last_activity_at: string | null;
  login_ip: string | null;
  mfa_verified: boolean;
}

export interface AuditLogEntry {
  id: string;
  email: string;
  user_id: string | null;
  success: boolean;
  ip_address: string | null;
  user_agent: string | null;
  failure_reason: string | null;
  created_at: string;
}

export interface TenantSession {
  id: string;
  user_id: string;
  email: string;
  created_at: string;
  expires_at: string;
  auth_method: string;
  auth_provider: string | null;
  last_activity_at: string | null;
  login_ip: string | null;
  mfa_verified: boolean;
}

export interface Passkey {
  id: string;
  name: string;
  is_synced: boolean;
  user_id: string;
  email: string;
  aaguid: string | null;
  created_at: string;
  last_used_at: string | null;
}

export interface Role {
  id: string;
  name: string;
  description: string | null;
  is_system: boolean;
  permissions: string[];
  tenant_id: string | null;
  created_at: string;
}

interface HostedPageData {
  headline: string;
  body: string;
  cta_text: string;
  cta_link: string;
  custom_html: string | null;
  updated_at: string | null;
}

export interface SecurityConfig {
  idle_timeout_minutes: number;
  max_sessions_per_user: number;
  host_prefix_cookies: boolean;
  csp_enabled: boolean;
  client_fingerprint: boolean;
  webauthn_enabled: boolean;
  webauthn_attestation: string;
  webauthn_require_device_bound: boolean;
  fedcm_enabled: boolean;
  dpop_required: boolean;
  par_required: boolean;
  mtls_enabled: boolean;
  introspection_enabled: boolean;
  caep_enabled: boolean;
  impossible_travel_enabled: boolean;
  impossible_travel_speed_threshold_kmh: number;
}

export interface TenantInvite {
  id: string;
  email: string;
  role_id: string;
  role_name: string | null;
  status: string;
  expires_at: string;
  created_at: string;
}

export interface Organization {
  id: string;
  name: string;
  slug: string;
  metadata: any;
  created_at: string;
  updated_at: string;
}

export interface OrgMember {
  user_id: string;
  email: string;
  name: string | null;
  role: string;
  created_at: string;
}

export interface UserDetail {
  id: string;
  email: string;
  name: string | null;
  email_verified: boolean;
  login_attempts: number;
  locked_until: string | null;
  roles: string[];
  permissions: string[];
  providers: string[];
  active_sessions: number;
  created_at: string;
}

export interface Webhook {
  id: string;
  url: string;
  events: string[];
  active: boolean;
  description: string | null;
  created_at: string;
  updated_at: string;
}

export interface WebhookDelivery {
  id: string;
  event_type: string;
  payload: any;
  response_status: number | null;
  response_body: string | null;
  attempt: number;
  max_attempts: number;
  status: string;
  delivered_at: string | null;
  created_at: string;
}

export interface PlatformUser {
  id: string;
  email: string;
  name: string | null;
  avatar: string | null;
  email_verified: boolean;
  roles: string[];
  auth_method: string;
  mfa_verified: boolean;
  mfa_enabled: boolean;
}

export interface MyPasskey {
  id: string;
  name: string;
  created_at: string;
  last_used_at: string | null;
  is_synced: boolean;
  transports: string[];
}

export type ConsoleView =
  | 'overview'
  | 'integrate'
  | 'security'
  | 'members'
  | 'hosted-page'
  | 'organizations'
  | 'roles'
  | 'audit-log'
  | 'sessions'
  | 'passkeys'
  | 'tokens'
  | 'activity';

export type TenantWorkspaceView = ConsoleView;
export type IntegrateSection = 'guide' | 'session-inspector' | 'webhooks';
export type OverviewWindow = '24h' | '7d' | '30d';

export type Plane = 'operator' | 'organization' | 'enduser';

export type AppMode = 'account' | 'workspace';
export type AccountTab = 'tenants' | 'team' | 'api-keys' | 'account-settings';

export interface TenantAssignment {
  tenant_id: string;
  tenant_name: string;
  role: 'admin' | 'editor' | 'viewer';
}

export interface PlatformMember {
  id: string;
  user_id: string;
  is_account_owner: boolean;
  status: 'active' | 'suspended';
  invited_by_user_id: string | null;
  email: string;
  name: string | null;
  last_login_at: string | null;
  created_at: string;
  tenant_assignments: TenantAssignment[];
}

export type PlatformRole = 'admin' | 'editor' | 'viewer';

interface PlaneTab {
  id: string;
  label: string;
  shortcut: string;
}

export const PLANES: { key: Plane; label: string; sector: string; num: number; tabs: PlaneTab[] }[] = [
  {
    key: 'operator', label: 'Operator', sector: 'SEC-01', num: 1,
    tabs: [
      { id: 'overview', label: 'Overview', shortcut: '\u2325\u0031' },
      { id: 'integrate', label: 'Integrate', shortcut: '\u2325\u0032' },
      { id: 'security', label: 'Security', shortcut: '\u2325\u0033' },
    ],
  },
  {
    key: 'organization', label: 'Organization', sector: 'SEC-02', num: 2,
    tabs: [
      { id: 'members', label: 'Members', shortcut: '\u2325\u0031' },
      { id: 'hosted-page', label: 'Hosted Page', shortcut: '\u2325\u0032' },
      { id: 'organizations', label: 'Organizations', shortcut: '\u2325\u0033' },
      { id: 'roles', label: 'Roles', shortcut: '\u2325\u0034' },
      { id: 'audit-log', label: 'Audit Log', shortcut: '\u2325\u0035' },
    ],
  },
  {
    key: 'enduser', label: 'End User', sector: 'SEC-03', num: 3,
    tabs: [
      { id: 'sessions', label: 'Sessions', shortcut: '\u2325\u0031' },
      { id: 'passkeys', label: 'Passkeys', shortcut: '\u2325\u0032' },
      { id: 'tokens', label: 'Tokens', shortcut: '\u2325\u0033' },
      { id: 'activity', label: 'Activity', shortcut: '\u2325\u0034' },
    ],
  },
];

// ── State ───────────────────────────────────────────────────────────────

export const [tenants, setTenants] = createSignal<Tenant[]>([]);
export const [selectedTenant, setSelectedTenant] = createSignal<Tenant | null>(null);
const [selectedTenantStats, setSelectedTenantStats] = createSignal<TenantStats | null>(null);
export { selectedTenantStats };
const [loading, setLoading] = createSignal(false);
export { loading };
const [view, setView] = createSignal<ConsoleView>('overview');
export { view };
const [users, setUsers] = createSignal<TenantUser[]>([]);
export { users };
const [usersLoading, setUsersLoading] = createSignal(false);
export { usersLoading };
const [hostedPage, setHostedPage] = createSignal<HostedPageData | null>(null);
export { hostedPage };
const [introspectData, setIntrospectData] = createSignal<IntrospectData | null>(null);
export { introspectData };
const [introspectToken, setIntrospectToken] = createSignal<string | null>(null);
export { introspectToken };
const [introspectUnavailable, setIntrospectUnavailable] = createSignal(false);
export { introspectUnavailable };
const [userSessions, setUserSessions] = createSignal<UserSession[]>([]);
export { userSessions, setUserSessions };
const [userSessionsLoading, setUserSessionsLoading] = createSignal(false);
export { userSessionsLoading };
const [auditLog, setAuditLog] = createSignal<AuditLogEntry[]>([]);
export { auditLog };
const [auditLogLoading, setAuditLogLoading] = createSignal(false);
export { auditLogLoading };
const [allSessions, setAllSessions] = createSignal<TenantSession[]>([]);
export { allSessions };
const [allSessionsLoading, setAllSessionsLoading] = createSignal(false);
export { allSessionsLoading };
const [passkeys, setPasskeys] = createSignal<Passkey[]>([]);
export { passkeys };
const [passkeysLoading, setPasskeysLoading] = createSignal(false);
export { passkeysLoading };
const [roles, setRoles] = createSignal<Role[]>([]);
export { roles };
const [rolesLoading, setRolesLoading] = createSignal(false);
export { rolesLoading };
const [jwks, setJwks] = createSignal<any>(null);
export { jwks };
const [integrateSection, setIntegrateSectionSignal] = createSignal<IntegrateSection>('guide');
export { integrateSection };
const [overviewWindow, setOverviewWindowSignal] = createSignal<OverviewWindow>('7d');
export { overviewWindow };
const [currentUser, setCurrentUser] = createSignal<PlatformUser | null>(null);
export { currentUser };
const [plane, setPlaneSignal] = createSignal<Plane>('operator');
export { plane };
const [mode, setMode] = createSignal<AppMode>('account');
export { mode };
export const [accountTab, setAccountTab] = createSignal<AccountTab>('tenants');
const [securityConfig, setSecurityConfig] = createSignal<SecurityConfig | null>(null);
export { securityConfig };
const [securityConfigLoading, setSecurityConfigLoading] = createSignal(false);
export { securityConfigLoading };
const [tenantPlan, setTenantPlan] = createSignal<string>('trial');
export { tenantPlan };
const [invites, setInvites] = createSignal<TenantInvite[]>([]);
export { invites };
const [invitesLoading, setInvitesLoading] = createSignal(false);
export { invitesLoading };
const [organizations, setOrganizations] = createSignal<Organization[]>([]);
export { organizations };
const [organizationsLoading, setOrganizationsLoading] = createSignal(false);
export { organizationsLoading };
const [userDetail, setUserDetail] = createSignal<UserDetail | null>(null);
export { userDetail, setUserDetail };
const [myPasskeys, setMyPasskeys] = createSignal<MyPasskey[]>([]);
export { myPasskeys };
const [webhooks, setWebhooks] = createSignal<Webhook[]>([]);
export { webhooks };
const [webhooksLoading, setWebhooksLoading] = createSignal(false);
export { webhooksLoading };
const [platformRole, setPlatformRole] = createSignal<PlatformRole | null>(null);
export { platformRole };
const [accountOwner, setAccountOwner] = createSignal(false);
export { accountOwner };
const [teamMembers, setTeamMembers] = createSignal<PlatformMember[]>([]);
export { teamMembers };
const [teamLoading, setTeamLoading] = createSignal(false);
export { teamLoading };

let tenantDetailRequestSeq = 0;
let usersRequestSeq = 0;
let hostedPageRequestSeq = 0;
let introspectRequestSeq = 0;
let sessionRequestSeq = 0;
let stopOverviewRefresh: (() => void) | null = null;

function withTransientTenantFields(nextTenant: Tenant, existingTenant?: Tenant | null): Tenant {
  if (nextTenant.management_api_key || !existingTenant?.management_api_key) {
    return nextTenant;
  }

  return { ...nextTenant, management_api_key: existingTenant.management_api_key };
}

function findKnownTenant(id: string): Tenant | null {
  const current = selectedTenant();
  if (current?.id === id) {
    return current;
  }

  return tenants().find((tenant) => tenant.id === id) || null;
}

function resetTenantScopedState(): void {
  batch(() => {
    setSelectedTenantStats(null);
    setUsers([]);
    setUsersLoading(false);
    setHostedPage(null);
    setIntrospectData(null);
    setIntrospectToken(null);
    setIntrospectUnavailable(false);
    setUserSessions([]);
    setUserSessionsLoading(false);
    setAuditLog([]);
    setAuditLogLoading(false);
    setAllSessions([]);
    setAllSessionsLoading(false);
    setPasskeys([]);
    setPasskeysLoading(false);
    setRoles([]);
    setRolesLoading(false);
    setJwks(null);
    setSecurityConfig(null);
    setSecurityConfigLoading(false);
    setTenantPlan('trial');
    setInvites([]);
    setInvitesLoading(false);
    setOrganizations([]);
    setOrganizationsLoading(false);
    setUserDetail(null);
    setWebhooks([]);
    setWebhooksLoading(false);
    setIntegrateSectionSignal('guide');
    setOverviewWindowSignal('7d');
  });
}

function planeForView(v: ConsoleView): Plane {
  for (const p of PLANES) {
    if (p.tabs.some((t) => t.id === v)) return p.key;
  }
  return 'operator';
}

export function setPlane(nextPlane: Plane): void {
  if (plane() === nextPlane) return;
  setPlaneSignal(nextPlane);
  const planeDef = PLANES.find((p) => p.key === nextPlane);
  if (planeDef) {
    const firstTab = planeDef.tabs[0].id as ConsoleView;
    navigateTenantView(firstTab as TenantWorkspaceView);
  }
}

// ── API Functions ───────────────────────────────────────────────────────

export async function loadTenants(): Promise<void> {
  setLoading(true);
  try {
    const data = await api.get<{ tenants: Tenant[] }>('/platform/self-serve/tenants');
    const nextTenants = (data.tenants || []).map((tenant) =>
      withTransientTenantFields(tenant, findKnownTenant(tenant.id)),
    );
    setTenants(nextTenants);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load tenants', 'error');
  } finally {
    setLoading(false);
  }
}

export async function rotateTenantKey(tenantId: string): Promise<string | null> {
  try {
    const data = await api.post<{ api_key: string }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/rotate-key', {},
    );
    toast.show('API key rotated', 'success');
    const updated = tenants().map((t) =>
      t.id === tenantId ? { ...t, management_api_key: data.api_key } : t,
    );
    setTenants(updated);
    return data.api_key;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to rotate key', 'error');
    return null;
  }
}

export interface PendingInvite {
  id: string;
  role: string;
  inviter_email: string | null;
  tenant_count: number;
}
const [pendingInvites, setPendingInvites] = createSignal<PendingInvite[]>([]);
export { pendingInvites };

async function loadCurrentUser(): Promise<void> {
  try {
    const data = await api.get<PlatformUser & { is_account_owner?: boolean; platform_role?: PlatformRole; pending_invites?: PendingInvite[] }>('/auth/me');
    setCurrentUser(data);
    setAccountOwner(data.is_account_owner ?? false);
    if (data.platform_role) {
      setPlatformRole(data.platform_role);
    }
    setPendingInvites(data.pending_invites || []);
  } catch {
    setCurrentUser(null);
  }
}

export async function acceptInviteFromDashboard(inviteId: string): Promise<boolean> {
  try {
    const token = auth.accessToken();
    const res = await fetch(serverUrl + '/platform/admin/invites/accept', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: 'Bearer ' + token } : {}),
      },
      credentials: 'include',
      body: JSON.stringify({ invite_id: inviteId }),
    });
    if (res.ok) {
      toast.show('Invite accepted!', 'success');
      await Promise.all([loadTenants(), loadCurrentUser()]);
      return true;
    }
    const data = await res.json().catch(() => ({}));
    toast.show(data.message || 'Failed to accept invite', 'error');
    return false;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to accept invite', 'error');
    return false;
  }
}

export async function loadLoginHistory(): Promise<any[]> {
  try {
    const data = await api.get<{ history: any[] }>('/auth/login-history');
    return data.history || [];
  } catch {
    return [];
  }
}

export async function loadMfaStatus(): Promise<any> {
  try {
    return await api.get<any>('/auth/mfa/status');
  } catch {
    return null;
  }
}

export async function loadMyPasskeys(): Promise<void> {
  try {
    const data = await api.get<{ credentials: MyPasskey[] }>('/auth/webauthn/credentials');
    setMyPasskeys(data.credentials || []);
  } catch { setMyPasskeys([]); }
}

export async function renameMyPasskey(id: string, name: string): Promise<void> {
  try {
    await api.request('/auth/webauthn/credentials/' + encodeURIComponent(id), {
      method: 'PATCH',
      body: { name },
    });
    toast.show('Passkey renamed', 'success');
    await loadMyPasskeys();
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to rename passkey', 'error');
  }
}

export async function deleteMyPasskey(id: string): Promise<void> {
  try {
    await api.del('/auth/webauthn/credentials/' + encodeURIComponent(id));
    toast.show('Passkey deleted', 'success');
    await loadMyPasskeys();
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to delete passkey', 'error');
  }
}

async function loadTenantDetail(id: string, statsWindow: OverviewWindow = overviewWindow()): Promise<void> {
  if (!id) return;
  const requestSeq = ++tenantDetailRequestSeq;
  setLoading(true);
  try {
    const encodedId = encodeURIComponent(id);
    const [tenant, stats] = await Promise.all([
      api.get<Tenant>('/platform/self-serve/tenants/' + encodedId),
      api.get<TenantStats>('/platform/self-serve/tenants/' + encodedId + '/stats', { window: statsWindow }),
    ]);
    if (requestSeq !== tenantDetailRequestSeq || selectedTenant()?.id !== id) {
      return;
    }
    batch(() => {
      setSelectedTenant(withTransientTenantFields(tenant, findKnownTenant(id)));
      setSelectedTenantStats(stats);
      setLoading(false);
    });
  } catch (e) {
    if (requestSeq === tenantDetailRequestSeq && selectedTenant()?.id === id) {
      setLoading(false);
      toast.show(e instanceof Error ? e.message : 'Failed to load tenant', 'error');
    }
  }
}

export async function updateTenant(
  id: string,
  fields: Partial<Pick<Tenant, 'name' | 'slug' | 'primary_color' | 'login_redirect_url' | 'onboarding_completed' | 'onboarding_step' | 'preferred_tab'>>,
): Promise<Tenant | null> {
  try {
    const updatedTenant = await api.patch<Tenant>(
      '/platform/self-serve/tenants/' + encodeURIComponent(id),
      fields,
    );
    const updated = withTransientTenantFields(updatedTenant, findKnownTenant(id));
    // Refresh selected tenant if it matches
    const current = selectedTenant();
    if (current && current.id === id) {
      setSelectedTenant(updated);
    }
    // Refresh the tenant list so the card reflects changes
    const list = tenants();
    setTenants(list.map((t) => (t.id === id ? { ...t, ...updated } : t)));
    return updated;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to update tenant', 'error');
    return null;
  }
}

export async function createTenant(fields: { name: string; slug: string }): Promise<Tenant | null> {
  try {
    const data = await api.post<{ tenant: Tenant; api_key?: string }>(
      '/platform/self-serve/tenants',
      fields,
    );
    const created = withTransientTenantFields(
      {
        ...data.tenant,
        management_api_key: data.api_key || undefined,
      },
      null,
    );

    setTenants([created, ...tenants()]);
    // Creating first tenant makes the user an account owner — refresh to update signals
    if (!accountOwner()) {
      await loadCurrentUser();
    }
    toast.show('Tenant created', 'success');
    return created;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to create tenant', 'error');
    return null;
  }
}

export async function checkSlugAvailability(slug: string): Promise<boolean> {
  const data = await api.get<{ available: boolean }>(
    '/platform/self-serve/check-slug/' + encodeURIComponent(slug),
  );
  return data.available;
}


export async function loadUsers(tenantId: string): Promise<void> {
  const requestSeq = ++usersRequestSeq;
  setUsersLoading(true);
  try {
    const data = await api.get<{ users: TenantUser[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users',
    );
    if (requestSeq !== usersRequestSeq || selectedTenant()?.id !== tenantId) {
      return;
    }
    setUsers(data.users || []);
  } catch (e) {
    if (requestSeq === usersRequestSeq && selectedTenant()?.id === tenantId) {
      toast.show(e instanceof Error ? e.message : 'Failed to load users', 'error');
    }
  } finally {
    if (requestSeq === usersRequestSeq) {
      setUsersLoading(false);
    }
  }
}

export async function loadInvites(tenantId: string): Promise<void> {
  setInvitesLoading(true);
  try {
    const data = await api.get<{ invites: TenantInvite[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/invites',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setInvites(data.invites || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load invites', 'error');
  } finally {
    setInvitesLoading(false);
  }
}

export async function sendInvite(tenantId: string, email: string, role: string): Promise<boolean> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/invites', { email, role });
    toast.show('Invite sent to ' + email, 'success');
    await loadInvites(tenantId);
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to send invite', 'error');
    return false;
  }
}

export async function revokeInvite(tenantId: string, inviteId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/invites/' + encodeURIComponent(inviteId) + '/revoke', {});
    toast.show('Invite revoked', 'success');
    await loadInvites(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to revoke invite', 'error');
  }
}

export async function blockUser(tenantId: string, userId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/block', {});
    toast.show('User blocked', 'success');
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to block user', 'error');
  }
}

export async function unblockUser(tenantId: string, userId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/unblock', {});
    toast.show('User unblocked', 'success');
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to unblock user', 'error');
  }
}

export async function revokeUserSessions(tenantId: string, userId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/revoke-sessions', {});
    toast.show('Sessions revoked', 'success');
    setUserSessions([]);
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to revoke sessions', 'error');
  }
}

export async function loadUserSessions(tenantId: string, userId: string): Promise<void> {
  const seq = ++sessionRequestSeq;
  setUserSessionsLoading(true);
  try {
    const data = await api.get<{ sessions: UserSession[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/sessions',
    );
    if (seq !== sessionRequestSeq || selectedTenant()?.id !== tenantId) {
      return;
    }
    setUserSessions(data.sessions || []);
  } catch (e) {
    if (seq === sessionRequestSeq) {
      toast.show(e instanceof Error ? e.message : 'Failed to load sessions', 'error');
    }
  } finally {
    if (seq === sessionRequestSeq) {
      setUserSessionsLoading(false);
    }
  }
}

export async function loadAuditLog(tenantId: string): Promise<void> {
  setAuditLogLoading(true);
  try {
    const data = await api.get<{ audit_log: AuditLogEntry[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/audit-log',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setAuditLog(data.audit_log || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load audit log', 'error');
  } finally {
    setAuditLogLoading(false);
  }
}

export async function loadAllSessions(tenantId: string): Promise<void> {
  setAllSessionsLoading(true);
  try {
    const data = await api.get<{ sessions: TenantSession[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/sessions',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setAllSessions(data.sessions || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load sessions', 'error');
  } finally {
    setAllSessionsLoading(false);
  }
}

export async function loadPasskeys(tenantId: string): Promise<void> {
  setPasskeysLoading(true);
  try {
    const data = await api.get<{ passkeys: Passkey[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/passkeys',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setPasskeys(data.passkeys || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load passkeys', 'error');
  } finally {
    setPasskeysLoading(false);
  }
}

export async function loadRoles(tenantId: string): Promise<void> {
  setRolesLoading(true);
  try {
    const data = await api.get<{ roles: Role[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/roles',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setRoles(data.roles || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load roles', 'error');
  } finally {
    setRolesLoading(false);
  }
}

export async function createRole(tenantId: string, name: string, description: string): Promise<boolean> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/roles', { name, description, permissions: [] });
    toast.show('Role created', 'success');
    await loadRoles(tenantId);
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to create role', 'error');
    return false;
  }
}

export async function deleteRole(tenantId: string, roleId: string): Promise<void> {
  try {
    await api.del('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/roles/' + encodeURIComponent(roleId));
    toast.show('Role deleted', 'success');
    await loadRoles(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to delete role', 'error');
  }
}

export async function loadOrganizations(tenantId: string): Promise<void> {
  setOrganizationsLoading(true);
  try {
    const data = await api.get<{ organizations: Organization[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/organizations',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setOrganizations(data.organizations || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load organizations', 'error');
  } finally {
    setOrganizationsLoading(false);
  }
}

export async function createOrganization(tenantId: string, name: string, slug: string): Promise<boolean> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/organizations', { name, slug });
    toast.show('Organization created', 'success');
    await loadOrganizations(tenantId);
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to create organization', 'error');
    return false;
  }
}

export async function deleteOrganization(tenantId: string, orgId: string): Promise<void> {
  try {
    await api.del('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/organizations/' + encodeURIComponent(orgId));
    toast.show('Organization deleted', 'success');
    await loadOrganizations(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to delete organization', 'error');
  }
}

export async function loadOrgMembers(tenantId: string, orgId: string): Promise<OrgMember[]> {
  try {
    const data = await api.get<{ members: OrgMember[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/organizations/' + encodeURIComponent(orgId) + '/members',
    );
    return data.members || [];
  } catch {
    return [];
  }
}

export async function addOrgMember(tenantId: string, orgId: string, userId: string, role: string): Promise<boolean> {
  try {
    await api.post(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/organizations/' + encodeURIComponent(orgId) + '/members',
      { user_id: userId, role },
    );
    toast.show('Member added', 'success');
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to add member', 'error');
    return false;
  }
}

export async function removeOrgMember(tenantId: string, orgId: string, userId: string): Promise<void> {
  try {
    await api.del(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/organizations/' + encodeURIComponent(orgId) + '/members/' + encodeURIComponent(userId),
    );
    toast.show('Member removed', 'success');
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to remove member', 'error');
  }
}

export async function loadUserDetail(tenantId: string, userId: string): Promise<void> {
  try {
    const data = await api.get<UserDetail>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/detail',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setUserDetail(data);
  } catch { setUserDetail(null); }
}

export async function loadWebhooks(tenantId: string): Promise<void> {
  setWebhooksLoading(true);
  try {
    const data = await api.get<{ webhooks: Webhook[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/webhooks',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setWebhooks(data.webhooks || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load webhooks', 'error');
  } finally {
    setWebhooksLoading(false);
  }
}

export async function createWebhook(
  tenantId: string,
  payload: { url: string; events: string[]; description?: string },
): Promise<{ webhook: Webhook & { secret: string } } | null> {
  try {
    const data = await api.post<{ webhook: Webhook & { secret: string } }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/webhooks',
      payload,
    );
    toast.show('Webhook created', 'success');
    await loadWebhooks(tenantId);
    return data;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to create webhook', 'error');
    return null;
  }
}

export async function updateWebhook(
  tenantId: string,
  webhookId: string,
  patch: { url?: string; events?: string[]; active?: boolean; description?: string },
): Promise<boolean> {
  try {
    await api.request(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/webhooks/' + encodeURIComponent(webhookId),
      { method: 'PATCH', body: patch },
    );
    toast.show('Webhook updated', 'success');
    await loadWebhooks(tenantId);
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to update webhook', 'error');
    return false;
  }
}

export async function deleteWebhook(tenantId: string, webhookId: string): Promise<void> {
  try {
    await api.del(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/webhooks/' + encodeURIComponent(webhookId),
    );
    toast.show('Webhook deleted', 'success');
    await loadWebhooks(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to delete webhook', 'error');
  }
}

export async function loadWebhookDeliveries(
  tenantId: string,
  webhookId: string,
): Promise<WebhookDelivery[]> {
  try {
    const data = await api.get<{ deliveries: WebhookDelivery[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/webhooks/' + encodeURIComponent(webhookId) + '/deliveries',
    );
    return data.deliveries || [];
  } catch {
    return [];
  }
}

export async function testWebhook(
  tenantId: string,
  webhookId: string,
): Promise<{ delivery_id: string; status: string; response_status: number | null } | null> {
  try {
    const data = await api.post<{ delivery_id: string; status: string; response_status: number | null }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/webhooks/' + encodeURIComponent(webhookId) + '/test',
      {},
    );
    toast.show(data.status === 'delivered' ? 'Test delivered' : 'Test failed', data.status === 'delivered' ? 'success' : 'error');
    return data;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to send test', 'error');
    return null;
  }
}

export async function loadJwks(slug: string): Promise<void> {
  try {
    const res = await fetch('/t/' + encodeURIComponent(slug) + '/auth/jwks');
    if (res.ok) setJwks(await res.json());
  } catch { /* ignore */ }
}

async function loadHostedPage(tenantId: string): Promise<void> {
  const requestSeq = ++hostedPageRequestSeq;
  try {
    const data = await api.get<HostedPageData>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/hosted-page',
    );
    if (requestSeq !== hostedPageRequestSeq || selectedTenant()?.id !== tenantId) {
      return;
    }
    setHostedPage(data);
  } catch (e) {
    if (requestSeq === hostedPageRequestSeq && selectedTenant()?.id === tenantId) {
      toast.show(e instanceof Error ? e.message : 'Failed to load hosted page', 'error');
    }
  }
}

export async function saveHostedPage(tenantId: string, data: Partial<HostedPageData>): Promise<boolean> {
  try {
    await api.put(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/hosted-page',
      data,
    );
    toast.show('Page saved', 'success');
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to save page', 'error');
    return false;
  }

  await loadHostedPage(tenantId);
  return true;
}

export async function loadSecurityConfig(tenantId: string): Promise<void> {
  setSecurityConfigLoading(true);
  try {
    const data = await api.get<{ security_config: SecurityConfig; plan: string }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/security-config',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setSecurityConfig(data.security_config);
    setTenantPlan(data.plan);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load security config', 'error');
  } finally {
    setSecurityConfigLoading(false);
  }
}

export async function saveSecurityConfig(tenantId: string, patch: Partial<SecurityConfig>): Promise<boolean> {
  try {
    const data = await api.request<{ security_config: SecurityConfig; plan: string }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/security-config',
      { method: 'PATCH', body: patch },
    );
    setSecurityConfig(data.security_config);
    setTenantPlan(data.plan);
    toast.show('Security config updated', 'success');
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to update security config', 'error');
    return false;
  }
}


async function loadIntrospect(slug: string): Promise<void> {
  const requestSeq = ++introspectRequestSeq;
  try {
    // Plane 3 introspect: uses tenant's refresh cookie (same-origin, sent automatically)
    const res = await fetch('/t/' + encodeURIComponent(slug) + '/auth/token', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'include',
      body: JSON.stringify({ grant_type: 'refresh_token' }),
    });
    if (!res.ok) {
      if (requestSeq === introspectRequestSeq && selectedTenant()?.slug === slug) {
        batch(() => {
          setIntrospectData(null);
          setIntrospectToken(null);
          setIntrospectUnavailable(true);
        });
      }
      return;
    }
    const tokenData = await res.json();
    const accessToken = tokenData.access_token;
    if (!accessToken) {
      if (requestSeq === introspectRequestSeq && selectedTenant()?.slug === slug) {
        batch(() => {
          setIntrospectData(null);
          setIntrospectToken(null);
          setIntrospectUnavailable(true);
        });
      }
      return;
    }

    const introspectRes = await fetch('/t/' + encodeURIComponent(slug) + '/auth/introspect', {
      headers: { 'Authorization': 'Bearer ' + accessToken },
      credentials: 'include',
    });
    if (!introspectRes.ok) {
      if (requestSeq === introspectRequestSeq && selectedTenant()?.slug === slug) {
        batch(() => {
          setIntrospectData(null);
          setIntrospectToken(null);
          setIntrospectUnavailable(true);
        });
      }
      return;
    }

    const data = await introspectRes.json();
    if (requestSeq !== introspectRequestSeq || selectedTenant()?.slug !== slug) {
      return;
    }
    batch(() => {
      setIntrospectData(data);
      setIntrospectToken(accessToken);
      setIntrospectUnavailable(false);
    });
  } catch {
    if (requestSeq === introspectRequestSeq && selectedTenant()?.slug === slug) {
      batch(() => {
        setIntrospectData(null);
        setIntrospectToken(null);
        setIntrospectUnavailable(true);
      });
    }
  }
}

function startVisibilityRefresh(slug: string): () => void {
  const handler = () => {
    if (document.visibilityState === 'visible') {
      loadIntrospect(slug);
    }
  };
  document.addEventListener('visibilitychange', handler);
  return () => document.removeEventListener('visibilitychange', handler);
}

function clearOverviewRefresh(): void {
  if (stopOverviewRefresh) {
    stopOverviewRefresh();
    stopOverviewRefresh = null;
  }
}

function syncIntrospectRefresh(slug: string): void {
  clearOverviewRefresh();
  stopOverviewRefresh = startVisibilityRefresh(slug);
}

function preferredTenantView(tenant: Tenant): TenantWorkspaceView {
  const tab = tenant.preferred_tab;
  if (tab === 'hosted-page') return 'hosted-page';
  if (tab === 'integrate') return 'integrate';
  return 'overview';
}

async function hydrateTenantView(nextView: TenantWorkspaceView, tenant: Tenant): Promise<void> {
  const tasks: Promise<void>[] = [];

  // Only load tenant detail for overview (stats dashboard) — not every tab switch
  if (nextView === 'overview') {
    tasks.push(loadTenantDetail(tenant.id, overviewWindow()));
  }

  if (nextView === 'integrate') {
    tasks.push(loadIntrospect(tenant.slug));
    tasks.push(loadWebhooks(tenant.id));
    syncIntrospectRefresh(tenant.slug);
  } else if (nextView === 'members') {
    tasks.push(loadUsers(tenant.id));
    tasks.push(loadInvites(tenant.id));
  } else if (nextView === 'security') {
    tasks.push(loadSecurityConfig(tenant.id));
  } else if (nextView === 'hosted-page') {
    tasks.push(loadHostedPage(tenant.id));
  } else if (nextView === 'audit-log' || nextView === 'activity') {
    tasks.push(loadAuditLog(tenant.id));
  } else if (nextView === 'sessions') {
    tasks.push(loadAllSessions(tenant.id));
  } else if (nextView === 'passkeys') {
    tasks.push(loadPasskeys(tenant.id));
  } else if (nextView === 'organizations') {
    tasks.push(loadOrganizations(tenant.id));
  } else if (nextView === 'roles') {
    tasks.push(loadRoles(tenant.id));
  } else if (nextView === 'tokens') {
    tasks.push(loadJwks(tenant.slug));
  }

  await Promise.all(tasks);
}

export function navigateTenantView(nextView: TenantWorkspaceView): void {
  const tenant = selectedTenant();
  if (!tenant) return;

  setView(nextView);
  setPlaneSignal(planeForView(nextView));

  void hydrateTenantView(nextView, tenant);
}

export function changeOverviewWindow(nextWindow: OverviewWindow): void {
  if (overviewWindow() === nextWindow) {
    return;
  }

  setOverviewWindowSignal(nextWindow);

  const tenant = selectedTenant();
  if (!tenant || view() !== 'overview') {
    return;
  }

  void loadTenantDetail(tenant.id, nextWindow);
}

export function openIntegrateGuide(): void {
  setIntegrateSectionSignal('guide');

  navigateTenantView('integrate');
}

export function openSessionInspector(): void {
  const tenant = selectedTenant();

  setIntegrateSectionSignal('session-inspector');


  if (!tenant) return;

  if (view() === 'integrate') {
    void hydrateTenantView('integrate', tenant);
    return;
  }

  navigateTenantView('integrate');
}

export function setIntegrateSection(nextSection: IntegrateSection): void {
  if (integrateSection() === nextSection) {
    return;
  }

  setIntegrateSectionSignal(nextSection);


  const tenant = selectedTenant();
  if (!tenant || view() !== 'integrate') {
    return;
  }

  if (nextSection === 'session-inspector') {
    void hydrateTenantView('integrate', tenant);
  } else if (nextSection === 'webhooks') {
    void loadWebhooks(tenant.id);
  }
}

export async function refreshCurrentView(): Promise<void> {
  const currentView = view();
  const tenant = selectedTenant();

  if (mode() === 'account' || !tenant) {
    await Promise.all([loadTenants(), loadCurrentUser()]);
    return;
  }

  await Promise.all([
    loadTenants(),
    loadCurrentUser(),
    hydrateTenantView(currentView, tenant),
  ]);
}

// ── Navigation ──────────────────────────────────────────────────────────

// Internal: switch to workspace mode without pushing history
function switchToWorkspaceInternal(tenant: Tenant): void {
  const nextView = preferredTenantView(tenant);
  batch(() => {
    setSelectedTenant(tenant);
    resetTenantScopedState();
    setView(nextView);
    setPlaneSignal(planeForView(nextView));
    setMode('workspace');
  });

  // hydrateTenantView handles all data loading (including loadTenantDetail for overview)
  void hydrateTenantView(nextView, tenant);
}

// Internal: switch to account mode without pushing history
function switchToAccountInternal(): void {
  clearOverviewRefresh();
  batch(() => {
    tenantDetailRequestSeq += 1;
    usersRequestSeq += 1;
    hostedPageRequestSeq += 1;
    introspectRequestSeq += 1;
    sessionRequestSeq += 1;
    setLoading(false);
    setSelectedTenant(null);
    resetTenantScopedState();
    setMode('account');
    setAccountTab('tenants');
  });

}

export function selectTenant(tenant: Tenant): void {
  switchToWorkspaceInternal(tenant);
  history.pushState({ slug: tenant.slug }, '', '/platform/console/' + tenant.slug);
}

export function backToTenants(): void {
  switchToAccountInternal();
  history.pushState(null, '', '/platform/console');
}

export function initRouting(): void {
  window.addEventListener('popstate', () => {
    const path = window.location.pathname;
    const match = path.match(/^\/platform\/console\/([^/]+)/);
    if (match) {
      const slug = match[1];
      const tenant = tenants().find((t) => t.slug === slug);
      if (tenant) {
        switchToWorkspaceInternal(tenant);
      } else {
        switchToAccountInternal();
        history.replaceState(null, '', '/platform/console');
      }
    } else {
      switchToAccountInternal();
    }
  });
}

// ── WebSocket invalidation subscriptions ────────────────────────────────

socket.subscribe('team:changed', () => {
  loadTeamMembers();
  loadTeamInvites();
});

socket.subscribe('metrics:updated', () => {
  const t = selectedTenant();
  if (t && view() === 'overview') {
    loadTenantDetail(t.id);
  }
});

socket.subscribe('tenant:activity', () => {
  const t = selectedTenant();
  if (!t) return;
  if (view() === 'audit-log') {
    loadAuditLog(t.id);
  }
  if (view() === 'sessions') {
    loadAllSessions(t.id);
  }
});

// ── Lifecycle ───────────────────────────────────────────────────────────

export async function init(): Promise<void> {
  try {
    await auth.silentRefresh();
    await Promise.all([loadTenants(), loadCurrentUser()]);

    // Handle query params (invite acceptance, verified toast)
    const initParams = new URLSearchParams(window.location.search);

    const inviteParam = initParams.get('owner_invite');
    if (inviteParam) {
      try {
        // Use direct fetch (not api.post) because the accept endpoint is NOT
        // tenant-scoped — it lives at /platform/admin/invites/accept, not under /t/{slug}/
        const token = auth.accessToken();
        const res = await fetch(serverUrl + '/platform/admin/invites/accept', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            ...(token ? { Authorization: 'Bearer ' + token } : {}),
          },
          credentials: 'include',
          body: JSON.stringify({ token: inviteParam }),
        });
        if (res.ok) {
          toast.show('Invite accepted! Welcome to the team.', 'success');
        } else {
          const data = await res.json().catch(() => ({}));
          toast.show(data.message || 'Failed to accept invite', 'error');
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : 'Failed to accept invite';
        toast.show(msg, 'error');
      }
      initParams.delete('owner_invite');
      await Promise.all([loadTenants(), loadCurrentUser()]);
    }

    if (initParams.get('verified') === 'true') {
      toast.show('Email verified successfully', 'success');
      initParams.delete('verified');
    }

    // Clean query params if any were consumed
    if (inviteParam || initParams.get('verified') === null) {
      const cleanPath = window.location.pathname + (initParams.toString() ? '?' + initParams.toString() : '');
      history.replaceState(null, '', cleanPath);
    }

    // URL determines mode — no localStorage override
    const urlMatch = window.location.pathname.match(/^\/platform\/console\/([^/]+)/);
    if (urlMatch) {
      const slug = urlMatch[1];
      const tenant = tenants().find((t) => t.slug === slug);
      if (tenant) {
        switchToWorkspaceInternal(tenant);
        auth.startAutoRefresh();
        socket.connect(serverUrl + '/ws/ticket');
        return;
      }
      // Slug not found — clean up the URL, stay in account mode
      history.replaceState(null, '', '/platform/console');
    }

    // /platform/console (no slug) = account mode, always
    // Don't restore workspace state from localStorage — URL is the source of truth

    auth.startAutoRefresh();
    socket.connect(serverUrl + '/ws/ticket');
  } catch {
    window.location.href = '/platform/login';
    throw new Error('auth_redirect');
  }
}

export async function signout(): Promise<void> {
  clearOverviewRefresh();
  socket.disconnect();
  auth.stopAutoRefresh();
  setCurrentUser(null);
  await auth.logout();
  window.location.href = '/platform/login';
}

// ── Team API Functions ──────────────────────────────────────────────────

export async function loadTeamMembers(): Promise<void> {
  setTeamLoading(true);
  try {
    const data = await api.get<{ members: PlatformMember[] }>('/platform/admin/team');
    setTeamMembers(data.members);
  } catch {
    setTeamMembers([]);
  } finally {
    setTeamLoading(false);
  }
}

export async function loadTeamMemberDetail(memberId: string): Promise<PlatformMember | null> {
  try {
    const data = await api.get<{ member: PlatformMember }>('/platform/admin/team/' + encodeURIComponent(memberId));
    return data.member;
  } catch {
    return null;
  }
}

export async function updateTeamMember(
  memberId: string,
  fields: { status?: string; tenant_assignments?: { tenant_id: string; role: string }[] },
): Promise<boolean> {
  try {
    await api.request('/platform/admin/team/' + encodeURIComponent(memberId), { method: 'PATCH', body: fields });
    toast.show('Member updated', 'success');
    await loadTeamMembers();
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to update member', 'error');
    return false;
  }
}

export async function removeTeamMember(memberId: string): Promise<boolean> {
  try {
    await api.del('/platform/admin/team/' + encodeURIComponent(memberId));
    toast.show('Member removed', 'success');
    await loadTeamMembers();
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to remove member', 'error');
    return false;
  }
}

export async function resendVerification(): Promise<boolean> {
  try {
    await api.post('/t/platform/auth/resend-verification', {});
    toast.show('Verification email sent — check your inbox', 'success');
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to send verification email', 'error');
    return false;
  }
}

export async function sendTeamInvite(email: string, role: string, tenantIds: string[]): Promise<{ invite_url: string } | null> {
  try {
    const data = await api.post<{ invite_url: string }>('/platform/admin/team/invites', {
      email,
      role,
      tenant_ids: tenantIds,
    });
    toast.show('Invite sent to ' + email, 'success');
    await loadTeamMembers();
    return data;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to send invite', 'error');
    return null;
  }
}

export async function revokeTeamInvite(inviteId: string): Promise<boolean> {
  try {
    await api.post('/platform/admin/team/invites/' + encodeURIComponent(inviteId) + '/revoke', {});
    toast.show('Invite revoked', 'success');
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to revoke invite', 'error');
    return false;
  }
}

export async function loadTeamInvites(): Promise<any[]> {
  try {
    const data = await api.get<{ invites: any[] }>('/platform/admin/team/invites');
    return data.invites;
  } catch {
    return [];
  }
}

export function canEdit(): boolean {
  if (accountOwner()) return true;
  const role = platformRole();
  // Only viewers are explicitly read-only. New users (no role yet) and
  // admin/editor can edit — new users need to create their first workspace.
  if (role === 'viewer') return false;
  return true;
}

export function isOwner(): boolean {
  return accountOwner();
}
