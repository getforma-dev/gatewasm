import { h, createShow, createSignal, createSwitch } from 'formajs';
import {
  selectedTenant,
  integrateSection,
  setIntegrateSection,
  webhooks,
  webhooksLoading,
  createWebhook,
  updateWebhook,
  deleteWebhook,
  loadWebhookDeliveries,
  testWebhook,
  canEdit,
  type IntegrateSection,
  type Webhook,
  type WebhookDelivery,
  serverUrl,
} from '../store';
import { SessionInspectorPanel } from './SessionInspectorPanel';
import { CopyButton } from '../../../core/components/ui/CopyButton';

export function IntegrateView() {
  const tenant = selectedTenant()!;
  const tenantState = () => selectedTenant() || tenant;

  const loginUrl = () => serverUrl + '/t/' + tenantState().slug + '/login';
  const previewLoginUrl = () => loginUrl() + '?preview=1';
  const siteUrl = () => serverUrl + '/t/' + tenantState().slug + '/site';

  const bridgeSnippet = () => {
    const currentTenant = tenantState();
    return `<script src="${serverUrl}/bridge.js"><\/script>
<script>
  GateAuthBridge.init({
    serverUrl: '${serverUrl}',
    slug: '${currentTenant.slug}',
    onStateChange: ({ state, user }) => {
      if (state === 'authenticated') {
        console.log('Hello,', user.name);
      }
    }
  });
<\/script>`;
  };

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Operator'),
        h('h2', { class: 'view-title' }, 'Integrate'),
        h('p', { class: 'view-subtitle text-muted' }, () => 'Everything you need to wire auth into /t/' + tenantState().slug),
      ),
      h('p', { class: 'view-header-note' },
        'Keep setup and debugging in one place: integrate the hosted login flow, then inspect the live tenant session without leaving the workspace.',
      ),
    ),
    h('section', { class: 'zone' },
      h('div', { class: 'zone-head' },
        h('div', { class: 'zone-label' }, 'Integration Workspace'),
        h('p', { class: 'zone-copy' },
          'Use the setup tab when you are wiring URLs and credentials. Switch to Auth Reference to see the payload shape, token format, and endpoints your app will receive.',
        ),
      ),
      h('div', { class: 'console-subtabs', role: 'tablist', 'aria-label': 'Integrate sections' },
        IntegrateTab('guide', 'Getting Started'),
        IntegrateTab('session-inspector', 'Auth Reference'),
        IntegrateTab('webhooks', 'Webhooks'),
      ),
      h('div', { class: 'integrate-pane' },
        createSwitch(
          integrateSection,
          [
            {
              match: 'guide',
              render: () => GuidePanel({
                bridgeSnippet,
                loginUrl,
                previewLoginUrl,
                siteUrl,
                tenantState,
                openInspector: () => setIntegrateSection('session-inspector'),
              }),
            },
            { match: 'session-inspector', render: () => SessionInspectorPanel() },
            { match: 'webhooks', render: () => WebhooksPanel() },
          ],
        ),
      ),
    ),
  );
}

function IntegrateTab(id: IntegrateSection, label: string) {
  return h('button', {
    type: 'button',
    role: 'tab',
    class: () => 'console-subtab' + (integrateSection() === id ? ' active' : ''),
    'aria-selected': () => String(integrateSection() === id),
    onClick: () => setIntegrateSection(id),
  }, label);
}

