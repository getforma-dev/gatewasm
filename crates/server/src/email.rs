use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use resend_rs::types::CreateEmailBaseOptions;
use resend_rs::Resend;

use crate::config::AppConfig;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn normalize_base_url(server_url: &str) -> String {
    server_url.trim_end_matches('/').to_string()
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Returns `true` when at least one email transport (Resend *or* SMTP) is configured.
pub fn email_delivery_configured(config: &AppConfig) -> bool {
    config.resend_api_key.is_some()
        || (config.smtp_host.is_some() && config.smtp_from_email.is_some())
}

// ---------------------------------------------------------------------------
// Unified entry point
// ---------------------------------------------------------------------------

/// Send a transactional email.
///
/// Prefers **Resend** when `RESEND_API_KEY` is set, otherwise falls back to
/// SMTP.  If neither provider is configured the call is a no-op (logged as a
/// warning).
pub async fn send_email(
    config: &AppConfig,
    to_email: &str,
    subject: &str,
    text_body: &str,
    html_body: &str,
) -> Result<(), String> {
    // Prefer Resend if API key is configured
    if let Some(ref api_key) = config.resend_api_key {
        return send_via_resend(
            api_key,
            &config.email_from_name,
            &config.email_from_address,
            to_email,
            subject,
            html_body,
            text_body,
        )
        .await;
    }

    // Fall back to SMTP
    if config.smtp_host.is_some() && config.smtp_from_email.is_some() {
        return send_via_smtp(config, to_email, subject, text_body, html_body).await;
    }

    tracing::warn!(
        to = to_email,
        subject = subject,
        "No email provider configured — email not sent"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Resend transport
// ---------------------------------------------------------------------------

async fn send_via_resend(
    api_key: &str,
    from_name: &str,
    from_address: &str,
    to_email: &str,
    subject: &str,
    html_body: &str,
    text_body: &str,
) -> Result<(), String> {
    let resend = Resend::new(api_key);
    let from = format!("{from_name} <{from_address}>");

    let email = CreateEmailBaseOptions::new(from, [to_email], subject)
        .with_html(html_body)
        .with_text(text_body);

    resend
        .emails
        .send(email)
        .await
        .map_err(|e| format!("Resend send failed: {e}"))?;

    tracing::info!(to = to_email, subject = subject, "Email sent via Resend");
    Ok(())
}

// ---------------------------------------------------------------------------
// SMTP transport (existing behaviour, extracted)
// ---------------------------------------------------------------------------

fn configured_from_mailbox(config: &AppConfig) -> Result<Mailbox, String> {
    let from_email = config
        .smtp_from_email
        .as_deref()
        .ok_or_else(|| "SMTP_FROM_EMAIL is not configured".to_string())?;
    let from = from_email
        .parse()
        .map_err(|e| format!("Invalid SMTP_FROM_EMAIL: {e}"))?;
    Ok(Mailbox::new(Some(config.smtp_from_name.clone()), from))
}

fn smtp_transport(config: &AppConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    let host = config
        .smtp_host
        .as_deref()
        .ok_or_else(|| "SMTP_HOST is not configured".to_string())?;

    let mut builder = if config.smtp_starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
            .map_err(|e| format!("Invalid SMTP relay host: {e}"))?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
    };

    builder = builder.port(config.smtp_port);

    if let (Some(username), Some(password)) =
        (config.smtp_username.as_ref(), config.smtp_password.as_ref())
    {
        builder = builder.credentials(Credentials::new(username.clone(), password.clone()));
    }

    Ok(builder.build())
}

async fn send_via_smtp(
    config: &AppConfig,
    to_email: &str,
    subject: &str,
    text_body: &str,
    html_body: &str,
) -> Result<(), String> {
    let to = to_email
        .parse()
        .map_err(|e| format!("Invalid recipient email '{to_email}': {e}"))?;

    let message = Message::builder()
        .from(configured_from_mailbox(config)?)
        .to(Mailbox::new(None, to))
        .subject(subject)
        .multipart(MultiPart::alternative_plain_html(
            text_body.to_string(),
            html_body.to_string(),
        ))
        .map_err(|e| format!("Failed to build email message: {e}"))?;

    let transport = smtp_transport(config)?;
    transport
        .send(message)
        .await
        .map_err(|e| format!("SMTP send failed: {e}"))?;

    tracing::info!(to = to_email, subject = subject, "Email sent via SMTP");
    Ok(())
}

// ---------------------------------------------------------------------------
// HTML wrapper
// ---------------------------------------------------------------------------

/// Wraps inner HTML content in a responsive, Outlook-compatible email shell.
fn email_html_wrapper(inner: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta http-equiv="X-UA-Compatible" content="IE=edge">
<title>Forma Auth</title>
</head>
<body style="margin:0;padding:0;background-color:#f4f4f7;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,'Helvetica Neue',Arial,sans-serif;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background-color:#f4f4f7;">
  <tr>
    <td align="center" style="padding:40px 16px;">
      <table role="presentation" width="600" cellpadding="0" cellspacing="0" style="max-width:600px;width:100%;background-color:#ffffff;border-radius:8px;overflow:hidden;box-shadow:0 1px 4px rgba(0,0,0,0.08);">
        <!-- Header -->
        <tr>
          <td style="padding:32px 40px 0 40px;">
            <p style="margin:0 0 4px;font-size:13px;font-weight:600;color:#6b7280;text-transform:uppercase;letter-spacing:0.5px;">Forma Auth</p>
          </td>
        </tr>
        <!-- Body -->
        <tr>
          <td style="padding:24px 40px 40px 40px;font-size:16px;line-height:1.6;color:#1f2937;">
            {inner}
          </td>
        </tr>
        <!-- Footer -->
        <tr>
          <td style="padding:20px 40px;border-top:1px solid #e5e7eb;font-size:12px;color:#9ca3af;text-align:center;">
            &copy; {year} Forma &middot; Secure authentication infrastructure
          </td>
        </tr>
      </table>
    </td>
  </tr>
</table>
</body>
</html>"#,
        inner = inner,
        year = chrono::Utc::now().format("%Y"),
    )
}

/// Renders a CTA button for email templates (table-based for Outlook).
fn email_button(href: &str, label: &str) -> String {
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:24px 0;">
  <tr>
    <td align="center" style="background-color:#4f46e5;border-radius:6px;">
      <a href="{href}" target="_blank" style="display:inline-block;padding:14px 32px;font-size:16px;font-weight:600;color:#ffffff;text-decoration:none;border-radius:6px;">{label}</a>
    </td>
  </tr>
</table>"#,
        href = href,
        label = label,
    )
}

