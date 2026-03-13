// ---------------------------------------------------------------------------
// store.ts — Reactive state management for the tenant dashboard
// ---------------------------------------------------------------------------

import { createStore, batch } from 'formajs';
import type { Api } from '../../core/hooks/createApi';
import type { ToastController } from '../../core/hooks/createToast';
import type { TenantDashboardView } from '../../core/types';

// -- Data interfaces ---------------------------------------------------------

export interface UserProfile {
  id: string;
  email: string;
  name?: string;
  avatar?: string;
  roles: string[];
  email_verified: boolean;
  permissions?: string[];
  created_at: string;
}

export interface LoginHistoryEntry {
  created_at: string;
  success: boolean;
  ip_address?: string;
  user_agent?: string;
}

// -- Factory -----------------------------------------------------------------

export function createTenantDashboardStore(api: Api, toast: ToastController) {

  const [store, setStore] = createStore({
    view: 'profile' as TenantDashboardView,
    loading: false,

    // Profile
    profile: null as UserProfile | null,

    // Login history
    history: [] as LoginHistoryEntry[],
    historyLoaded: false,
  });

  // -- Data loaders ----------------------------------------------------------

  async function loadProfile(): Promise<void> {
    setStore({ loading: true });
    try {
      const data = await api.get<UserProfile>('/auth/me');
      setStore({ profile: data, loading: false });
    } catch (err: unknown) {
      setStore({ loading: false });
      toast.show(err instanceof Error ? err.message : 'Failed to load profile', 'error');
      throw err;
    }
  }

  async function loadHistory(): Promise<void> {
    if (store.historyLoaded) return;
    setStore({ loading: true });
    try {
      const data = await api.get<{ history?: LoginHistoryEntry[] }>('/auth/login-history');
      batch(() => {
        setStore({
          history: data.history ?? [],
          historyLoaded: true,
          loading: false,
        });
      });
    } catch (err: unknown) {
      setStore({ loading: false });
      toast.show(err instanceof Error ? err.message : 'Failed to load login history', 'error');
    }
  }

  // -- Navigation ------------------------------------------------------------

  function navigate(view: TenantDashboardView): void {
    setStore({ view });
    if (view === 'history') {
      loadHistory().catch(() => {});
    }
  }

  // -- Mutations -------------------------------------------------------------

  async function changePassword(currentPassword: string, newPassword: string): Promise<void> {
    try {
      await api.post('/auth/change-password', {
        current_password: currentPassword,
        new_password: newPassword,
      });
      toast.show('Password changed successfully', 'success');
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to change password';
      toast.show(msg, 'error');
      throw err;
    }
  }

  async function deleteAccount(): Promise<void> {
    try {
      await api.post('/auth/delete-account');
      toast.show('Account deleted', 'success');
    } catch (err: unknown) {
      toast.show(err instanceof Error ? err.message : 'Failed to delete account', 'error');
      throw err;
    }
  }

  // -- Public API ------------------------------------------------------------

  return {
    store,
    setStore,
    loadProfile,
    loadHistory,
    navigate,
    changePassword,
    deleteAccount,
  };
}

export type TenantDashboardStore = ReturnType<typeof createTenantDashboardStore>;