function GuidePanel({
  bridgeSnippet,
  loginUrl,
  previewLoginUrl,
  siteUrl,
  tenantState,
  openInspector,
}: {
  bridgeSnippet: () => string;
  loginUrl: () => string;
  previewLoginUrl: () => string;
  siteUrl: () => string;
  tenantState: () => { management_api_key?: string; name: string; slug: string };
  openInspector: () => void;
}) {
  return h('div', { class: 'integrate-guide-layout' },
    h('div', { class: 'integrate-main dashboard-stack' },
      h('div', { class: 'console-card integrate-card' },
        h('div', { class: 'step-label' }, 'Step 1'),
        h('h3', { class: 'section-title' }, 'Load the bridge script'),
        h('p', { class: 'card-copy' }, 'Drop this snippet into your app shell so you can observe auth state changes and access the signed-in user payload.'),
        h('pre', { class: 'code-block' }, bridgeSnippet),
        h('div', { class: 'card-actions' },
          CopyButton({ text: bridgeSnippet, label: 'Copy snippet' }),
        ),
      ),
      h('div', { class: 'console-card integrate-card' },
        h('div', { class: 'step-label' }, 'Step 2'),
        h('h3', { class: 'section-title' }, 'Redirect to your login page'),
        h('p', { class: 'card-copy' }, 'Use this URL whenever a user needs to authenticate before returning to your app.'),
        h('div', { class: 'inline-copy-row' },
          h('code', { class: 'code-block code-inline-block' }, loginUrl),
          CopyButton({ text: loginUrl, label: 'Copy URL' }),
        ),
        h('div', { class: 'card-actions' },
          h('a', {
            class: 'btn btn-sm btn-ghost',
            href: previewLoginUrl,
            target: '_blank',
          }, 'Preview login page'),
        ),
      ),
    ),
    h('aside', { class: 'integrate-aside dashboard-stack' },
      h('div', { class: 'console-card integrate-summary-card' },
        h('div', { class: 'card-label' }, 'Tenant Endpoints'),
        h('h3', { class: 'section-title' }, 'Everything you need for QA'),
        h('p', { class: 'card-copy' },
          'Keep your most useful tenant links together so you can test the hosted flow without leaving the setup screen.',
        ),
        h('div', { class: 'surface-link-list' },
          EndpointRow('Login URL', loginUrl(), {
            label: 'Open',
            href: loginUrl(),
          }),
          EndpointRow('Login Preview', previewLoginUrl(), {
            label: 'Open',
            href: previewLoginUrl(),
          }),
          EndpointRow('Protected Site', siteUrl(), {
            label: 'Open',
            href: siteUrl(),
          }),
        ),
        h('div', { class: 'card-actions' },
          h('button', {
            type: 'button',
            class: 'btn btn-primary btn-sm',
            onClick: openInspector,
          }, 'Open Auth Reference'),
        ),
      ),
      h('div', { class: 'console-card integrate-card' },
        h('div', { class: 'step-label' }, 'Step 3'),
        h('h3', { class: 'section-title' }, 'Use the management key on your backend'),
        h('p', { class: 'card-copy' }, 'Keep this key server-side only when you need to manage users, roles, or webhooks via the Management API.'),
        createShow(
          () => !!tenantState().management_api_key,
          () => ManagementKeyCard(tenantState().management_api_key || ''),
        ),
        createShow(
          () => !tenantState().management_api_key,
          () => h('p', { class: 'card-copy' },
            'The key is only shown at creation time. Rotate it if you need a fresh credential.',
          ),
        ),
      ),
    ),
  );
}

function EndpointRow(
  label: string,
  value: string,
  action: { label: string; href?: string; onClick?: () => void },
) {
  return h('div', { class: 'surface-link-row' },
    h('div', { class: 'surface-link-copy' },
      h('span', { class: 'detail-label' }, label),
      h('code', { class: 'surface-link-value' }, value),
    ),
    h('div', { class: 'surface-link-actions' },
      action.href
        ? h('a', {
            href: action.href,
            target: '_blank',
            class: 'btn btn-sm btn-ghost',
          }, action.label)
        : h('button', {
            type: 'button',
            class: 'btn btn-sm btn-ghost',
            onClick: action.onClick,
          }, action.label),
      CopyButton({ text: value, iconOnly: true, title: 'Copy ' + label.toLowerCase() }),
    ),
  );
}

