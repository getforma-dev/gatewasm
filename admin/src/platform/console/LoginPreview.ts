/**
 * LoginPreview — native reactive mock of the tenant login page
 * wrapped in a mini browser chrome for a polished preview feel.
 */
import { h, createShow } from 'formajs';
import { GoogleIcon, GitHubIcon, EyeIcon } from '../../core/components/ui/BrandIcons';
import { sanitizeCSS } from './sanitizeCSS';

interface LoginPreviewProps {
  appName: () => string;
  logoUrl: () => string;
  primaryColor: () => string;
  darkMode: () => boolean;
  buttonText: () => string;
  providers: () => string[];
  slug?: () => string;
  bgColor?: () => string;
  cardColor?: () => string;
  customCSS?: () => string;
}

// Token-accurate defaults from tokens.css
const DARK_TOKENS = {
  bg: '#0a0a0c', surface: '#111114', surfaceRaised: '#16161a',
  border: '#2a2a33', text: '#e8e6e3', textDim: '#6b6a72', textMuted: '#52525b',
};
const LIGHT_TOKENS = {
  bg: '#f8fafc', surface: '#ffffff', surfaceRaised: '#f1f5f9',
  border: '#cbd5e1', text: '#0f172a', textDim: '#475569', textMuted: '#94a3b8',
};

export function LoginPreview(props: LoginPreviewProps) {
  const hasGoogle = () => props.providers().includes('google');
  const hasGithub = () => props.providers().includes('github');
  const hasOAuth = () => hasGoogle() || hasGithub();
  const hasEmail = () => props.providers().includes('email');

  const tokens = () => props.darkMode() ? DARK_TOKENS : LIGHT_TOKENS;

  const rootVars = () => {
    const t = tokens();
    return {
      '--lp-accent': props.primaryColor(),
      '--lp-bg': props.bgColor ? (props.bgColor() || t.bg) : t.bg,
      '--lp-card-bg': props.cardColor ? (props.cardColor() || t.surface) : t.surface,
      '--lp-card-border': t.border,
      '--lp-text': t.text,
      '--lp-text-dim': t.textDim,
      '--lp-text-muted': t.textMuted,
      '--lp-input-bg': t.surface,
      '--lp-input-border': t.border,
      '--lp-oauth-bg': t.surfaceRaised,
      '--lp-btn-text': props.darkMode() ? t.bg : '#ffffff',
    };
  };

  const slugText = props.slug ? props.slug : () => 'your-app';

  const customStyle = () => {
    if (!props.customCSS) return '';
    const raw = props.customCSS();
    if (!raw) return '';
    return sanitizeCSS(raw);
  };

  return h('div', { class: 'lp-browser', style: rootVars },
    // Browser chrome
    h('div', { class: 'lp-chrome' },
      h('div', { class: 'lp-chrome-dots' },
        h('span', { class: 'lp-dot lp-dot-red' }),
        h('span', { class: 'lp-dot lp-dot-yellow' }),
        h('span', { class: 'lp-dot lp-dot-green' }),
      ),
      h('div', { class: 'lp-chrome-bar' },
        h('span', { class: 'lp-chrome-lock' }, '\uD83D\uDD12'),
        h('span', null, () => 'yoursite.com/t/' + slugText() + '/login'),
      ),
    ),

    // Page body
    h('div', { class: 'lp-page' },
      h('div', { class: 'lp-card', style: () => customStyle() },
        h('div', { class: 'lp-logo-area' },
          createShow(
            () => props.logoUrl() !== '',
            () => h('img', {
              src: () => props.logoUrl(),
              alt: 'Logo',
              class: 'lp-logo',
              onError: (e: Event) => (e.target as HTMLElement).classList.add('lp-logo-broken'),
              onLoad: (e: Event) => (e.target as HTMLElement).classList.remove('lp-logo-broken'),
            }),
          ),
          h('div', { class: 'lp-app-name' }, () => props.appName() || 'Your App'),
        ),

        h('div', { class: 'lp-subtitle' }, 'Sign in to your account'),

        createShow(hasOAuth, () =>
          h('div', { class: 'lp-oauth-buttons' },
            createShow(hasGoogle, () =>
              h('div', { class: 'lp-btn lp-btn-oauth' }, GoogleIcon(), 'Continue with Google'),
            ),
            createShow(hasGithub, () =>
              h('div', { class: 'lp-btn lp-btn-oauth' }, GitHubIcon(), 'Continue with GitHub'),
            ),
          ),
        ),

        createShow(() => hasOAuth() && hasEmail(), () =>
          h('div', { class: 'lp-divider' },
            h('span', { class: 'lp-divider-line' }),
            h('span', null, 'OR'),
            h('span', { class: 'lp-divider-line' }),
          ),
        ),

        createShow(hasEmail, () =>
          h('div', { class: 'lp-form' },
            h('div', { class: 'lp-field' },
              h('label', null, 'EMAIL'),
              h('div', { class: 'lp-input' }, 'you@example.com'),
            ),
            h('div', { class: 'lp-field' },
              h('label', null, 'PASSWORD'),
              h('div', { class: 'lp-input lp-input-password' },
                h('span', null, '\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022'),
                h('span', { class: 'lp-eye' }, EyeIcon()),
              ),
            ),
            h('div', { class: 'lp-forgot' },
              h('span', null, 'Forgot password?'),
            ),
            h('label', { class: 'lp-remember' },
              h('input', { type: 'checkbox', disabled: true }),
              'Remember email',
            ),
            h('div', {
              class: 'lp-btn lp-btn-primary',
              style: () => ({
                background: props.primaryColor(),
                borderColor: props.primaryColor(),
              }),
            }, () => props.buttonText() || 'Sign in'),
          ),
        ),

        h('div', { class: 'lp-toggle' }, "Don\u2019t have an account? ",
          h('span', { class: 'lp-toggle-link' }, 'Sign up'),
        ),

        h('div', { class: 'lp-secured' },
          'Secured by ',
          h('span', { class: 'lp-secured-link' }, 'fAuth'),
        ),
      ),
    ),
  );
}
