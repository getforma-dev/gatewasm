// ---------------------------------------------------------------------------
// BillingView.ts — Billing tab: summary cards, payment form, filters, table
// ---------------------------------------------------------------------------

import { h, createSignal } from 'formajs';
import { StatCard } from '../../../core/components/StatCard';
import { FilterBar } from '../../../core/components/FilterBar';
import { DataTable } from '../../../core/components/DataTable';
import { Pager } from '../../../core/components/Pager';
import { Pill } from '../../../core/components/Pill';
import { Panel, PanelTitle } from '../../../core/components/Panel';
import { money, fmtDate } from '../../../core/utils';
import type { OwnerStore } from '../store';
import type { Payment, FilterField } from '../../../core/types';

// -- Component --------------------------------------------------------------

export function BillingView(os: OwnerStore) {
  const { store, paymentFilters, paymentPager } = os;
  const { applyPaymentFilters, clearPaymentFilters, changePaymentPage, exportCsv, createPayment } = os;

  // Payment form signals
  const [formCustomer, setFormCustomer] = createSignal('');
  const [formTenant, setFormTenant] = createSignal('');
  const [formAmount, setFormAmount] = createSignal('');
  const [formStatus, setFormStatus] = createSignal('paid');
  const [formCurrency, setFormCurrency] = createSignal('usd');
  const [formInvoice, setFormInvoice] = createSignal('');
  const [formReceipt, setFormReceipt] = createSignal('');
  const [formNotes, setFormNotes] = createSignal('');

  function handleSubmit(e: Event) {
    e.preventDefault();
    createPayment({
      customer_user_id: formCustomer(),
      tenant_id: formTenant() || undefined,
      amount: formAmount(),
      status: formStatus(),
      currency: formCurrency(),
      invoice: formInvoice() || undefined,
      receipt_url: formReceipt() || undefined,
      notes: formNotes() || undefined,
    }).then(() => {
      setFormCustomer('');
      setFormTenant('');
      setFormAmount('');
      setFormStatus('paid');
      setFormCurrency('usd');
      setFormInvoice('');
      setFormReceipt('');
      setFormNotes('');
    });
  }

  // Summary stat cards
  const summary = h('div', { class: 'stat-grid' },
    StatCard({ label: 'Paid', value: () => money(store.paymentSummary?.paid ?? 0) }),
    StatCard({ label: 'Pending', value: () => money(store.paymentSummary?.pending ?? 0) }),
    StatCard({ label: 'Refunded', value: () => money(store.paymentSummary?.refunded ?? 0) }),
    StatCard({ label: 'Failed', value: () => money(store.paymentSummary?.failed ?? 0) }),
    StatCard({ label: 'Records', value: () => store.payments.length }),
  );

  // Payment create form
  const paymentForm = Panel({},
    PanelTitle(
      h('span', null, 'Record Payment / Receipt'),
      h('span', { class: 'text-muted' }, 'Manual settlement log'),
    ),
    h('form', { class: 'form-grid', onSubmit: handleSubmit },
      h('input', {
        name: 'customer_user_id',
        placeholder: 'Customer user id (usr_...)',
        value: formCustomer,
        onInput: (e: Event) => setFormCustomer((e.target as HTMLInputElement).value),
      }),
      h('input', {
        name: 'tenant_id',
        placeholder: 'Tenant id (optional)',
        value: formTenant,
        onInput: (e: Event) => setFormTenant((e.target as HTMLInputElement).value),
      }),
      h('input', {
        name: 'amount',
        type: 'number',
        step: '0.01',
        min: '0',
        placeholder: 'Amount in USD',
        value: formAmount,
        onInput: (e: Event) => setFormAmount((e.target as HTMLInputElement).value),
      }),
      h('select', {
        name: 'status',
        value: formStatus,
        onChange: (e: Event) => setFormStatus((e.target as HTMLSelectElement).value),
      },
        h('option', { value: 'paid' }, 'Paid'),
        h('option', { value: 'pending' }, 'Pending'),
        h('option', { value: 'failed' }, 'Failed'),
        h('option', { value: 'refunded' }, 'Refunded'),
      ),
      h('input', {
        name: 'currency',
        placeholder: 'Currency',
        value: formCurrency,
        onInput: (e: Event) => setFormCurrency((e.target as HTMLInputElement).value),
      }),
      h('input', {
        name: 'invoice',
        placeholder: 'Invoice ID (optional)',
        value: formInvoice,
        onInput: (e: Event) => setFormInvoice((e.target as HTMLInputElement).value),
      }),
      h('input', {
        name: 'receipt_url',
        placeholder: 'Receipt URL (optional)',
        value: formReceipt,
        onInput: (e: Event) => setFormReceipt((e.target as HTMLInputElement).value),
      }),
      h('textarea', {
        name: 'notes',
        rows: 2,
        placeholder: 'Notes (optional)',
        value: formNotes,
        onInput: (e: Event) => setFormNotes((e.target as HTMLTextAreaElement).value),
      }),
      h('button', { class: 'btn primary', type: 'submit' }, 'Record Event'),
    ),
  );

  // Filter bar
  const fields: FilterField[] = [
    {
      name: 'status', label: 'Status', type: 'select',
      value: () => paymentFilters.filters().status,
      onChange: (v) => paymentFilters.update({ status: v } as any),
      options: () => [
        { value: 'paid', label: 'Paid' },
        { value: 'pending', label: 'Pending' },
        { value: 'failed', label: 'Failed' },
        { value: 'refunded', label: 'Refunded' },
      ],
    },
    {
      name: 'customer_user_id', label: 'Customer', type: 'text',
      value: () => paymentFilters.filters().customer_user_id,
      onChange: (v) => paymentFilters.update({ customer_user_id: v } as any),
      placeholder: 'Customer user id',
    },
    {
      name: 'from', label: 'From', type: 'datetime',
      value: () => paymentFilters.filters().from,
      onChange: (v) => paymentFilters.update({ from: v } as any),
    },
    {
      name: 'to', label: 'To', type: 'datetime',
      value: () => paymentFilters.filters().to,
      onChange: (v) => paymentFilters.update({ to: v } as any),
    },
  ];

  const filterBar = Panel({},
    PanelTitle(h('span', null, 'Payment Filters')),
    FilterBar({
      id: 'payment-filters',
      fields,
      onApply: () => applyPaymentFilters(),
      onClear: () => clearPaymentFilters(),
      onSave: () => paymentFilters.save(),
      onLoad: () => { paymentFilters.load(); applyPaymentFilters(); },
      onExport: () => exportCsv('payments'),
    }),
  );

  // Payment table
  const table = DataTable<Payment>({
    headers: ['ID', 'Customer', 'Tenant', 'Amount', 'Status', 'Invoice', 'Receipt', 'Created'],
    rows: () => store.payments,
    keyFn: (p) => p.id,
    renderRow: (p) => h('tr', null,
      h('td', null, h('code', null, p.id)),
      h('td', null, p.customer_email || p.customer_user_id || '-'),
      h('td', null, p.tenant_slug || '-'),
      h('td', null,
        money(p.amount_cents),
        ' ',
        h('span', { class: 'text-muted' }, (p.currency || 'usd').toUpperCase()),
      ),
      h('td', null, Pill(p.status)),
      h('td', null,
        p.external_invoice_id ? h('code', null, p.external_invoice_id) : '-',
      ),
      h('td', null,
        p.receipt_url
          ? h('a', { href: p.receipt_url, target: '_blank', rel: 'noopener' }, 'Receipt')
          : '-',
      ),
      h('td', null, fmtDate(p.created_at)),
    ),
    emptyText: 'No payment events',
  });

  const pager = Pager({
    total: () => store.paymentTotal,
    page: () => paymentPager.page(),
    onNavigate: (dir) => changePaymentPage(dir),
  });

  return h('div', { class: 'content' },
    h('div', { class: 'view-section' }, summary),
    h('div', { class: 'view-section' },
      h('div', { class: 'split billing-split' }, paymentForm, filterBar),
    ),
    h('div', { class: 'view-section' },
      Panel({},
        PanelTitle(
          h('span', null, 'Payment Events'),
          h('span', { class: 'text-muted' }, 'Receipts and invoice references'),
        ),
        table,
        pager,
      ),
    ),
  );
}