function ManagementKeyCard(key: string) {
  const [reveal, setReveal] = createSignal(false);

  return h('div', { class: 'stack-sm' },
    () => reveal()
      ? h('code', { class: 'code-block code-inline-block break-all' }, key)
      : null,
    h('div', { class: 'card-actions' },
      h('button', {
        class: 'btn btn-sm btn-ghost',
        onClick: () => {
          setReveal(!reveal());
        },
      }, () => reveal() ? 'Hide key' : 'Reveal key'),
      () => reveal()
        ? CopyButton({ text: key, label: 'Copy key' })
        : null,
    ),
  );
}

// ── Webhooks Panel ─────────────────────────────────────────────────────

const WEBHOOK_EVENTS = [
  'user.registered',
  'user.logged_in',
  'user.logged_out',
  'user.deleted',
  'user.email_verified',
  'user.password_reset',
  'user.role_changed',
];

function WebhooksPanel() {
  const tenant = selectedTenant()!;
  const [showCreate, setShowCreate] = createSignal(false);
  const [createdSecret, setCreatedSecret] = createSignal<string | null>(null);
  const [expandedId, setExpandedId] = createSignal<string | null>(null);

  return h('div', { class: 'dashboard-stack' },
    h('div', { class: 'console-card' },
      h('div', { class: 'card-head-row' },
        h('div', null,
          h('h3', { class: 'section-title' }, 'Webhooks'),
          h('p', { class: 'card-copy' },
            'Receive real-time HTTP callbacks when events occur in your tenant.',
          ),
        ),
        h('button', {
          type: 'button',
          class: () => showCreate() ? 'btn btn-sm btn-ghost' : 'clip-btn',
          disabled: () => !canEdit(),
          onClick: () => { setShowCreate(!showCreate()); setCreatedSecret(null); },
        }, () => showCreate() ? 'Cancel' : 'Create Webhook'),
      ),
      createShow(
        () => createdSecret() !== null,
        () => h('div', { class: 'toast-banner toast-success' },
          h('strong', null, 'Signing secret (shown once): '),
          h('code', { class: 'break-all' }, () => createdSecret()!),
          h('div', { class: 'card-actions' },
            CopyButton({ text: () => createdSecret()!, label: 'Copy secret' }),
          ),
        ),
      ),
      createShow(showCreate, () => WebhookCreateForm(tenant.id, (secret: string) => {
        setShowCreate(false);
        setCreatedSecret(secret);
      })),
      createShow(webhooksLoading, () => h('p', { class: 'text-muted' }, 'Loading webhooks...')),
      createShow(
        () => !webhooksLoading() && webhooks().length === 0 && !showCreate(),
        () => h('p', { class: 'text-muted' }, 'No webhooks configured yet.'),
      ),
      () => {
        const list = webhooks();
        if (list.length === 0) return null;
        return h('div', { class: 'data-table-wrap' },
          h('table', { class: 'data-table' },
            h('thead', null,
              h('tr', null,
                h('th', null, 'URL'),
                h('th', null, 'Events'),
                h('th', null, 'Status'),
                h('th', null, 'Created'),
                h('th', null, ''),
              ),
            ),
            h('tbody', null,
              ...list.map((wh) => WebhookRow(wh, tenant.id, expandedId, setExpandedId)),
            ),
          ),
        );
      },
    ),
  );
}

