/**
 * LoginPreview — native reactive mock of the tenant login page
 * wrapped in a mini browser chrome for a polished preview feel.
 */
import { createEffect, createShow, h } from "formajs";
import {
  EyeIcon,
  GitHubIcon,
  GoogleIcon,
} from "../../core/components/ui/BrandIcons";
// sanitizeCSS is available for when customCSS (a Pro feature) is enabled.
// Currently always receives "" from the wizard, so unused.
// import { sanitizeCSS } from "./sanitizeCSS";

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
  fontFamily?: () => string;
  fontSize?: () => string;
  textColor?: () => string;
  /**
   * Controls whether the "Don't have an account? Sign up" link renders.
   * "open" → link visible (anyone can sign up).
   * "invite_only" → link hidden (only invited users can sign in).
   * Optional; defaults to "open" so existing call sites are unchanged.
   */
  signupMode?: () => string;
}

// Token-accurate defaults from tokens.css
const DARK_TOKENS = {
  bg: "#0a0a0c",
  surface: "#111114",
  surfaceRaised: "#16161a",
  border: "#2a2a33",
  text: "#e8e6e3",
  textDim: "#6b6a72",
  textMuted: "#52525b",
};
const LIGHT_TOKENS = {
  bg: "#f8fafc",
  surface: "#ffffff",
  surfaceRaised: "#f1f5f9",
  border: "#cbd5e1",
  text: "#0f172a",
  textDim: "#475569",
  textMuted: "#94a3b8",
};

