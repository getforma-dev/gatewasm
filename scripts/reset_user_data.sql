-- Reset all user/account runtime data while preserving tenant/config metadata.
-- Safe for local development resets; do not run on production unless intended.

BEGIN;

DO $$
DECLARE
    tables_to_clear text[] := ARRAY[
        'owner_timeline_events',
        'provider_billing_events',
        'owner_invites',
        'organization_members',
        'api_keys',
        'auth_codes',
        'oauth_states',
        'refresh_tokens',
        'sessions',
        'credentials',
        'user_roles',
        'platform_accounts',
        'login_attempts',
        'audit_log',
        'users'
    ];
    table_name text;
BEGIN
    FOREACH table_name IN ARRAY tables_to_clear LOOP
        IF to_regclass('public.' || table_name) IS NOT NULL THEN
            EXECUTE format('TRUNCATE TABLE %I RESTART IDENTITY CASCADE', table_name);
        END IF;
    END LOOP;
END $$;

COMMIT;
