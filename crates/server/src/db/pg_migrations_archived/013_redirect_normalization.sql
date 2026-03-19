-- Migration 013: normalize login redirect URLs
--
-- Removes legacy localhost callback defaults and enforces tenant dashboard defaults.

UPDATE tenants
SET login_redirect_url = '/t/' || slug || '/dashboard'
WHERE login_redirect_url IS NULL
   OR trim(login_redirect_url) = ''
   OR login_redirect_url = 'http://localhost:3000/callback'
   OR login_redirect_url LIKE 'http://localhost:%/callback'
   OR login_redirect_url LIKE 'http://127.0.0.1:%/callback';