function WebhookCreateForm(tenantId: string, onCreated: (secret: string) => void) {
  const [url, setUrl] = createSignal('');
  const [description, setDescription] = createSignal('');
  const [selectedEvents, setSelectedEvents] = createSignal<string[]>([
    'user.registered',
    'user.logged_in',
  ]);
  const [submitting, setSubmitting] = createSignal(false);

  const toggleEvent = (ev: string) => {
    const current = selectedEvents();
    if (current.includes(ev)) {
      setSelectedEvents(current.filter((e) => e !== ev));
    } else {
      setSelectedEvents([...current, ev]);
    }
  };

  const handleSubmit = async () => {
    if (!url().trim() || selectedEvents().length === 0) return;
    setSubmitting(true);
    const result = await createWebhook(tenantId, {
      url: url().trim(),
      events: selectedEvents(),
      description: description().trim() || undefined,
    });
    setSubmitting(false);
    if (result?.webhook?.secret) {
      onCreated(result.webhook.secret);
      setUrl('');
      setDescription('');
    }
  };

  return h('div', { class: 'console-card stack-sm webhook-create-form' },
    h('div', { class: 'form-group' },
      h('label', { class: 'form-label' }, 'Endpoint URL'),
      h('input', {
        type: 'url',
        class: 'form-input',
        placeholder: 'https://example.com/webhook',
        value: url,
        onInput: (e: Event) => setUrl((e.target as HTMLInputElement).value),
      }),
    ),
    h('div', { class: 'form-group' },
      h('label', { class: 'form-label' }, 'Description (optional)'),
      h('input', {
        type: 'text',
        class: 'form-input',
        placeholder: 'e.g. Production user events',
        value: description,
        onInput: (e: Event) => setDescription((e.target as HTMLInputElement).value),
      }),
    ),
    h('div', { class: 'form-group' },
      h('label', { class: 'form-label' }, 'Events'),
      h('div', { class: 'webhook-events-grid' },
        ...WEBHOOK_EVENTS.map((ev) =>
          h('label', { class: 'webhook-event-check' },
            h('input', {
              type: 'checkbox',
              checked: () => selectedEvents().includes(ev),
              onChange: () => toggleEvent(ev),
            }),
            h('span', null, ev),
          ),
        ),
      ),
    ),
    h('div', { class: 'card-actions' },
      h('button', {
        type: 'button',
        class: 'btn btn-primary btn-sm',
        disabled: () => submitting() || !url().trim() || selectedEvents().length === 0,
        onClick: handleSubmit,
      }, () => submitting() ? 'Creating...' : 'Create Webhook'),
    ),
  );
}

function WebhookRow(
  wh: Webhook,
  tenantId: string,
  expandedId: () => string | null,
  setExpandedId: (id: string | null) => void,
) {
  const isExpanded = () => expandedId() === wh.id;
  const eventsLabel = () => {
    const evts: string[] = Array.isArray(wh.events) ? wh.events : [];
    return evts.length <= 2 ? evts.join(', ') : evts.slice(0, 2).join(', ') + ' +' + (evts.length - 2);
  };

  const createdDate = new Date(wh.created_at).toLocaleDateString();

  return [
    h('tr', {
      class: () => 'clickable-row' + (isExpanded() ? ' expanded' : ''),
      onClick: () => setExpandedId(isExpanded() ? null : wh.id),
    },
      h('td', null,
        h('code', { class: 'text-sm break-all' }, wh.url),
      ),
      h('td', null, h('span', { class: 'text-sm text-muted' }, eventsLabel)),
      h('td', null,
        h('span', {
          class: () => 'status-badge ' + (wh.active ? 'status-active' : 'status-inactive'),
        }, wh.active ? 'Active' : 'Inactive'),
      ),
      h('td', { class: 'text-muted text-sm' }, createdDate),
      h('td', null,
        h('button', {
          type: 'button',
          class: 'btn btn-xs btn-ghost',
          onClick: (e: Event) => { e.stopPropagation(); setExpandedId(isExpanded() ? null : wh.id); },
        }, () => isExpanded() ? 'Collapse' : 'Expand'),
      ),
    ),
    createShow(isExpanded, () =>
      h('tr', { class: 'expanded-detail-row' },
        h('td', { colSpan: 5 },
          WebhookDetail(wh, tenantId),
        ),
      ),
    ),
  ] as unknown as HTMLElement;
}

