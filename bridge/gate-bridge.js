/**
 * GateWASM Bridge — Universal Auth
 * Connects GateAuth WASM module to browser APIs.
 * Zero dependencies. ~3KB minified.
 *
 * Usage:
 *   const bridge = await GateAuthBridge.init({
 *     serverUrl: 'http://localhost:3099',
 *     clientId: 'demo-app',
 *   });
 *   bridge.onStateChange(({ state, user }) => console.log(state, user));
 *   await bridge.login('google');
 */

// ── Base64url <-> ArrayBuffer helpers ──────────────────────────────────────

function base64urlToBuffer(str) {
  const base64 = str.replace(/-/g, '+').replace(/_/g, '/');
  const pad = base64.length % 4 === 0 ? '' : '='.repeat(4 - (base64.length % 4));
  const binary = atob(base64 + pad);
  return Uint8Array.from(binary, c => c.charCodeAt(0)).buffer;
}

function bufferToBase64url(buffer) {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
}

class GateAuthBridge {
  #wasm = null;
  #config = null;
  #callbacks = new Map();
  #refreshTimer = null;
  #dpopKeyPair = null;
  #dpopEnabled = false;

  static async init(config) {
    const bridge = new GateAuthBridge();
    bridge.#config = config;
    bridge.#dpopEnabled = config.dpopEnabled || false;
    const wasmPath = config.wasmPath || new URL('./gatewasm_core.js', import.meta.url).href;
    const wasmModule = await import(wasmPath);
    await wasmModule.default();

    bridge.#wasm = new wasmModule.GateAuth({
      server_url: config.serverUrl,
      client_id: config.clientId,
      redirect_uri: config.redirectUri || window.location.origin + '/auth/callback',
      scopes: config.scopes || ['openid', 'profile', 'email'],
      auto_refresh: config.autoRefresh !== false,
      refresh_threshold_seconds: config.refreshThreshold || 300,
    });

    await bridge.#handlePossibleCallback();

    // If not authenticated after callback, try silent refresh via httpOnly cookie
    if (!bridge.isAuthenticated()) {
      try {
        const tokenUrl = `${config.serverUrl}/auth/token`;
        const headers = { 'Content-Type': 'application/json' };
        if (bridge.#dpopEnabled) {
          headers['DPoP'] = await bridge.#createDPoPProof('POST', tokenUrl);
        }
        const res = await fetch(tokenUrl, {
          method: 'POST',
          headers,
          body: JSON.stringify({ grant_type: 'refresh_token' }),
          credentials: 'include',
        });
        if (res.ok) {
          const tokens = await res.json();
          bridge.#wasm.processTokens(tokens);
          bridge.#emit({ state: 'authenticated', user: bridge.getUser() });
        }
      } catch (_) {
        // No valid cookie — user is not authenticated
      }
    }

    if (config.autoRefresh !== false) bridge.#startRefreshLoop();
    return bridge;
  }

  async login(provider = 'google', credentials = null) {
    const request = this.#wasm.prepareLogin(provider);

    // serde_wasm_bindgen may serialize internally-tagged enums as Map objects
    // where property access (request.type) returns undefined — use Map.get() as fallback
    const reqType = request.type ?? (request instanceof Map ? request.get('type') : undefined);
    const req = request instanceof Map ? Object.fromEntries(request) : request;

    switch (reqType) {
      case 'oauth_redirect': {
        // Unwrap nested params if also a Map
        const rawParams = req.params instanceof Map ? Object.fromEntries(req.params) : req.params;

        // Save PKCE data to sessionStorage before redirect (WASM memory won't survive page navigation)
        const verifier = this.#wasm.getCodeVerifier();
        const oauthState = this.#wasm.getOAuthState();
        const redirectUri = this.#config.redirectUri || window.location.origin + '/auth/callback';
        if (!verifier || !oauthState) {
          throw new Error(`Missing PKCE data: verifier=${!!verifier}, state=${!!oauthState}`);
        }
        sessionStorage.setItem('gatewasm_pkce', JSON.stringify({
          code_verifier: verifier,
          state: oauthState,
          server_url: this.#config.serverUrl,
          client_id: this.#config.clientId,
          redirect_uri: redirectUri,
        }));

        // Build query string from params
        const qs = new URLSearchParams();
        for (const [key, val] of Object.entries(rawParams)) {
          if (val != null) qs.set(key, String(val));
        }
        window.location.href = `${req.authorize_url}?${qs}`;
        break;
      }
      case 'email_password': {
        if (!credentials?.email || !credentials?.password)
          throw new Error('Email and password required');
        const url = credentials.isRegister ? req.register_url : req.login_url;
        const res = await fetch(url, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(credentials),
          credentials: 'include',
        });
        if (!res.ok) {
          const err = await res.json().catch(() => ({ message: 'Auth failed' }));
          this.#emit({ state: 'error', error: err });
          throw new Error(err.message || 'Authentication failed');
        }
        const tokens = await res.json();
        const user = this.#wasm.processTokens(tokens);
        this.#emit({ state: 'authenticated', user });
        return user;
      }
      case 'webauthn': {
        const cRes = await fetch(req.challenge_url, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          credentials: 'include',
        });
        const challengeData = await cRes.json();
        const cred = await navigator.credentials.get({ publicKey: challengeData.publicKey });
        const enc = b => btoa(String.fromCharCode(...new Uint8Array(b)));
        const vRes = await fetch(`${req.challenge_url}/verify`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            id: cred.id, rawId: enc(cred.rawId),
            response: {
              authenticatorData: enc(cred.response.authenticatorData),
              clientDataJSON: enc(cred.response.clientDataJSON),
              signature: enc(cred.response.signature),
            },
            type: cred.type,
          }),
          credentials: 'include',
        });
        const tokens = await vRes.json();
        const user = this.#wasm.processTokens(tokens);
        this.#emit({ state: 'authenticated', user });
        return user;
      }
      case 'saml_redirect':
        window.location.href = req.sso_url;
        break;
      default:
        throw new Error(`Unknown login request type: "${reqType}"`);
    }
  }

  async logout() {
    const request = this.#wasm.prepareLogout();
    try {
      await fetch(request.url, {
        method: request.method,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(request.body),
        credentials: 'include',
      });
    } catch (_) {}
    this.#stopRefreshLoop();
    this.#emit({ state: 'idle', user: null });
  }

  getUser() { return this.#wasm.getUser(); }

  async getToken() {
    await this.#tryRefresh();
    return this.#wasm.getToken();
  }

  isAuthenticated() { return this.#wasm.isAuthenticated(); }

  onStateChange(cb) {
    const id = crypto.randomUUID();
    this.#callbacks.set(id, cb);
    return () => this.#callbacks.delete(id);
  }

  hasRole(role) { return this.#wasm.hasRole(role); }

  async hasPermission(perm) {
    if (!this.isAuthenticated()) return false;
    if (!this._permissions) {
      const token = await this.getToken();
      if (!token) return false;
      try {
        const res = await fetch(`${this.#config.serverUrl}/auth/me`, {
          headers: { 'Authorization': `Bearer ${token}` },
          credentials: 'include',
        });
        if (res.ok) {
          const data = await res.json();
          this._permissions = data.permissions || [];
        }
      } catch { this._permissions = []; }
    }
    return (this._permissions || []).includes(perm);
  }

  // ── Passkey (WebAuthn) Methods ─────────────────────────────────────────

  /**
   * Register a new passkey for the currently authenticated user.
   * Requires the user to already be logged in (JWT is sent as Bearer token).
   *
   * @param {string} name - Friendly name for the credential (default: "My Passkey")
   * @returns {Promise<{id: string, name: string, is_synced: boolean, transports: string[]}>}
   */
  async registerPasskey(name = 'My Passkey') {
    const token = await this.getToken();
    if (!token) throw new Error('Must be authenticated to register a passkey');

    // Step 1: Begin registration — get challenge and options from server
    const beginRes = await fetch(`${this.#config.serverUrl}/auth/webauthn/register/begin`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify({ name }),
      credentials: 'include',
    });
    if (!beginRes.ok) {
      const err = await beginRes.json().catch(() => ({ message: 'Passkey registration failed' }));
      throw new Error(err.message || 'Failed to begin passkey registration');
    }
    const options = await beginRes.json();
    const challengeId = options.challenge_id;
    const publicKey = options.publicKey;

    // Step 2: Decode base64url fields for the browser WebAuthn API
    publicKey.challenge = base64urlToBuffer(publicKey.challenge);
    publicKey.user.id = base64urlToBuffer(publicKey.user.id);
    if (publicKey.excludeCredentials) {
      publicKey.excludeCredentials = publicKey.excludeCredentials.map(c => ({
        ...c,
        id: base64urlToBuffer(c.id),
      }));
    }

    // Step 3: Call the browser WebAuthn API
    const credential = await navigator.credentials.create({ publicKey });

    // Step 4: Encode response fields back to base64url for the server
    const attestationResponse = credential.response;
    const transports = typeof attestationResponse.getTransports === 'function'
      ? attestationResponse.getTransports()
      : [];

    const completeBody = {
      challenge_id: challengeId,
      credential_id: bufferToBase64url(credential.rawId),
      attestation_object: bufferToBase64url(attestationResponse.attestationObject),
      client_data_json: bufferToBase64url(attestationResponse.clientDataJSON),
      transports,
    };

    // Step 5: Complete registration on the server
    const completeRes = await fetch(`${this.#config.serverUrl}/auth/webauthn/register/complete`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify(completeBody),
      credentials: 'include',
    });
    if (!completeRes.ok) {
      const err = await completeRes.json().catch(() => ({ message: 'Passkey registration failed' }));
      throw new Error(err.message || 'Failed to complete passkey registration');
    }

    return await completeRes.json();
  }

  /**
   * Authenticate using a passkey. Does not require an existing session.
   * On success, processes tokens and emits an authenticated state change.
   *
   * @param {string} email - The user's email address
   * @returns {Promise<AuthUser>} The authenticated user
   */
  async loginWithPasskey(email) {
    if (!email) throw new Error('Email is required for passkey login');

    // Step 1: Begin login — get challenge from server
    const beginRes = await fetch(`${this.#config.serverUrl}/auth/webauthn/login/begin`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email }),
      credentials: 'include',
    });
    if (!beginRes.ok) {
      const err = await beginRes.json().catch(() => ({ message: 'Passkey login failed' }));
      this.#emit({ state: 'error', error: err });
      throw new Error(err.message || 'Failed to begin passkey login');
    }
    const options = await beginRes.json();
    const challengeId = options.challenge_id;
    const publicKey = options.publicKey;

    // Step 2: Decode base64url fields for the browser WebAuthn API
    publicKey.challenge = base64urlToBuffer(publicKey.challenge);
    if (publicKey.allowCredentials) {
      publicKey.allowCredentials = publicKey.allowCredentials.map(c => ({
        ...c,
        id: base64urlToBuffer(c.id),
      }));
    }

    // Step 3: Call the browser WebAuthn API
    const assertion = await navigator.credentials.get({ publicKey });

    // Step 4: Encode response fields back to base64url for the server
    const completeBody = {
      challenge_id: challengeId,
      credential_id: bufferToBase64url(assertion.rawId),
      authenticator_data: bufferToBase64url(assertion.response.authenticatorData),
      client_data_json: bufferToBase64url(assertion.response.clientDataJSON),
      signature: bufferToBase64url(assertion.response.signature),
    };

    // Step 5: Complete login on the server
    const completeRes = await fetch(`${this.#config.serverUrl}/auth/webauthn/login/complete`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(completeBody),
      credentials: 'include',
    });
    if (!completeRes.ok) {
      const err = await completeRes.json().catch(() => ({ message: 'Passkey login failed' }));
      this.#emit({ state: 'error', error: err });
      throw new Error(err.message || 'Failed to complete passkey login');
    }

    // Step 6: Process tokens (same pattern as email/password login)
    const tokens = await completeRes.json();
    const user = this.#wasm.processTokens(tokens);
    this.#emit({ state: 'authenticated', user });
    return user;
  }

  /**
   * List the current user's registered passkeys.
   * Requires authentication.
   *
   * @returns {Promise<Array<{id: string, name: string, created_at: string, last_used_at: string|null, is_synced: boolean, transports: string[]}>>}
   */
  async listPasskeys() {
    const token = await this.getToken();
    if (!token) throw new Error('Must be authenticated to list passkeys');

    const res = await fetch(`${this.#config.serverUrl}/auth/webauthn/credentials`, {
      headers: { 'Authorization': `Bearer ${token}` },
      credentials: 'include',
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ message: 'Failed to list passkeys' }));
      throw new Error(err.message || 'Failed to list passkeys');
    }

    const data = await res.json();
    return data.credentials;
  }

  /**
   * Rename a registered passkey.
   * Requires authentication.
   *
   * @param {string} credentialId - The credential ID (wak_...)
   * @param {string} newName - The new friendly name
   * @returns {Promise<{id: string, name: string}>}
   */
  async renamePasskey(credentialId, newName) {
    if (!credentialId || !newName) throw new Error('Credential ID and new name are required');

    const token = await this.getToken();
    if (!token) throw new Error('Must be authenticated to rename a passkey');

    const res = await fetch(`${this.#config.serverUrl}/auth/webauthn/credentials/${encodeURIComponent(credentialId)}`, {
      method: 'PATCH',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify({ name: newName }),
      credentials: 'include',
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ message: 'Failed to rename passkey' }));
      throw new Error(err.message || 'Failed to rename passkey');
    }

    return await res.json();
  }

  /**
   * Delete a registered passkey.
   * Requires authentication.
   *
   * @param {string} credentialId - The credential ID (wak_...)
   * @returns {Promise<void>}
   */
  async deletePasskey(credentialId) {
    if (!credentialId) throw new Error('Credential ID is required');

    const token = await this.getToken();
    if (!token) throw new Error('Must be authenticated to delete a passkey');

    const res = await fetch(`${this.#config.serverUrl}/auth/webauthn/credentials/${encodeURIComponent(credentialId)}`, {
      method: 'DELETE',
      headers: { 'Authorization': `Bearer ${token}` },
      credentials: 'include',
    });
    if (!res.ok && res.status !== 204) {
      const err = await res.json().catch(() => ({ message: 'Failed to delete passkey' }));
      throw new Error(err.message || 'Failed to delete passkey');
    }
  }

  /**
   * Check if passkeys (WebAuthn) are available in the current browser.
   * @returns {Promise<boolean>}
   */
  static async isPasskeyAvailable() {
    if (!window.PublicKeyCredential) return false;
    if (typeof PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable === 'function') {
      return await PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable();
    }
    return true;
  }

  /**
   * Check if conditional mediation (passkey autofill) is available.
   * This enables "Sign in with a passkey" in browser autofill prompts.
   * @returns {Promise<boolean>}
   */
  static async isConditionalMediationAvailable() {
    if (!window.PublicKeyCredential?.isConditionalMediationAvailable) return false;
    return await PublicKeyCredential.isConditionalMediationAvailable();
  }

  // ── DPoP (Demonstrating Proof of Possession) ────────────────────────────

  /**
   * Generate or retrieve the cached ECDSA P-256 keypair for DPoP proofs.
   * A fresh keypair is created on every page load (not persisted).
   */
  async #ensureDPoPKey() {
    if (!this.#dpopKeyPair) {
      this.#dpopKeyPair = await crypto.subtle.generateKey(
        { name: 'ECDSA', namedCurve: 'P-256' },
        true,  // extractable — need public key for JWK thumbprint
        ['sign']
      );
    }
    return this.#dpopKeyPair;
  }

  /**
   * Create a DPoP proof JWT (RFC 9449) bound to the given HTTP method and URL.
   * @param {string} method - HTTP method (e.g. 'POST', 'GET')
   * @param {string} url - The full URL of the request
   * @returns {Promise<string>} A signed dpop+jwt token
   */
  async #createDPoPProof(method, url) {
    const keyPair = await this.#ensureDPoPKey();
    const publicJwk = await crypto.subtle.exportKey('jwk', keyPair.publicKey);
    // Remove non-essential fields for a cleaner JWK in the proof header
    delete publicJwk.key_ops;
    delete publicJwk.ext;

    const header = { typ: 'dpop+jwt', alg: 'ES256', jwk: publicJwk };
    const payload = {
      jti: crypto.randomUUID(),
      htm: method.toUpperCase(),
      htu: url,
      iat: Math.floor(Date.now() / 1000),
    };

    const headerB64 = this.#base64urlEncode(JSON.stringify(header));
    const payloadB64 = this.#base64urlEncode(JSON.stringify(payload));
    const signingInput = new TextEncoder().encode(`${headerB64}.${payloadB64}`);

    const signature = await crypto.subtle.sign(
      { name: 'ECDSA', hash: 'SHA-256' },
      keyPair.privateKey,
      signingInput
    );

    const sigB64 = bufferToBase64url(new Uint8Array(signature));
    return `${headerB64}.${payloadB64}.${sigB64}`;
  }

  /**
   * Base64url-encode a string (used for DPoP JWT header/payload).
   * @param {string} str
   * @returns {string}
   */
  #base64urlEncode(str) {
    return btoa(str).replace(/=/g, '').replace(/\+/g, '-').replace(/\//g, '_');
  }

  /**
   * Make an authenticated fetch call with DPoP proof attached (if enabled).
   * Automatically includes the Bearer token and DPoP header.
   *
   * @param {string} url - The URL to fetch
   * @param {RequestInit} [options={}] - Fetch options (method, body, etc.)
   * @returns {Promise<Response>}
   */
  async fetchWithDPoP(url, options = {}) {
    const method = (options.method || 'GET').toUpperCase();
    const headers = new Headers(options.headers || {});

    const token = await this.getToken();
    if (token) {
      headers.set('Authorization', `Bearer ${token}`);
    }

    if (this.#dpopEnabled) {
      headers.set('DPoP', await this.#createDPoPProof(method, url));
    }

    return fetch(url, {
      ...options,
      method,
      headers,
      credentials: options.credentials || 'include',
    });
  }

  async #handlePossibleCallback() {
    const params = new URLSearchParams(window.location.search);
    const code = params.get('code'), state = params.get('state');
    const error = params.get('error'), errorDesc = params.get('error_description');

    // Handle error redirects from server (provider denied consent, etc.)
    if (error) {
      this.#emit({ state: 'error', error: { message: errorDesc || error } });
      window.history.replaceState({}, '', window.location.pathname);
      return;
    }

    if (!code || !state) return;

    // Restore PKCE data saved before the OAuth redirect
    const pkceRaw = sessionStorage.getItem('gatewasm_pkce');
    sessionStorage.removeItem('gatewasm_pkce');

    if (!pkceRaw) {
      this.#emit({ state: 'error', error: { message: 'No PKCE session data found after OAuth redirect' } });
      window.history.replaceState({}, '', window.location.pathname);
      return;
    }

    let pkce;
    try {
      pkce = JSON.parse(pkceRaw);
    } catch (e) {
      this.#emit({ state: 'error', error: { message: 'Corrupt PKCE session data' } });
      window.history.replaceState({}, '', window.location.pathname);
      return;
    }

    if (pkce.state !== state) {
      this.#emit({ state: 'error', error: { message: 'OAuth state mismatch' } });
      window.history.replaceState({}, '', window.location.pathname);
      return;
    }

    // Exchange the internal auth code directly (bypass WASM's prepareTokenExchange since memory was wiped by redirect)
    const tokenUrl = `${pkce.server_url}/auth/token`;
    const tokenBody = {
      grant_type: 'authorization_code',
      code,
      code_verifier: pkce.code_verifier,
      client_id: pkce.client_id,
      redirect_uri: pkce.redirect_uri,
    };

    try {
      const res = await fetch(tokenUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(tokenBody),
        credentials: 'include',
      });

      if (res.ok) {
        const tokens = await res.json();
        const user = this.#wasm.processTokens(tokens);
        this.#emit({ state: 'authenticated', user });
        window.history.replaceState({}, '', window.location.pathname);
      } else {
        const err = await res.json().catch(() => ({ message: `Token exchange failed (HTTP ${res.status})` }));
        this.#emit({ state: 'error', error: err });
        window.history.replaceState({}, '', window.location.pathname);
      }
    } catch (e) {
      this.#emit({ state: 'error', error: { message: `Network error: ${e.message}` } });
      window.history.replaceState({}, '', window.location.pathname);
    }
  }

  async #tryRefresh() {
    const req = this.#wasm.checkRefresh();
    if (req) {
      // WASM has a refresh token in memory — use it
      const headers = { 'Content-Type': 'application/json' };
      if (this.#dpopEnabled) {
        headers['DPoP'] = await this.#createDPoPProof(req.method, req.url);
      }
      const res = await fetch(req.url, {
        method: req.method,
        headers,
        body: JSON.stringify(req.body),
        credentials: 'include',
      });
      if (res.ok) {
        this.#wasm.processTokens(await res.json());
      } else {
        this.#wasm.prepareLogout();
        this.#emit({ state: 'idle', user: null });
      }
      return;
    }

    // Fallback: no in-memory token, try httpOnly cookie
    try {
      const tokenUrl = `${this.#config.serverUrl}/auth/token`;
      const headers = { 'Content-Type': 'application/json' };
      if (this.#dpopEnabled) {
        headers['DPoP'] = await this.#createDPoPProof('POST', tokenUrl);
      }
      const res = await fetch(tokenUrl, {
        method: 'POST',
        headers,
        body: JSON.stringify({ grant_type: 'refresh_token' }),
        credentials: 'include',
      });
      if (res.ok) {
        this.#wasm.processTokens(await res.json());
      }
    } catch (_) {}
  }

  #startRefreshLoop() { this.#refreshTimer = setInterval(() => this.#tryRefresh(), 60000); }
  #stopRefreshLoop() { if (this.#refreshTimer) { clearInterval(this.#refreshTimer); this.#refreshTimer = null; } }
  #emit(data) { this.#callbacks.forEach(cb => { try { cb(data); } catch (e) { console.error('GateAuth callback error:', e); } }); }
}

export default GateAuthBridge;
