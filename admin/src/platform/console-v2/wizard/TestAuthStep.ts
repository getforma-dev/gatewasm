import { h, createSignal, createShow } from 'formajs';
import { createPoller } from '../../../core/hooks/createPoller';
import { pollTenantStats, updateTenant, type Tenant, type TenantStats } from '../store';

interface TestAuthStepProps {
  tenant: Tenant;
  onNext: () => void;
}

export function TestAuthStep({ tenant, onNext }: TestAuthStepProps) {
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const loginUrl = serverUrl + '/t/' + tenant.slug + '/login';
  const sandboxUrl = serverUrl + '/t/' + tenant.slug + '/sandbox';
  const [success, setSuccess] = createSignal(false);
  const [latestUser, setLatestUser] = createSignal<TenantStats['latest_user']>(null);

  // Auto-open login page
  window.open(loginUrl, '_blank');

  // Poll for first auth event
  const poller = createPoller({
    initialMs: 2000,
    backoffMs: 5000,
    backoffAfterMs: 30000,
    fn: async () => {
      const stats = await pollTenantStats(tenant.id);
      if (stats.total_users > 0 && stats.latest_user) {
        setLatestUser(stats.latest_user);
        setSuccess(true);
        return true; // stop polling
      }
      return false;
    },
  });
  poller.start();

  async function handleNext() {
    await updateTenant(tenant.id, { onboarding_step: 3 });
    onNext();
  }

  return h('div', { class: 'wizard-step' },
    h('div', { class: 'wizard-step-header' },
      h('div', { class: 'wizard-step-counter' }, 'Step 2 of 3'),
      h('h2', { class: 'wizard-step-title' },
        () => success() ? 'It works' : 'See it work',
      ),
      createShow(
        () => !success(),
        () => h('p', { class: 'wizard-step-subtitle' },
          'Open your login page in a new tab, register as a test user, and watch the magic.',
        ),
      ),
    ),
    h('div', { class: 'wizard-step-body' },
      // Waiting state
      createShow(
        () => !success(),
        () => h('div', { class: 'wizard-waiting' },
          h('div', { class: 'wizard-url-bar' },
            h('code', null, loginUrl),
            h('button', {
              class: 'btn btn-ghost btn-sm',
              onClick: () => navigator.clipboard.writeText(loginUrl),
            }, 'Copy'),
            h('a', {
              class: 'btn btn-primary btn-sm',
              href: loginUrl,
              target: '_blank',
              rel: 'noopener',
            }, 'Open login page \u2197'),
          ),
          h('div', { class: 'wizard-pulse' },
            h('div', { class: 'wizard-pulse-dot' }),
            h('p', { class: 'text-muted' }, 'Waiting for your first login...'),
            h('p', { class: 'text-muted-dim text-xs' },
              'Register on your login page. This console will advance automatically.',
            ),
          ),
        ),
      ),
      // Success state
      createShow(
        () => success(),
        () => h('div', { class: 'wizard-success' },
          h('div', { class: 'wizard-success-card' },
            h('div', { class: 'wizard-success-label' }, 'First user authenticated'),
            h('div', { class: 'wizard-success-email' }, () => latestUser()?.email || ''),
            h('div', { class: 'wizard-success-meta' },
              () => {
                const u = latestUser();
                if (!u) return '';
                return new Date(u.created_at).toLocaleString() + ' \u2014 ' + u.provider + ' provider';
              },
            ),
          ),
          h('a', {
            class: 'btn-link text-sm',
            href: sandboxUrl,
            target: '_blank',
            rel: 'noopener',
          }, 'See what your app receives \u2192'),
        ),
      ),
    ),
    // Footer only in success state
    createShow(
      () => success(),
      () => h('div', { class: 'wizard-step-footer' },
        h('button', {
          class: 'btn btn-primary',
          onClick: handleNext,
        }, 'Next \u2192'),
      ),
    ),
  );
}