// ---------------------------------------------------------------------------
// Email content builders
// ---------------------------------------------------------------------------

pub fn verification_email_content(
    server_url: &str,
    tenant_slug: &str,
    app_name: &str,
    token: &str,
) -> (String, String, String) {
    let base = normalize_base_url(server_url);
    let link = format!(
        "{}/t/{}/auth/verify-email?token={}",
        base,
        tenant_slug,
        urlencoding::encode(token)
    );
    let escaped_app_name = escape_html(app_name);
    let escaped_link = escape_html(&link);

    let subject = format!("Verify your {} account", app_name);

    let text = format!(
        "Welcome to {app_name}.\n\nVerify your email by opening this link:\n{link}\n\nThis link expires in 24 hours."
    );

    let inner = format!(
        r#"<h1 style="margin:0 0 16px;font-size:22px;font-weight:700;color:#111827;">Verify your email</h1>
<p style="margin:0 0 8px;">Welcome to <strong>{escaped_app_name}</strong>. Please verify your email address to get started.</p>
{button}
<p style="margin:0;font-size:13px;color:#6b7280;">If the button doesn't work, copy and paste this link into your browser:</p>
<p style="margin:4px 0 0;font-size:13px;word-break:break-all;color:#4f46e5;"><a href="{escaped_link}" style="color:#4f46e5;">{escaped_link}</a></p>
<p style="margin:16px 0 0;font-size:13px;color:#6b7280;">This link expires in 24 hours.</p>"#,
        escaped_app_name = escaped_app_name,
        button = email_button(&escaped_link, "Verify email address"),
        escaped_link = escaped_link,
    );
    let html = email_html_wrapper(&inner);

    (subject, text, html)
}

pub fn password_reset_email_content(
    server_url: &str,
    tenant_slug: &str,
    app_name: &str,
    token: &str,
) -> (String, String, String) {
    let base = normalize_base_url(server_url);
    let link = format!(
        "{}/t/{}/login?reset_token={}",
        base,
        tenant_slug,
        urlencoding::encode(token)
    );
    let escaped_app_name = escape_html(app_name);
    let escaped_link = escape_html(&link);

    let subject = format!("Reset your {} password", app_name);

    let text = format!(
        "We received a password reset request for {app_name}.\n\nOpen this link to set a new password:\n{link}\n\nThis link expires in 1 hour."
    );

    let inner = format!(
        r#"<h1 style="margin:0 0 16px;font-size:22px;font-weight:700;color:#111827;">Reset your password</h1>
<p style="margin:0 0 8px;">We received a request to reset your <strong>{escaped_app_name}</strong> password. Click the button below to choose a new one.</p>
{button}
<p style="margin:0;font-size:13px;color:#6b7280;">If the button doesn't work, copy and paste this link into your browser:</p>
<p style="margin:4px 0 0;font-size:13px;word-break:break-all;color:#4f46e5;"><a href="{escaped_link}" style="color:#4f46e5;">{escaped_link}</a></p>
<p style="margin:16px 0 0;font-size:13px;color:#6b7280;">This link expires in 1 hour. If you didn't request this, you can safely ignore this email.</p>"#,
        escaped_app_name = escaped_app_name,
        button = email_button(&escaped_link, "Reset password"),
        escaped_link = escaped_link,
    );
    let html = email_html_wrapper(&inner);

    (subject, text, html)
}

