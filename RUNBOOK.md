# GateWASM Auth — Deployment Runbook

Operations guide for deploying and managing the GateWASM auth server on AWS.

**Architecture:** App Runner + RDS PostgreSQL + fck-nat + Route 53
**Domain:** `auth.getforma.dev`
**Region:** `us-east-1`
**Account:** `877573132739`
**Estimated cost:** ~$23/mo (~$11/mo year 1 with RDS free tier credits)

Production hardening status (March 2026):
- Immutable image tags + explicit rollback script are now the default deployment path.
- Staging environment support exists in CDK (`gatewasm:staging:*` contexts) and should gate prod promotion.
- Auth alerts are provisioned from App Runner application logs (`missing_cookie`, MFA failures, token errors).
- A synthetic owner session canary runs every few minutes when `ownerSessionSecretArn` is configured.

---

## Quick Commands (Cheat Sheet)

```bash
# --- AWS Login ---
aws configure                # First time: set access key, secret, region
aws sts get-caller-identity  # Verify you're authenticated

# --- ECR Login (required before every docker push) ---
aws ecr get-login-password --region us-east-1 | \
  docker login --username AWS --password-stdin 877573132739.dkr.ecr.us-east-1.amazonaws.com

# --- Build & Deploy Code ---
cd /path/to/forma-code/auth-module-poc
TAG=$(./scripts/image_tag.sh)
./scripts/build_and_push_image.sh "$TAG"
./scripts/deploy_service_stack.sh staging "$TAG"
BASE_URL=https://auth-staging.getforma.dev ./scripts/smoke_auth_flow.sh
./scripts/deploy_service_stack.sh prod "$TAG"

# --- Deploy Infrastructure Changes ---
cd auth-module-poc/infra
cdk diff --all -c gatewasm:imageTag=$TAG   # Preview what will change
cdk deploy --all -c gatewasm:imageTag=$TAG # Apply changes

# --- Health Check ---
curl https://auth.getforma.dev/health

# --- View Logs ---
aws logs tail /aws/apprunner/gatewasm-server/service --follow

# --- Force Restart ---
aws apprunner start-deployment --service-arn arn:aws:apprunner:us-east-1:877573132739:service/gatewasm-server/7cf6cfb205964690b4672686d06c5379

# --- Check Costs ---
aws ce get-cost-and-usage \
  --time-period Start=$(date +%Y-%m-01),End=$(date +%Y-%m-%d) \
  --granularity MONTHLY --metrics BlendedCost \
  --group-by Type=DIMENSION,Key=SERVICE --output table
```

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [First-Time Setup](#2-first-time-setup)
3. [Deploy Infrastructure](#3-deploy-infrastructure)
4. [Build & Push Docker Image](#4-build--push-docker-image)
5. [Populate Secrets](#5-populate-secrets)
6. [Configure DNS](#6-configure-dns)
7. [Verify Deployment](#7-verify-deployment)
8. [Subsequent Deploys](#8-subsequent-deploys)
9. [Monitoring & Logs](#9-monitoring--logs)
10. [Troubleshooting](#10-troubleshooting)
11. [Rollback](#11-rollback)
12. [Infrastructure Changes](#12-infrastructure-changes)
13. [Cost Management](#13-cost-management)
14. [Secrets Rotation](#14-secrets-rotation)
15. [Database Operations](#15-database-operations)
16. [Tear Down](#16-tear-down)

---

## 1. Prerequisites

### Tools

```bash
# AWS CLI v2
brew install awscli
aws --version    # aws-cli/2.x.x

# AWS CDK
npm install -g aws-cdk
cdk --version    # 2.x.x

# Docker
docker --version # Docker 24+

# Node.js (for CDK)
node --version   # v18+ or v20+
```

### AWS Authentication

```bash
# Configure credentials
aws configure
# AWS Access Key ID: <your-key>
# AWS Secret Access Key: <your-secret>
# Default region: us-east-1
# Default output: json

# Verify
aws sts get-caller-identity
# Expected: account 877573132739
```

### Repository

```bash
cd /path/to/forma-code/auth-module-poc/infra
npm install
```

---

## 2. First-Time Setup

### 2a. Bootstrap CDK

CDK bootstrap creates an S3 bucket and IAM roles that CDK uses for deployments. Run once per account/region.

```bash
cd auth-module-poc/infra
cdk bootstrap aws://877573132739/us-east-1
```

Expected output: `Environment aws://877573132739/us-east-1 bootstrapped.`

### 2b. Generate Ed25519 Key

The auth server uses an Ed25519 private key for JWT signing. Generate one before deploying:

```bash
openssl genpkey -algorithm ed25519 -out /tmp/ed25519.pem
cat /tmp/ed25519.pem
# -----BEGIN PRIVATE KEY-----
# MC4CAQAwBQYDK2VwBCIEI...
# -----END PRIVATE KEY-----
```

Keep this file until Step 5 (secrets population). Delete after.

---

## 3. Deploy Infrastructure

### 3a. Preview Changes

```bash
cd auth-module-poc/infra
cdk diff --all -c gatewasm:imageTag=$(../scripts/image_tag.sh)
```

This shows what CloudFormation resources will be created/modified/deleted without making any changes.

### 3b. Deploy

```bash
TAG=$(../scripts/image_tag.sh)
cdk deploy --all -c gatewasm:imageTag=$TAG
```

The infrastructure is split into 4 stacks: `GatewasmNetworkStack`, `GatewasmDatabaseStack`, `GatewasmServiceStack`, `GatewasmCdnStack`. Using `--all` deploys them in dependency order.

This creates all AWS resources:
- VPC (2 AZs, public + private subnets, fck-nat t4g.nano for internet egress)
- RDS PostgreSQL 16 (t4g.micro, private subnet, encrypted)
- 6 required Secrets Manager secrets (+ optional SMTP secret for outbound auth email delivery)
- ECR repository
- App Runner service + VPC Connector + Auto Scaling (max 2 instances)
- Route 53 hosted zone

**Deployment takes 10-15 minutes** (RDS creation is the slowest part).

### 3c. Note the Outputs

After deploy, CDK prints outputs:

```
GatewasmStack.ServiceServiceUrl = https://xxxxx.us-east-1.awsapprunner.com
GatewasmStack.ServiceEcrRepoUri = 877573132739.dkr.ecr.us-east-1.amazonaws.com/gatewasm-server
GatewasmStack.DnsNameservers = ns-123.awsdns-45.com, ns-678.awsdns-90.net, ...
GatewasmStack.DnsAuthDomain = auth.getforma.dev
```

Save the **Nameservers** — you'll need them for DNS configuration (Step 6).

### 3d. Enable Staging Environment (Recommended)

Set the following in `infra/cdk.json` (or override via `cdk -c ...`) before first staging deploy:
- `gatewasm:staging:enabled=true`
- `gatewasm:staging:secretArns` (staging-specific secret ARNs; supports optional `Smtp`)
- `gatewasm:staging:authSubdomain` (default: `auth-staging`)
- `gatewasm:staging:platformOwnerEmails` (owner/admin emails for owner-plane access control)
- `gatewasm:staging:alertsEmail` (optional)
- `gatewasm:staging:ownerSessionSecretArn` (required to enable the synthetic owner-session canary)

Then deploy:

```bash
cd auth-module-poc
TAG=$(./scripts/image_tag.sh)
./scripts/deploy_service_stack.sh staging "$TAG"
```

---

## 4. Build & Push Docker Image

### 4a. Authenticate Docker to ECR

```bash
aws ecr get-login-password --region us-east-1 | \
  docker login --username AWS --password-stdin \
  877573132739.dkr.ecr.us-east-1.amazonaws.com
```

### 4b. Build & Push

Build from the **monorepo root** (the Dockerfile references files outside `auth-module-poc/`):

```bash
cd /path/to/forma-code/auth-module-poc
TAG=$(./scripts/image_tag.sh)
./scripts/build_and_push_image.sh "$TAG"
```

**Important:** Use `--platform linux/amd64` on Apple Silicon Macs. App Runner runs x86_64 instances.

**Build time:** ~5-10 minutes (first build downloads Rust crate dependencies).

### 4c. Verify Image in ECR

```bash
aws ecr describe-images \
  --repository-name gatewasm-server \
  --query 'imageDetails[*].{Tag:imageTags[0],Size:imageSizeInBytes,Pushed:imagePushedAt}' \
  --output table
```

---

## 5. Populate Secrets

After the first deploy, several secrets need manual population.

### 5a. Ed25519 Private Key (Required)

```bash
aws secretsmanager put-secret-value \
  --secret-id gatewasm/ed25519-key \
  --secret-string "$(cat /tmp/ed25519.pem)"

# Clean up local key
rm /tmp/ed25519.pem
```

### 5b. Google OAuth (Optional)

Get credentials from [Google Cloud Console](https://console.cloud.google.com/apis/credentials):
- Create OAuth 2.0 Client ID (Web application)
- Authorized redirect URI: `https://auth.getforma.dev/auth/callback/google`

```bash
aws secretsmanager put-secret-value \
  --secret-id gatewasm/oauth-google \
  --secret-string '{"client_id":"YOUR_GOOGLE_CLIENT_ID","client_secret":"YOUR_GOOGLE_SECRET"}'
```

### 5c. GitHub OAuth (Optional)

Get credentials from [GitHub Developer Settings](https://github.com/settings/developers):
- Create OAuth App
- Authorization callback URL: `https://auth.getforma.dev/auth/callback/github`

```bash
aws secretsmanager put-secret-value \
  --secret-id gatewasm/oauth-github \
  --secret-string '{"client_id":"YOUR_GITHUB_CLIENT_ID","client_secret":"YOUR_GITHUB_SECRET"}'
```

### 5d. Verify Secrets

```bash
# List all gatewasm secrets
aws secretsmanager list-secrets \
  --filters Key=name,Values=gatewasm \
  --query 'SecretList[*].{Name:Name,Created:CreatedDate}' \
  --output table

# Test reading a secret (non-sensitive)
aws secretsmanager get-secret-value \
  --secret-id gatewasm/csrf-secret \
  --query 'SecretString' --output text
```

### 5e. SMTP Secret (Optional, Required for Verification + Reset Emails)

Create `gatewasm/smtp` with JSON fields:

```json
{
  "host": "smtp.gmail.com",
  "username": "you@getforma.dev",
  "password": "app-password-or-provider-token",
  "from_email": "no-reply@getforma.dev"
}
```

Populate/update the secret:

```bash
aws secretsmanager put-secret-value \
  --secret-id gatewasm/smtp \
  --secret-string '{"host":"smtp.gmail.com","username":"you@getforma.dev","password":"REPLACE_ME","from_email":"no-reply@getforma.dev"}'
```

Then set `Smtp` in `gatewasm:secretArns` (and staging map if needed) to the full ARN for `gatewasm/smtp`, and redeploy the service stack.

### 5f. Auto-Generated Secrets

These secrets are auto-generated by CDK and don't need manual population:
- `gatewasm/csrf-secret` — 44-character random string
- `gatewasm/platform-admin-key` — 40-character random string (use this as your `plat_` admin key)
- `gatewasm/rds-credentials` — RDS username, password, host, port, dbname

### 5g. Security Feature Env Vars (Optional)

These environment variables enable optional security hardening features. The server starts and operates normally without them — the corresponding features are simply unavailable.

| Variable | Purpose | Example |
|----------|---------|---------|
| `SSF_SIGNING_KEY` | PEM-encoded Ed25519 private key used exclusively for signing Security Event Tokens (SSF/SET). Keeps SSF signatures independent of the main JWT signing key. | Contents of an Ed25519 PEM file |
| `MAXMIND_DB_PATH` | Filesystem path to a MaxMind GeoIP `.mmdb` database. Used by the impossible-travel detection engine to geolocate login IPs. | `/data/GeoLite2-City.mmdb` |

To configure on AWS, add the values to App Runner environment variables (see Section 12 — "Adding Environment Variables") or store them in Secrets Manager and reference the ARNs in `infra/lib/constructs/service.ts`.

---

## 6. Configure DNS

### 6a. Update Nameservers at Porkbun

1. Go to [Porkbun Dashboard](https://porkbun.com/account/domainsSold) → `getforma.dev`
2. Click **DNS** → **Nameservers**
3. Replace existing nameservers with the 4 AWS nameservers from the CDK output:
   ```
   ns-XXX.awsdns-XX.com
   ns-XXX.awsdns-XX.net
   ns-XXX.awsdns-XX.org
   ns-XXX.awsdns-XX.co.uk
   ```
4. Save changes

**DNS propagation takes 15 minutes to 48 hours.** Usually under 1 hour.

### 6b. Verify DNS

```bash
# Check NS records propagated
dig NS getforma.dev

# Check auth CNAME resolves
dig CNAME auth.getforma.dev

# Should return the App Runner domain
```

### 6c. App Runner Custom Domain (Optional)

To serve traffic directly on `auth.getforma.dev` with App Runner's managed TLS certificate:

```bash
# Get the App Runner service ARN
aws apprunner list-services \
  --query 'ServiceSummaryList[?ServiceName==`gatewasm-server`].ServiceArn' \
  --output text

# Associate custom domain
aws apprunner associate-custom-domain \
  --service-arn arn:aws:apprunner:us-east-1:877573132739:service/gatewasm-server/7cf6cfb205964690b4672686d06c5379 \
  --domain-name auth.getforma.dev

# This returns DNS validation records — add them to Route 53
# App Runner auto-provisions an ACM certificate
```

---

## 7. Verify Deployment

### 7a. Health Check

```bash
# Via App Runner default domain
curl https://<app-runner-url>/health
# Expected: {"status":"ok","db_status":"ok"}

# Via custom domain (after DNS propagation)
curl https://auth.getforma.dev/health
```

### 7b. JWKS Endpoint

```bash
curl https://auth.getforma.dev/auth/jwks | jq
# Expected: {"keys":[{"kty":"OKP","crv":"Ed25519","x":"..."}]}
```

### 7c. Create First Tenant

```bash
# Get the auto-generated platform admin key
PLAT_KEY=$(aws secretsmanager get-secret-value \
  --secret-id gatewasm/platform-admin-key \
  --query 'SecretString' --output text)

# Create a tenant
curl -X POST https://auth.getforma.dev/platform/tenants \
  -H "Authorization: Bearer $PLAT_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My App",
    "slug": "myapp",
    "allowed_origins": ["https://myapp.com"],
    "allowed_providers": ["email"]
  }'
```

### 7d. Test Hosted Login

Open in browser: `https://auth.getforma.dev/t/myapp/login`

### 7e. Full Smoke Test

```bash
# Register a user
curl -X POST https://auth.getforma.dev/auth/register \
  -H "X-Tenant-Slug: myapp" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","password":"Test1234!","name":"Test User"}'

# Login (use -c to save the httpOnly refresh cookie)
curl -c cookies.txt -X POST https://auth.getforma.dev/auth/login \
  -H "X-Tenant-Slug: myapp" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","password":"Test1234!"}'
# Returns: access_token in JSON body, refresh_token as httpOnly Set-Cookie

# Refresh via cookie
curl -b cookies.txt -X POST https://auth.getforma.dev/auth/token \
  -H "X-Tenant-Slug: myapp" \
  -H "Content-Type: application/json" \
  -d '{"grant_type":"refresh_token"}'
```

---

## 8. Subsequent Deploys

### Recommended Promotion Flow (Immutable Tags)

```bash
cd /path/to/forma-code/auth-module-poc

# 1. Build immutable image
TAG=$(./scripts/image_tag.sh)
./scripts/build_and_push_image.sh "$TAG"

# 2. Deploy staging and run smoke
./scripts/deploy_service_stack.sh staging "$TAG"
BASE_URL=https://auth-staging.getforma.dev ./scripts/smoke_auth_flow.sh

# 3. Promote same immutable image to prod
./scripts/deploy_service_stack.sh prod "$TAG"
```

### Infrastructure-Only Changes

```bash
cd auth-module-poc/infra
TAG=$(../scripts/image_tag.sh)
cdk diff --all -c gatewasm:imageTag=$TAG
cdk deploy --all -c gatewasm:imageTag=$TAG
```

### GitHub Actions Deployment Path

Use `.github/workflows/deploy-auth.yml`:
- Build/push immutable tag.
- Deploy staging.
- Run `smoke_auth_flow.sh`.
- Promote same tag to production.

---

## 9. Monitoring & Logs

### Auth Failure Alerts (CloudWatch + SNS)

The service stack now creates metric filters and alarms for:
- `missing_cookie` (owner session gate failures)
- MFA failures (`invalid_mfa_code`, `mfa_locked`)
- token errors (`invalid_grant`, `token_reuse_detected`, refresh rejects)

Set `gatewasm:alertsEmail` (and `gatewasm:staging:alertsEmail`) in `infra/cdk.json` or override via `-c`.

### App Runner Logs

```bash
# Stream logs
aws logs tail /aws/apprunner/gatewasm-server/service --follow

# Search recent logs
aws logs filter-log-events \
  --log-group-name /aws/apprunner/gatewasm-server/service \
  --start-time $(date -v-1H +%s000) \
  --filter-pattern "ERROR"
```

### App Runner Service Status

```bash
aws apprunner describe-service \
  --service-arn arn:aws:apprunner:us-east-1:877573132739:service/gatewasm-server/7cf6cfb205964690b4672686d06c5379 \
  --query 'Service.{Status:Status,Updated:UpdatedAt,URL:ServiceUrl}' \
  --output table
```

### Synthetic Owner Session Check

- CDK can provision a canary when `ownerSessionSecretArn` is configured.
- Canary cadence defaults to 5 minutes.
- The canary rotates a stored owner refresh token and validates `/platform/admin/dashboard` in a headless browser.

Owner session secret JSON shape:

```json
{
  "owner_refresh_token": "rt_...",
  "last_checked_at": "2026-03-07T00:00:00Z"
}
```

Repository-level fallback synthetic monitor is also available:
- `.github/workflows/synthetic-owner-session.yml` (every 5 minutes)

### RDS Monitoring

```bash
# Connection count
aws cloudwatch get-metric-statistics \
  --namespace AWS/RDS \
  --metric-name DatabaseConnections \
  --dimensions Name=DBInstanceIdentifier,Value=<RDS_INSTANCE_ID> \
  --start-time $(date -v-1H -u +%Y-%m-%dT%H:%M:%SZ) \
  --end-time $(date -u +%Y-%m-%dT%H:%M:%SZ) \
  --period 300 --statistics Maximum \
  --output table

# CPU utilization
aws cloudwatch get-metric-statistics \
  --namespace AWS/RDS \
  --metric-name CPUUtilization \
  --dimensions Name=DBInstanceIdentifier,Value=<RDS_INSTANCE_ID> \
  --start-time $(date -v-1H -u +%Y-%m-%dT%H:%M:%SZ) \
  --end-time $(date -u +%Y-%m-%dT%H:%M:%SZ) \
  --period 300 --statistics Average \
  --output table
```

### Health Check

```bash
# Quick health check (run from anywhere)
curl -s https://auth.getforma.dev/health | jq

# Continuous monitoring (every 30s)
watch -n 30 'curl -s https://auth.getforma.dev/health | jq'
```

---

## 10. Troubleshooting

### App Runner Won't Start

**Symptom:** Service status stuck in `OPERATION_IN_PROGRESS` or `CREATE_FAILED`.

1. Check logs for startup errors:
   ```bash
   aws logs tail /aws/apprunner/gatewasm-server/service --since 10m
   ```
2. Common causes:
   - **No image in ECR** — Push an image first (Step 4)
   - **Missing secrets** — Populate Ed25519 key (Step 5)
   - **RDS not reachable** — Check VPC Connector security groups
   - **Wrong platform** — Ensure image built with `--platform linux/amd64`

### Database Connection Fails

**Symptom:** Health check returns `{"status":"ok","db_status":"error"}` or server won't start.

1. Verify VPC Connector is attached:
   ```bash
   aws apprunner describe-service --service-arn arn:aws:apprunner:us-east-1:877573132739:service/gatewasm-server/7cf6cfb205964690b4672686d06c5379 \
     --query 'Service.NetworkConfiguration'
   ```
2. Verify security groups allow 5432:
   ```bash
   aws ec2 describe-security-groups \
     --filters Name=group-name,Values='*gatewasm*' \
     --query 'SecurityGroups[*].{Name:GroupName,ID:GroupId,Rules:IpPermissions}'
   ```
3. Verify RDS is running:
   ```bash
   aws rds describe-db-instances \
     --query 'DBInstances[*].{ID:DBInstanceIdentifier,Status:DBInstanceStatus,Endpoint:Endpoint.Address}'
   ```

### DNS Not Resolving

1. Check nameservers propagated:
   ```bash
   dig NS getforma.dev +short
   ```
2. If still showing Porkbun nameservers, wait longer (up to 48h) or verify the values at Porkbun match the CDK output.
3. Check the CNAME record exists:
   ```bash
   aws route53 list-resource-record-sets \
     --hosted-zone-id <ZONE_ID> \
     --query 'ResourceRecordSets[?Name==`auth.getforma.dev.`]'
   ```

### Deploy Fails with "already exists"

**Symptom:** `cdk deploy` fails with `Resource ... already exists` for Secrets Manager, ECR, or other resources.

**Cause:** A previous failed deploy created resources that survived the stack rollback (Secrets Manager and ECR have `RemovalPolicy.RETAIN`).

**Fix:** Delete the orphaned resources, then redeploy:

```bash
# 1. Delete the failed stack
aws cloudformation delete-stack --stack-name GatewasmStack --region us-east-1
aws cloudformation wait stack-delete-complete --stack-name GatewasmStack --region us-east-1

# 2. Delete orphaned secrets
for secret in gatewasm/oauth-google gatewasm/oauth-github gatewasm/ed25519-key \
              gatewasm/csrf-secret gatewasm/platform-admin-key gatewasm/smtp; do
  aws secretsmanager delete-secret --secret-id "$secret" \
    --force-delete-without-recovery --region us-east-1 2>/dev/null
done

# 3. Delete orphaned ECR repo (if empty)
aws ecr delete-repository --repository-name gatewasm-server \
  --region us-east-1 --force 2>/dev/null

# 4. Redeploy
cdk deploy
```

### OAuth Callback Fails

1. Verify `SERVER_URL` is set to `https://auth.getforma.dev`
2. Verify OAuth credentials are populated in Secrets Manager
3. Check redirect URIs match exactly in Google/GitHub OAuth app settings:
   - Google: `https://auth.getforma.dev/auth/callback/google`
   - GitHub: `https://auth.getforma.dev/auth/callback/github`

### High Latency

1. Check RDS CPU/connections (see Monitoring section)
2. Check App Runner scaling:
   ```bash
   aws apprunner describe-service --service-arn arn:aws:apprunner:us-east-1:877573132739:service/gatewasm-server/7cf6cfb205964690b4672686d06c5379 \
     --query 'Service.AutoScalingConfigurationSummary'
   ```
3. Current limits: min 1, max 2 instances, 100 concurrent requests per instance

---

## 11. Rollback

### Roll Back Code (Immutable Tag)

```bash
# List available images
aws ecr describe-images \
  --repository-name gatewasm-server \
  --query 'sort_by(imageDetails,&imagePushedAt)[*].{Tag:imageTags[0],Digest:imageDigest,Pushed:imagePushedAt}' \
  --output table

# Deploy a previous immutable tag directly
cd /path/to/forma-code/auth-module-poc
./scripts/rollback_to_tag.sh prod release-20260307-131200-abcd1234
```

### Roll Back Infrastructure

```bash
cd auth-module-poc/infra

# Check git history for previous CDK state
git log --oneline lib/

# Revert to previous commit
git checkout <COMMIT_HASH> -- lib/

# Deploy reverted infrastructure
TAG=$(../scripts/image_tag.sh)
cdk diff --all -c gatewasm:imageTag=$TAG    # Review changes
cdk deploy --all -c gatewasm:imageTag=$TAG  # Apply
```

---

## 12. Infrastructure Changes

### Scaling App Runner

Edit `infra/lib/constructs/service.ts`:

```typescript
// Change instance size
instanceConfiguration: {
  cpu: '1 vCPU',      // was 0.25 vCPU
  memory: '2 GB',     // was 0.5 GB
}
```

Then: `TAG=$(../scripts/image_tag.sh) && cdk deploy GatewasmServiceStack -c gatewasm:imageTag=$TAG`

### Scaling RDS

Edit `infra/lib/constructs/database.ts`:

```typescript
// Upgrade instance
instanceType: ec2.InstanceType.of(ec2.InstanceClass.T4G, ec2.InstanceSize.SMALL),
```

Then: `TAG=$(../scripts/image_tag.sh) && cdk deploy GatewasmServiceStack -c gatewasm:imageTag=$TAG` (causes brief downtime for instance modification).

### Adding Environment Variables

Edit `infra/lib/constructs/service.ts`, add to `runtimeEnvironmentVariables` array:

```typescript
{ name: 'NEW_VAR', value: 'value' },
```

Then: `TAG=$(../scripts/image_tag.sh) && cdk deploy GatewasmServiceStack -c gatewasm:imageTag=$TAG`

---

## 13. Cost Management

### Monthly Cost Breakdown

| Resource | Monthly | Notes |
|----------|---------|-------|
| App Runner (1 instance, idle) | ~$5 | $0.007/vCPU-hr idle |
| RDS t4g.micro | ~$12 | $0 year 1 with free tier credits |
| RDS storage (20GB gp3) | ~$2 | Grows up to 50GB |
| fck-nat t4g.nano | ~$3 | Replaces managed NAT Gateway ($32/mo) |
| Route 53 hosted zone | $0.50 | Per zone |
| Secrets Manager (6 required + optional SMTP) | ~$2.40+ | $0.40/secret/month |
| ECR (~1GB) | ~$0.10 | $0.10/GB/month |
| CloudWatch Logs | ~$0.50 | Log ingestion |
| **Total** | **~$23/mo** | **~$11/mo year 1** |

### Cost Protection

- **Auto-scaling capped at 2 instances** — prevents DDoS or traffic spikes from scaling to 25 instances (the default) and running up hundreds in hourly charges
- **No build fees** — we deploy pre-built Docker images from ECR, not from GitHub source. App Runner only charges build fees for source-based deploys
- **fck-nat instead of managed NAT Gateway** — saves ~$29/mo. Swap to managed NAT when revenue justifies it (edit `networking.ts`, remove `FckNatInstanceProvider`, set `natGateways: 1`)

### Cost Optimization Tips

1. **Delete unused ECR images** — lifecycle policy keeps only last 10
2. **RDS right-sizing** — t4g.micro handles ~50 concurrent connections; upgrade only when needed
3. **App Runner min instances** — set to 1 (already minimal); 0 is not supported
4. **Secrets consolidation** — consider combining OAuth secrets into one JSON secret
5. **Log retention** — set CloudWatch log retention to 7 or 14 days:
   ```bash
   aws logs put-retention-policy \
     --log-group-name /aws/apprunner/gatewasm-server/service \
     --retention-in-days 14
   ```

### Monitor Spend

```bash
# Current month's cost (updated daily)
aws ce get-cost-and-usage \
  --time-period Start=$(date +%Y-%m-01),End=$(date +%Y-%m-%d) \
  --granularity MONTHLY \
  --metrics BlendedCost \
  --group-by Type=DIMENSION,Key=SERVICE \
  --output table
```

---

## 14. Secrets Rotation

### Ed25519 Key Rotation

Rotating the JWT signing key invalidates all existing access tokens. Plan for a brief window where users need to re-authenticate.

```bash
# 1. Generate new key
openssl genpkey -algorithm ed25519 -out /tmp/ed25519-new.pem

# 2. Update secret
aws secretsmanager put-secret-value \
  --secret-id gatewasm/ed25519-key \
  --secret-string "$(cat /tmp/ed25519-new.pem)"
rm /tmp/ed25519-new.pem

# 3. Restart App Runner to pick up new key
# (force a new deployment)
aws apprunner start-deployment --service-arn arn:aws:apprunner:us-east-1:877573132739:service/gatewasm-server/7cf6cfb205964690b4672686d06c5379
```

### RDS Password Rotation

```bash
# Rotate automatically via Secrets Manager
aws secretsmanager rotate-secret \
  --secret-id gatewasm/rds-credentials

# Restart App Runner to pick up new credentials
aws apprunner start-deployment --service-arn arn:aws:apprunner:us-east-1:877573132739:service/gatewasm-server/7cf6cfb205964690b4672686d06c5379
```

### Platform Admin Key

```bash
# Generate new key
NEW_KEY="plat_$(openssl rand -hex 20)"

# Update secret
aws secretsmanager put-secret-value \
  --secret-id gatewasm/platform-admin-key \
  --secret-string "$NEW_KEY"

# Restart service
aws apprunner start-deployment --service-arn arn:aws:apprunner:us-east-1:877573132739:service/gatewasm-server/7cf6cfb205964690b4672686d06c5379

echo "New platform admin key: $NEW_KEY"
```

---

## 15. Database Operations

### Connect to RDS (via Bastion)

RDS is in a private subnet with no public access. To connect directly:

```bash
# Option 1: Use AWS Systems Manager Session Manager port forwarding
# (requires a bastion EC2 instance in the VPC with SSM agent)

# Option 2: Temporary bastion
# Launch a small EC2 in the public subnet of the same VPC,
# then SSH tunnel through it to RDS on port 5432.

# Get RDS endpoint
aws rds describe-db-instances \
  --query 'DBInstances[0].Endpoint' --output json

# Get credentials
aws secretsmanager get-secret-value \
  --secret-id gatewasm/rds-credentials \
  --query 'SecretString' --output text | jq
```

### Run Migrations

Migrations run automatically on server startup. To force a migration run, restart the service:

```bash
aws apprunner start-deployment --service-arn arn:aws:apprunner:us-east-1:877573132739:service/gatewasm-server/7cf6cfb205964690b4672686d06c5379
```

### Database Backup

RDS automated backups are enabled (7-day retention). To create a manual snapshot:

```bash
aws rds create-db-snapshot \
  --db-instance-identifier <INSTANCE_ID> \
  --db-snapshot-identifier gatewasm-manual-$(date +%Y%m%d)
```

### Restore from Snapshot

```bash
aws rds restore-db-instance-from-db-snapshot \
  --db-instance-identifier gatewasm-restored \
  --db-snapshot-identifier <SNAPSHOT_ID>
```

### Restore Drill (Scripted)

Run a live restore drill and capture timings:

```bash
cd /path/to/forma-code/auth-module-poc
./scripts/db_restore_drill.sh --source-instance gatewasmstack-databaseinstanceaa8a5fde-vv99npqcxrxz
```

The script:
1. creates a manual snapshot
2. restores a temporary drill instance
3. waits for readiness and reports timing
4. deletes the drill instance (unless `--keep-restored` is set)

### Recovery Targets (RTO / RPO)

- **Target RTO:** <= 60 minutes for full service recovery from snapshot.
- **Target RPO:** <= 5 minutes using RDS automated backups + PITR.
- **Backup retention:** 7 days (configured in CDK).

Latest drill result (2026-03-07 UTC):
- Snapshot: `gatewasmstack-databaseinstanceaa8a5fde-vv99npqcxrxz-drill-20260307200903`
- Snapshot creation: 107s
- Restore availability: 457s
- Total measured RTO: 564s (9m24s)

Track each run in your incident log (date, snapshot time, restore time, total RTO, cleanup status).

---

## 16. Tear Down

To completely remove all infrastructure:

```bash
cd auth-module-poc/infra

# 1. Disable deletion protection on RDS first
aws rds modify-db-instance \
  --db-instance-identifier <INSTANCE_ID> \
  --no-deletion-protection

# 2. Delete all ECR images (required before stack deletion)
aws ecr batch-delete-image \
  --repository-name gatewasm-server \
  --image-ids "$(aws ecr list-images --repository-name gatewasm-server --query 'imageIds' --output json)"

# 3. Destroy the stack
cdk destroy

# Note: Resources with RemovalPolicy.RETAIN (RDS, Secrets) are NOT deleted
# by cdk destroy. Delete them manually if needed:
aws rds delete-db-instance \
  --db-instance-identifier <INSTANCE_ID> \
  --skip-final-snapshot

aws secretsmanager delete-secret --secret-id gatewasm/ed25519-key --force-delete-without-recovery
aws secretsmanager delete-secret --secret-id gatewasm/csrf-secret --force-delete-without-recovery
aws secretsmanager delete-secret --secret-id gatewasm/platform-admin-key --force-delete-without-recovery
aws secretsmanager delete-secret --secret-id gatewasm/oauth-google --force-delete-without-recovery
aws secretsmanager delete-secret --secret-id gatewasm/oauth-github --force-delete-without-recovery
aws secretsmanager delete-secret --secret-id gatewasm/rds-credentials --force-delete-without-recovery
```

---

## Quick Reference

| Task | Command |
|------|---------|
| Build immutable tag | `cd auth-module-poc && TAG=$(./scripts/image_tag.sh) && ./scripts/build_and_push_image.sh "$TAG"` |
| Deploy staging | `cd auth-module-poc && ./scripts/deploy_service_stack.sh staging "$TAG"` |
| Run staging smoke | `cd auth-module-poc && BASE_URL=https://auth-staging.getforma.dev ./scripts/smoke_auth_flow.sh` |
| Promote prod | `cd auth-module-poc && ./scripts/deploy_service_stack.sh prod "$TAG"` |
| Rollback | `cd auth-module-poc && ./scripts/rollback_to_tag.sh prod <immutable-tag>` |
| ECR login | `aws ecr get-login-password --region us-east-1 \| docker login --username AWS --password-stdin 877573132739.dkr.ecr.us-east-1.amazonaws.com` |
| Health check | `curl https://auth.getforma.dev/health` |
| View logs | `aws logs tail /aws/apprunner/gatewasm-server/service --follow` |
| Force restart | `aws apprunner start-deployment --service-arn arn:aws:apprunner:us-east-1:877573132739:service/gatewasm-server/7cf6cfb205964690b4672686d06c5379` |
| Preview changes | `cd auth-module-poc/infra && cdk diff --all -c gatewasm:imageTag=$TAG` |
| Check cost | `aws ce get-cost-and-usage --time-period Start=$(date +%Y-%m-01),End=$(date +%Y-%m-%d) --granularity MONTHLY --metrics BlendedCost` |
