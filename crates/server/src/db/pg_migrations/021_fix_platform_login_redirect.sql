-- Fix platform tenant login_redirect_url after /platform/dashboard → /platform/console rename
UPDATE tenants SET login_redirect_url = '/platform/console' WHERE slug = 'platform' AND login_redirect_url = '/platform/dashboard';