pub fn tenant_invite_email_content(
    app_name: &str,
    invite_url: &str,
    role_name: &str,
    invited_by: Option<&str>,
) -> (String, String, String) {
    let escaped_app_name = escape_html(app_name);
    let escaped_role_name = escape_html(role_name);
    let escaped_invite_url = escape_html(invite_url);
    let inviter_line = invited_by
        .map(|value| format!("Invited by: {value}.\n\n"))
        .unwrap_or_default();
    let inviter_html = invited_by
        .map(|value| {
            format!(
                r#"<p style="margin:0 0 8px;">Invited by <strong>{}</strong>.</p>"#,
                escape_html(value)
            )
        })
        .unwrap_or_default();

    let subject = format!("You were invited to {}", app_name);

    let text = format!(
        "You have been invited to join {app_name} with role '{role_name}'.\n\n{inviter_line}Accept invite:\n{invite_url}\n\nThis invite expires soon."
    );

    let inner = format!(
        r#"<h1 style="margin:0 0 16px;font-size:22px;font-weight:700;color:#111827;">You're invited</h1>
<p style="margin:0 0 8px;">You have been invited to join <strong>{escaped_app_name}</strong> with the role <strong>{escaped_role_name}</strong>.</p>
{inviter_html}{button}
<p style="margin:0;font-size:13px;color:#6b7280;">If the button doesn't work, copy and paste this link into your browser:</p>
<p style="margin:4px 0 0;font-size:13px;word-break:break-all;color:#4f46e5;"><a href="{escaped_invite_url}" style="color:#4f46e5;">{escaped_invite_url}</a></p>
<p style="margin:16px 0 0;font-size:13px;color:#6b7280;">This invite expires soon.</p>"#,
        escaped_app_name = escaped_app_name,
        escaped_role_name = escaped_role_name,
        inviter_html = inviter_html,
        button = email_button(&escaped_invite_url, "Accept invite"),
        escaped_invite_url = escaped_invite_url,
    );
    let html = email_html_wrapper(&inner);

    (subject, text, html)
}

