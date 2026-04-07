/**
 * fAuth Bridge — TypeScript Definitions
 * Type-safe interface for the fAuth bridge module.
 */

export interface GateAuthConfig {
  /** URL of the fAuth server (e.g. "http://localhost:3001") */
  serverUrl: string;
  /** OAuth client ID registered on the server */
  clientId: string;
  /** OAuth redirect URI (defaults to `window.location.origin + '/auth/callback'`) */
  redirectUri?: string;
  /** Path to the WASM JS glue module (auto-detected by default) */
  wasmPath?: string;
  /** OAuth scopes to request (defaults to `['openid', 'profile', 'email']`) */
  scopes?: string[];
  /** Enable automatic token refresh (defaults to `true`) */
  autoRefresh?: boolean;
  /** Seconds before expiry to trigger refresh (defaults to `300`) */
  refreshThreshold?: number;
  /** Enable DPoP (Demonstrating Proof of Possession) proof generation (defaults to `false`) */
  dpopEnabled?: boolean;
}

export interface AuthUser {
  id: string;
  email: string;
  name: string | null;
  avatar: string | null;
  email_verified: boolean;
  roles: string[];
}

export type AuthStateName = "idle" | "authenticated" | "loading" | "error";

export interface AuthStateIdle {
  state: "idle";
  user: null;
}

export interface AuthStateAuthenticated {
  state: "authenticated";
  user: AuthUser;
}

export interface AuthStateLoading {
  state: "loading";
}

export interface AuthStateError {
  state: "error";
  error: { message: string; [key: string]: unknown };
}

export type AuthState =
  | AuthStateIdle
  | AuthStateAuthenticated
  | AuthStateLoading
  | AuthStateError;

export interface EmailCredentials {
  email: string;
  password: string;
  /** Set to `true` for registration, `false`/omitted for login */
  isRegister?: boolean;
  /** Optional display name (only used during registration) */
  name?: string;
}

export type StateChangeCallback = (state: AuthState) => void;
export type Unsubscribe = () => void;

declare class GateAuthBridge {
  /**
   * Initialize the bridge. Loads the WASM module and handles any pending OAuth callback.
   */
  static init(config: GateAuthConfig): Promise<GateAuthBridge>;

  /**
   * Start a login flow.
   *
   * For OAuth providers (`"google"`, `"github"`), this redirects the browser.
   * For email/password, pass credentials to authenticate directly.
   *
   * @param provider - The auth provider (default: `"google"`)
   * @param credentials - Required for `"email"` provider
   * @returns The authenticated user (for email/password/webauthn flows)
   */
  login(provider?: string, credentials?: EmailCredentials): Promise<AuthUser | void>;

  /**
   * Log out the current user. Revokes the refresh token on the server
   * and clears local state.
   */
  logout(): Promise<void>;

  /** Get the current authenticated user, or `null` if not authenticated. */
  getUser(): AuthUser | null;

  /**
   * Get a valid access token (JWT). Automatically refreshes if near expiry.
   * Returns `null` if not authenticated.
   */
  getToken(): Promise<string | null>;

  /** Check if a user is currently authenticated. */
  isAuthenticated(): boolean;

  /**
   * Subscribe to auth state changes.
   * @returns An unsubscribe function.
   */
  onStateChange(callback: StateChangeCallback): Unsubscribe;

  /** Check if the current user has a specific role. */
  hasRole(role: string): boolean;

  /** Check if the current user has a specific permission (resolved from roles). */
  hasPermission(permission: string): Promise<boolean>;

  // ── DPoP (Demonstrating Proof of Possession) ──────────────────────

  /**
   * Make an authenticated fetch call with DPoP proof attached (if enabled).
   * Automatically includes the Bearer token and DPoP header.
   *
   * @param url - The URL to fetch
   * @param options - Standard fetch options (method, body, headers, etc.)
   * @returns The fetch Response
   */
  fetchWithDPoP(url: string, options?: RequestInit): Promise<Response>;

  // ── Passkey (WebAuthn) Methods ─────────────────────────────────────

  /**
   * Register a new passkey for the currently authenticated user.
   * Requires the user to already be logged in (JWT is sent as Bearer token).
   * Invokes the browser WebAuthn API to create a credential.
   *
   * @param name - Friendly name for the credential (default: "My Passkey")
   * @returns The registered credential info
   */
  registerPasskey(name?: string): Promise<PasskeyCredentialInfo>;

  /**
   * Authenticate using a passkey. Does not require an existing session.
   * On success, processes tokens and emits an authenticated state change.
   *
   * @param email - The user's email address
   * @returns The authenticated user
   */
  loginWithPasskey(email: string): Promise<AuthUser>;

  /**
   * List the current user's registered passkeys.
   * Requires authentication.
   */
  listPasskeys(): Promise<PasskeyCredential[]>;

  /**
   * Rename a registered passkey.
   * Requires authentication.
   *
   * @param credentialId - The credential ID (wak_...)
   * @param newName - The new friendly name
   */
  renamePasskey(credentialId: string, newName: string): Promise<{ id: string; name: string }>;

  /**
   * Delete a registered passkey.
   * Requires authentication.
   *
   * @param credentialId - The credential ID (wak_...)
   */
  deletePasskey(credentialId: string): Promise<void>;

  /**
   * Check if passkeys (WebAuthn) are available in the current browser.
   * Returns true if a platform authenticator (Touch ID, Windows Hello, etc.) is available.
   */
  static isPasskeyAvailable(): Promise<boolean>;

  /**
   * Check if conditional mediation (passkey autofill) is available.
   * This enables "Sign in with a passkey" in browser autofill prompts.
   */
  static isConditionalMediationAvailable(): Promise<boolean>;
}

/** Info returned after registering a new passkey. */
export interface PasskeyCredentialInfo {
  id: string;
  name: string;
  is_synced: boolean;
  transports: string[];
}

/** A registered passkey credential (from list). */
export interface PasskeyCredential {
  id: string;
  name: string;
  created_at: string;
  last_used_at: string | null;
  is_synced: boolean;
  transports: string[];
}

export default GateAuthBridge;