export function LoginPreview(props: LoginPreviewProps) {
  const hasGoogle = () => props.providers().includes("google");
  const hasGithub = () => props.providers().includes("github");
  const hasOAuth = () => hasGoogle() || hasGithub();
  const hasEmail = () => props.providers().includes("email");

  const tokens = () => (props.darkMode() ? DARK_TOKENS : LIGHT_TOKENS);

  // Build the root element imperatively so we can apply CSS custom
  // properties via el.style.setProperty() — Object.assign on
  // CSSStyleDeclaration doesn't reliably set custom properties (keys
  // starting with --) across all browsers.
  const rootEl = h("div", { class: "lp-browser" }) as HTMLElement;
  createEffect(() => {
    const t = tokens();
    const userText = props.textColor ? props.textColor() : "";
    const s = rootEl.style;
    s.setProperty("--lp-accent", props.primaryColor());
    s.setProperty("--lp-bg", props.bgColor ? props.bgColor() || t.bg : t.bg);
    s.setProperty(
      "--lp-card-bg",
      props.cardColor ? props.cardColor() || t.surface : t.surface,
    );
    s.setProperty("--lp-card-border", t.border);
    s.setProperty("--lp-text", userText || t.text);
    s.setProperty("--lp-text-dim", userText || t.textDim);
    s.setProperty("--lp-text-muted", userText || t.textMuted);
    s.setProperty("--lp-input-bg", t.surface);
    s.setProperty("--lp-input-border", t.border);
    s.setProperty("--lp-oauth-bg", t.surfaceRaised);
    s.setProperty("--lp-btn-text", props.darkMode() ? t.bg : "#ffffff");
  });

  const slugText = props.slug ? props.slug : () => "your-app";

  // Build the chrome and page as children, then append to rootEl.
  const chrome = h(
    "div",
    { class: "lp-chrome" },
    h(
      "div",
      { class: "lp-chrome-dots" },
      h("span", { class: "lp-dot lp-dot-red" }),
      h("span", { class: "lp-dot lp-dot-yellow" }),
      h("span", { class: "lp-dot lp-dot-green" }),
    ),
    h(
      "div",
      { class: "lp-chrome-bar" },
      h("span", { class: "lp-chrome-lock" }, "\uD83D\uDD12"),
      h("span", null, () => "yoursite.com/t/" + slugText() + "/login"),
    ),
  );

  const page = h(
    "div",
    {
      class: "lp-page",
      style: () => ({
        background: props.bgColor
          ? props.bgColor() || tokens().bg
          : tokens().bg,
      }),
    },
    h(
      "div",
      {
        class: "lp-card",
        style: () => {
          const bg = props.cardColor
            ? props.cardColor() || tokens().surface
            : tokens().surface;
          const font = props.fontFamily ? props.fontFamily() : "";
          const size = props.fontSize ? props.fontSize() : "medium";
          const px = size === "small" ? "13px" : size === "large" ? "17px" : "15px";
          const result: Record<string, string> = {
            background: bg,
            fontSize: px,
          };
          if (font && font !== "System default") {
            result.fontFamily = '"' + font + '", sans-serif';
          }
          return result;
        },
      },
      h(
        "div",
        { class: "lp-logo-area" },
        createShow(
          () => props.logoUrl() !== "",
          () =>
            h("img", {
              src: () => props.logoUrl(),
              alt: "Logo",
              class: "lp-logo",
              onError: (e: Event) =>
                (e.target as HTMLElement).classList.add("lp-logo-broken"),
              onLoad: (e: Event) =>
                (e.target as HTMLElement).classList.remove("lp-logo-broken"),
            }),
        ),
        createShow(
          () => props.appName() !== "",
          () => h("div", { class: "lp-app-name" }, () => props.appName()),
        ),
      ),

      h("div", { class: "lp-subtitle" }, "Sign in to your account"),

      createShow(hasOAuth, () =>
        h(
          "div",
          { class: "lp-oauth-buttons" },
          createShow(hasGoogle, () =>
            h(
              "div",
              { class: "lp-btn lp-btn-oauth" },
              GoogleIcon(),
              "Continue with Google",
            ),
          ),
          createShow(hasGithub, () =>
            h(
              "div",
              { class: "lp-btn lp-btn-oauth" },
              GitHubIcon(),
              "Continue with GitHub",
            ),
          ),
        ),
      ),

      createShow(
        () => hasOAuth() && hasEmail(),
        () =>
          h(
            "div",
            { class: "lp-divider" },
            h("span", { class: "lp-divider-line" }),
            h("span", null, "OR"),
            h("span", { class: "lp-divider-line" }),
          ),
      ),

      createShow(hasEmail, () =>
        h(
          "div",
          { class: "lp-form" },
          h(
            "div",
            { class: "lp-field" },
            h("label", null, "EMAIL"),
            h("div", { class: "lp-input" }, "you@example.com"),
          ),
          h(
            "div",
            { class: "lp-field" },
            h("label", null, "PASSWORD"),
            h(
              "div",
              { class: "lp-input lp-input-password" },
              h(
                "span",
                null,
                "\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022",
              ),
              h("span", { class: "lp-eye" }, EyeIcon()),
            ),
          ),
          h(
            "div",
            { class: "lp-forgot" },
            h("span", null, "Forgot password?"),
          ),
          h(
            "label",
            { class: "lp-remember" },
            h("input", { type: "checkbox", disabled: true }),
            "Remember email",
          ),
          h(
            "div",
            {
              class: "lp-btn lp-btn-primary",
              style: () => ({
                background: props.primaryColor(),
                borderColor: props.primaryColor(),
              }),
            },
            () => props.buttonText() || "Sign in",
          ),
        ),
      ),

      createShow(
        () => (props.signupMode ? props.signupMode() : "open") !== "invite_only",
        () =>
          h(
            "div",
            { class: "lp-toggle" },
            "Don\u2019t have an account? ",
            h("span", { class: "lp-toggle-link" }, "Sign up"),
          ),
      ),

      h(
        "div",
        { class: "lp-secured" },
        "Secured by ",
        h("span", { class: "lp-secured-link" }, "fAuth"),
      ),
    ),
  );

  rootEl.appendChild(chrome);
  rootEl.appendChild(page);
  return rootEl;
}