/// Build the subject / text / HTML for a platform team invite email.
pub fn platform_team_invite_email_content(
    invite_url: &str,
    role: &str,
    invited_by_email: &str,
    tenant_names: &[String],
) -> (String, String, String) {
    let role_display = match role {
        "admin" => "Admin",
        "editor" => "Editor",
        "viewer" => "Viewer",
        other => other,
    };

    let tenant_line = if tenant_names.is_empty() {
        "You'll have access to all workspaces.".to_string()
    } else {
        format!("You'll have access to: {}.", tenant_names.join(", "))
    };

    let escaped_invite_url = escape_html(invite_url);
    let escaped_role_display = escape_html(role_display);
    let escaped_invited_by = escape_html(invited_by_email);
    let escaped_tenant_line = escape_html(&tenant_line);

    let subject = "You've been invited to join the fAuth platform team".to_string();

    let text = format!(
        "You have been invited to join the fAuth platform team as {role_display}.\n\nInvited by: {invited_by_email}\n\n{tenant_line}\n\nAccept invite:\n{invite_url}\n\nThis invite expires soon."
    );

    let inner = format!(
        r#"<h1 style="margin:0 0 16px;font-size:22px;font-weight:700;color:#111827;">You're invited to the platform team</h1>
<p style="margin:0 0 8px;">You have been invited to join the <strong>fAuth platform team</strong> with the role <strong>{escaped_role_display}</strong>.</p>
<p style="margin:0 0 8px;">Invited by <strong>{escaped_invited_by}</strong>.</p>
<p style="margin:0 0 16px;">{escaped_tenant_line}</p>
{button}
<p style="margin:0;font-size:13px;color:#6b7280;">If the button doesn't work, copy and paste this link into your browser:</p>
<p style="margin:4px 0 0;font-size:13px;word-break:break-all;color:#4f46e5;"><a href="{escaped_invite_url}" style="color:#4f46e5;">{escaped_invite_url}</a></p>
<p style="margin:16px 0 0;font-size:13px;color:#6b7280;">This invite expires soon.</p>"#,
        escaped_role_display = escaped_role_display,
        escaped_invited_by = escaped_invited_by,
        escaped_tenant_line = escaped_tenant_line,
        button = email_button(&escaped_invite_url, "Accept invite"),
        escaped_invite_url = escaped_invite_url,
    );
    let html = email_html_wrapper(&inner);

    (subject, text, html)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_team_invite_subject_and_role_mapping() {
        let (subject, text, html) = platform_team_invite_email_content(
            "https://example.com/invite?tok=abc&x=1",
            "admin",
            "alice@example.com",
            &["Acme".to_string(), "Beta Corp".to_string()],
        );

        assert_eq!(subject, "You've been invited to join the fAuth platform team");
        assert!(text.contains("Admin"));
        assert!(text.contains("alice@example.com"));
        assert!(text.contains("Acme, Beta Corp"));
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Admin"));
        assert!(html.contains("alice@example.com"));
        // URL ampersand must be escaped in HTML
        assert!(html.contains("tok=abc&amp;x=1"));
        assert!(!html.contains("tok=abc&x=1"));
    }

    #[test]
    fn platform_team_invite_empty_tenants_shows_all_workspaces() {
        let (_, text, html) = platform_team_invite_email_content(
            "https://example.com/invite",
            "viewer",
            "bob@example.com",
            &[],
        );

        assert!(text.contains("You'll have access to all workspaces."));
        assert!(html.contains("You&#39;ll have access to all workspaces."));
    }

    #[test]
    fn platform_team_invite_escapes_untrusted_inputs() {
        let (_, _, html) = platform_team_invite_email_content(
            "https://example.com/invite",
            "editor",
            "evil<script>@example.com",
            &["Tenant <X>".to_string()],
        );

        assert!(html.contains("evil&lt;script&gt;@example.com"));
        assert!(html.contains("Tenant &lt;X&gt;"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn invite_html_escapes_untrusted_inputs() {
        let (_, _, html) = tenant_invite_email_content(
            "App <Name>",
            "https://example.com/invite?x=1&y=2",
            "admin<script>",
            Some("Alice & Bob"),
        );

        assert!(html.contains("App &lt;Name&gt;"));
        assert!(html.contains("admin&lt;script&gt;"));
        assert!(html.contains("Alice &amp; Bob"));
        assert!(html.contains("x=1&amp;y=2"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn email_delivery_configured_resend_only() {
        let mut config = test_config();
        config.resend_api_key = Some("re_test_key".to_string());
        config.smtp_host = None;
        config.smtp_from_email = None;
        assert!(email_delivery_configured(&config));
    }

    #[test]
    fn email_delivery_configured_smtp_only() {
        let mut config = test_config();
        config.resend_api_key = None;
        config.smtp_host = Some("smtp.example.com".to_string());
        config.smtp_from_email = Some("noreply@example.com".to_string());
        assert!(email_delivery_configured(&config));
    }

    #[test]
    fn email_delivery_configured_neither() {
        let mut config = test_config();
        config.resend_api_key = None;
        config.smtp_host = None;
        config.smtp_from_email = None;
        assert!(!email_delivery_configured(&config));
    }

    #[test]
    fn verification_email_contains_wrapper() {
        let (_, _, html) =
            verification_email_content("https://app.example.com", "demo", "Demo App", "tok_123");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Verify your email"));
        assert!(html.contains("Demo App"));
        assert!(html.contains("tok_123"));
    }

    #[test]
    fn password_reset_email_contains_wrapper() {
        let (_, _, html) =
            password_reset_email_content("https://app.example.com", "demo", "Demo App", "rst_abc");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Reset your password"));
        assert!(html.contains("rst_abc"));
    }

    /// Minimal `AppConfig` for unit tests — only email-relevant fields matter.
    fn test_config() -> AppConfig {
        use crate::compliance::ComplianceConfig;
        AppConfig {
            port: 3001,
            host: "localhost".to_string(),
            server_url: "http://localhost:3001".to_string(),
            database_url: String::new(),
            ed25519_private_key_pem: None,
            jwt_issuer: String::new(),
            jwt_audience: String::new(),
            access_token_expiry: 900,
            refresh_token_expiry: 2592000,
            google_client_id: None,
            google_client_secret: None,
            github_client_id: None,
            github_client_secret: None,
            allowed_origins: vec![],
            csrf_secret: String::new(),
            mfa_encryption_key: String::new(),
            mfa_encryption_key_previous: vec![],
            platform_admin_key: None,
            platform_owner_emails: vec![],
            platform_domain: None,
            resend_api_key: None,
            email_from_address: "noreply@getforma.dev".to_string(),
            email_from_name: "Forma Auth".to_string(),
            smtp_host: None,
            smtp_port: 587,
            smtp_username: None,
            smtp_password: None,
            smtp_from_email: None,
            smtp_from_name: "Forma Auth".to_string(),
            smtp_starttls: true,
            compliance: ComplianceConfig::default(),
            ssf_signing_key_pem: None,
            maxmind_db_path: None,
        }
    }
}
