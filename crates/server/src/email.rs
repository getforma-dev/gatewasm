use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::AppConfig;

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

pub async fn send_email(
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

    Ok(())
}

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
    let html = format!(
        "<p>Welcome to <strong>{escaped_app_name}</strong>.</p><p>Verify your email by clicking the link below:</p><p><a href=\"{escaped_link}\">Verify email</a></p><p>This link expires in 24 hours.</p>"
    );
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
    let html = format!(
        "<p>We received a password reset request for <strong>{escaped_app_name}</strong>.</p><p>Open the link below to set a new password:</p><p><a href=\"{escaped_link}\">Reset password</a></p><p>This link expires in 1 hour.</p>"
    );
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
                "<p>Invited by: <strong>{}</strong>.</p>",
                escape_html(value)
            )
        })
        .unwrap_or_default();

    let subject = format!("You were invited to {}", app_name);
    let text = format!(
        "You have been invited to join {app_name} with role '{role_name}'.\n\n{inviter_line}Accept invite:\n{invite_url}\n\nThis invite expires soon."
    );
    let html = format!(
        "<p>You have been invited to join <strong>{escaped_app_name}</strong> with role <strong>{escaped_role_name}</strong>.</p>{inviter_html}<p><a href=\"{escaped_invite_url}\">Accept invite</a></p><p>This invite expires soon.</p>"
    );
    (subject, text, html)
}

pub fn email_delivery_configured(config: &AppConfig) -> bool {
    config.smtp_host.is_some() && config.smtp_from_email.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