function WebhookDetail(wh: Webhook, tenantId: string) {
  const [deliveries, setDeliveries] = createSignal<WebhookDelivery[]>([]);
  const [loadingDeliveries, setLoadingDeliveries] = createSignal(true);
  const [testing, setTesting] = createSignal(false);
  const [toggling, setToggling] = createSignal(false);
  const [deleting, setDeleting] = createSignal(false);

  // Load deliveries on mount
  void (async () => {
    const data = await loadWebhookDeliveries(tenantId, wh.id);
    setDeliveries(data);
    setLoadingDeliveries(false);
  })();

  const handleTest = async () => {
    setTesting(true);
    await testWebhook(tenantId, wh.id);
    // Refresh deliveries
    const data = await loadWebhookDeliveries(tenantId, wh.id);
    setDeliveries(data);
    setTesting(false);
  };

  const handleToggle = async () => {
    setToggling(true);
    await updateWebhook(tenantId, wh.id, { active: !wh.active });
    setToggling(false);
  };

  const handleDelete = async () => {
    setDeleting(true);
    await deleteWebhook(tenantId, wh.id);
    setDeleting(false);
  };

  const evts: string[] = Array.isArray(wh.events) ? wh.events : [];

  return h('div', { class: 'webhook-detail stack-sm' },
    h('div', { class: 'detail-grid compact' },
      h('span', { class: 'detail-label' }, 'URL'),
      h('code', { class: 'break-all' }, wh.url),
      h('span', { class: 'detail-label' }, 'Events'),
      h('span', null, evts.join(', ')),
      h('span', { class: 'detail-label' }, 'Description'),
      h('span', null, wh.description || '(none)'),
      h('span', { class: 'detail-label' }, 'Status'),
      h('span', {
        class: () => 'status-badge ' + (wh.active ? 'status-active' : 'status-inactive'),
      }, wh.active ? 'Active' : 'Inactive'),
    ),
    h('div', { class: 'card-actions' },
      h('button', {
        type: 'button',
        class: 'btn btn-sm btn-ghost',
        disabled: () => testing() || !canEdit(),
        onClick: handleTest,
      }, () => testing() ? 'Sending...' : 'Send Test'),
      h('button', {
        type: 'button',
        class: 'btn btn-sm btn-ghost',
        disabled: () => toggling() || !canEdit(),
        onClick: handleToggle,
      }, () => toggling() ? 'Updating...' : (wh.active ? 'Deactivate' : 'Activate')),
      createShow(() => canEdit(), () =>
        h('button', {
          type: 'button',
          class: 'btn btn-sm btn-danger',
          disabled: deleting,
          onClick: handleDelete,
        }, () => deleting() ? 'Deleting...' : 'Delete'),
      ),
    ),
    h('div', { class: 'webhook-deliveries' },
      h('h4', { class: 'section-subtitle' }, 'Recent Deliveries'),
      createShow(loadingDeliveries, () => h('p', { class: 'text-muted text-sm' }, 'Loading deliveries...')),
      createShow(
        () => !loadingDeliveries() && deliveries().length === 0,
        () => h('p', { class: 'text-muted text-sm' }, 'No deliveries yet.'),
      ),
      () => {
        const items = deliveries();
        if (items.length === 0) return null;
        return h('table', { class: 'data-table data-table-compact' },
          h('thead', null,
            h('tr', null,
              h('th', null, 'Event'),
              h('th', null, 'Status'),
              h('th', null, 'HTTP'),
              h('th', null, 'Attempt'),
              h('th', null, 'Time'),
            ),
          ),
          h('tbody', null,
            ...items.map((d) =>
              h('tr', null,
                h('td', null, h('code', { class: 'text-sm' }, d.event_type)),
                h('td', null,
                  h('span', {
                    class: 'status-badge ' + (d.status === 'delivered' ? 'status-active' : 'status-error'),
                  }, d.status),
                ),
                h('td', { class: 'text-sm text-muted' }, d.response_status != null ? String(d.response_status) : '-'),
                h('td', { class: 'text-sm text-muted' }, d.attempt + '/' + d.max_attempts),
                h('td', { class: 'text-sm text-muted' },
                  new Date(d.created_at).toLocaleString(),
                ),
              ),
            ),
          ),
        );
      },
    ),
  );
}
