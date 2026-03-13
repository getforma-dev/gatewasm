// ---------------------------------------------------------------------------
// CreateTenant.ts — Simple form to create a new tenant
// ---------------------------------------------------------------------------

import { Card, CardHead } from '../../../core/components/Card';
import { Field } from '../../../core/components/Field';
import { Header } from '../../../core/components/Header';
import type { ConsoleStore } from '../store';

import { h, createSignal } from 'formajs';

const serverUrl = (window as any).__FORMA_SERVER_URL__ || window.location.origin;

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

export function CreateTenant(cs: ConsoleStore) {
  const { navigate, createTenant } = cs;

  const [name, setName] = createSignal('');
  const [slug, setSlug] = createSignal('');
  const [slugEdited, setSlugEdited] = createSignal(false);

  function handleNameInput(value: string) {
    setName(value);
    if (!slugEdited()) {
      setSlug(slugify(value));
    }
  }

  function handleSlugInput(value: string) {
    setSlugEdited(true);
    setSlug(value);
  }

  function handleSubmit() {
    const n = name().trim();
    const s = slug().trim();
    if (!n || !s) return;
    createTenant(n, s);
  }

  return h('div', { class: 'content' },
    Header({ title: 'New Tenant' }),
    Card({},
      h('div', { class: 'p-sm-v' },

        Field({
          label: 'Tenant Name',
          name: 'name',
          value: name,
          onChange: handleNameInput,
          placeholder: 'My SaaS App',
          required: true,
        }),

        Field({
          label: 'URL Slug',
          name: 'slug',
          value: slug,
          onChange: handleSlugInput,
          placeholder: 'my-saas-app',
          required: true,
          hint: 'Login page: ' + serverUrl + '/t/',
        }),

        h('small', { class: 'field-hint mb-md' },
          'Login URL: ',
          h('code', null, () => serverUrl + '/t/' + (slug() || 'your-slug') + '/login'),
        ),

        h('div', { class: 'flex gap-sm' },
          h('button', {
            class: 'btn primary',
            onClick: handleSubmit,
          }, 'Create Tenant'),
          h('button', {
            class: 'btn small',
            onClick: () => navigate('tenants'),
          }, 'Cancel'),
        ),
      ),
    ),
  );
}
