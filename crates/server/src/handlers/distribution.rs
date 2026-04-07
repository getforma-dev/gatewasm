use axum::response::{Html, IntoResponse};

/// GET /get-started — Bridge.js distribution page showing all integration options.
pub async fn get_started_page() -> impl IntoResponse {
    let html = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>fAuth — Get Started</title>
<style>
:root {
  --primary: #6366f1;
  --primary-hover: #4f46e5;
  --bg: #f8fafc;
  --card-bg: #ffffff;
  --text: #1e293b;
  --text-muted: #64748b;
  --border: #e2e8f0;
  --radius: 10px;
  --code-bg: #1a1d23;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  background: var(--bg);
  color: var(--text);
  min-height: 100vh;
  padding: 2rem 1rem;
}
.container { max-width: 720px; margin: 0 auto; }
.header { text-align: center; margin-bottom: 2.5rem; }
.header h1 { font-size: 1.75rem; font-weight: 700; letter-spacing: -0.02em; margin-bottom: 0.5rem; }
.header p { color: var(--text-muted); font-size: 1rem; }
.card {
  background: var(--card-bg);
  border-radius: 12px;
  box-shadow: 0 1px 3px rgba(0,0,0,0.06);
  border: 1px solid var(--border);
  margin-bottom: 1.5rem;
}
.card-header {
  padding: 1.25rem 1.5rem;
  border-bottom: 1px solid var(--border);
  font-weight: 600;
  font-size: 0.9375rem;
}
.card-body { padding: 1.5rem; }
.option {
  display: flex;
  align-items: flex-start;
  padding: 1rem 0;
  border-bottom: 1px solid var(--border);
  gap: 1rem;
}
.option:last-child { border-bottom: none; }
.option-label {
  width: 80px;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  padding-top: 0.25rem;
  flex-shrink: 0;
}
.option-value {
  flex: 1;
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.8125rem;
  word-break: break-all;
  line-height: 1.5;
}
.option-note {
  font-family: -apple-system, sans-serif;
  color: var(--text-muted);
  font-size: 0.8125rem;
}
.code-block {
  background: var(--code-bg);
  border-radius: var(--radius);
  padding: 1.25rem;
  overflow-x: auto;
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.8125rem;
  line-height: 1.7;
  color: #e5e7eb;
  position: relative;
}
.code-block .kw { color: #c084fc; }
.code-block .str { color: #34d399; }
.code-block .fn { color: #60a5fa; }
.code-block .cm { color: #6b7280; }
.code-copy {
  position: absolute;
  top: 0.75rem;
  right: 0.75rem;
  background: rgba(255,255,255,0.08);
  border: 1px solid rgba(255,255,255,0.1);
  color: #9ca3af;
  padding: 0.25rem 0.5rem;
  border-radius: 4px;
  font-size: 0.6875rem;
  cursor: pointer;
}
.code-copy:hover { background: rgba(255,255,255,0.15); color: #fff; }
.links { display: flex; gap: 0.75rem; margin-top: 1.5rem; flex-wrap: wrap; }
.links a {
  padding: 0.625rem 1.25rem;
  border-radius: 8px;
  font-size: 0.875rem;
  font-weight: 500;
  text-decoration: none;
  transition: all 0.12s;
}
.link-primary { background: var(--primary); color: #fff; }
.link-primary:hover { background: var(--primary-hover); }
.link-outline { background: var(--card-bg); border: 1px solid var(--border); color: var(--text); }
.link-outline:hover { background: #f1f5f9; }
.link-primary-color { color: var(--primary); text-decoration: none; }
.footer { text-align: center; margin-top: 2rem; font-size: 0.75rem; color: var(--text-muted); }
.footer a { color: var(--text-muted); text-decoration: none; }
.footer a:hover { text-decoration: underline; }
</style>
</head>
<body>
<div class="container">
  <div class="header">
    <h1>Get Started with fAuth</h1>
    <p>Add authentication to your app in under 5 minutes</p>
  </div>

  <div class="card">
    <div class="card-header">1. Add bridge.js</div>
    <div class="card-body">
      <div class="option">
        <span class="option-label">CDN</span>
        <span class="option-value">&lt;script src="https://cdn.getforma.dev/bridge/v1/gate-bridge.min.js"&gt;&lt;/script&gt;</span>
      </div>
      <div class="option">
        <span class="option-label">Direct</span>
        <span class="option-value">&lt;script src="https://auth.getforma.dev/bridge.js"&gt;&lt;/script&gt;</span>
      </div>
      <div class="option">
        <span class="option-label">Download</span>
        <span class="option-value">
          <a href="/bridge.js" download="gate-bridge.js" class="link-primary-color">gate-bridge.js</a>
          <span class="option-note"> (~3.4KB minified)</span>
        </span>
      </div>
      <div class="option">
        <span class="option-label">npm</span>
        <span class="option-value option-note">Coming soon</span>
      </div>
    </div>
  </div>

  <div class="card">
    <div class="card-header">2. Initialize &amp; authenticate</div>
    <div class="card-body">
      <div class="code-block" id="code">
        <button class="code-copy" id="copy-btn">Copy</button>
        <pre><span class="cm">// Initialize the bridge</span>
<span class="kw">const</span> bridge = <span class="kw">await</span> GateAuthBridge.<span class="fn">init</span>({
  serverUrl: <span class="str">'https://auth.getforma.dev'</span>,
  clientId:  <span class="str">'your-client-id'</span>,
});

<span class="cm">// Listen for state changes</span>
bridge.<span class="fn">onStateChange</span>(({ state, user }) => {
  <span class="kw">if</span> (state === <span class="str">'authenticated'</span>) {
    console.<span class="fn">log</span>(<span class="str">'Welcome,'</span>, user.name);
  }
});

<span class="cm">// Trigger login</span>
bridge.<span class="fn">login</span>(<span class="str">'google'</span>);  <span class="cm">// or 'github', 'email'</span></pre>
      </div>
    </div>
  </div>

  <div class="links">
    <a href="/docs" class="link-primary">API Documentation</a>
    <a href="/bridge.js" download class="link-outline">Download bridge.js</a>
  </div>

  <div class="footer">
    Powered by <a href="https://getforma.dev">Forma</a>
  </div>
</div>

<script>
document.getElementById('copy-btn').onclick = function() {
  var code = document.querySelector('#code pre').textContent;
  navigator.clipboard.writeText(code).catch(function() {});
  this.textContent = 'Copied!';
  var btn = this;
  setTimeout(function() { btn.textContent = 'Copy'; }, 2000);
};
</script>
</body>
</html>"##
        .to_string();

    Html(html)
}
