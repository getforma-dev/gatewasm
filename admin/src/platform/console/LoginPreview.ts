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
import { sanitizeCSS } from "./sanitizeCSS";

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

  const rootVars = () => {
    const t = tokens();
    const userText = props.textColor ? props.textColor() : "";
    const font = props.fontFamily ? props.fontFamily() : "";
    const effectiveFont =
      font && font !== "System default"
        ? `"${font}", sans-serif`
        : "inherit";
    return {
      "--lp-accent": props.primaryColor(),
      "--lp-bg": props.bgColor ? props.bgColor() || t.bg : t.bg,
      "--lp-card-bg": props.cardColor
        ? props.cardColor() || t.surface
        : t.surface,
      "--lp-card-border": t.border,
      "--lp-text": userText || t.text,
      "--lp-text-dim": userText || t.textDim,
      "--lp-text-muted": userText || t.textMuted,
      "--lp-input-bg": t.surface,
      "--lp-input-border": t.border,
      "--lp-oauth-bg": t.surfaceRaised,
      "--lp-btn-text": props.darkMode() ? t.bg : "#ffffff",
      "--lp-font": effectiveFont,
    };
  };

  // Dynamically load Google Fonts when the operator picks a non-system font.
  // Each font gets its own <link> (idempotent — the browser deduplicates
  // identical hrefs). We don't remove old links because the operator may
  // switch back and forth during the session.
  createEffect(() => {
    const font = props.fontFamily ? props.fontFamily() : "";
    if (!font || font === "System default") return;
    const id = "lp-gfont-" + font.replace(/\s+/g, "-").toLowerCase();
    if (document.getElementById(id)) return;
    const link = document.createElement("link");
    link.id = id;
    link.rel = "stylesheet";
    link.href =
      "https://fonts.googleapis.com/css2?family=" +
      encodeURIComponent(font) +
      ":wght@400;500;600;700&display=swap";
    document.head.appendChild(link);
  });

  const slugText = props.slug ? props.slug : () => "your-app";

  const customStyle = () => {
    if (!props.customCSS) return "";
    const raw = props.customCSS();
    if (!raw) return "";
    return sanitizeCSS(raw);
  };

  return h(
    "div",
    { class: "lp-browser", style: rootVars },
    // Browser chrome
    h(
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
    ),

    // Page body
    h(
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
            let s =
              "background:" +
              (props.cardColor
                ? props.cardColor() || tokens().surface
                : tokens().surface) +
              ";";
            const font = props.fontFamily ? props.fontFamily() : "";
            if (font && font !== "System default") {
              s += 'font-family:"' + font + '",sans-serif;';
            }
            const size = props.fontSize ? props.fontSize() : "medium";
            const px = size === "small" ? "13px" : size === "large" ? "17px" : "15px";
            s += "font-size:" + px + ";";
            const custom = customStyle();
            return s + custom;
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
    ),
  );
}
