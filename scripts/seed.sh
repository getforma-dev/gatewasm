#!/usr/bin/env bash
# seed.sh — Populate local dev database with test tenants and users.
#
# Usage:
#   npm run db:seed            (from project root)
#   ./scripts/seed.sh          (directly)
#   BASE=http://localhost:4500 ./scripts/seed.sh  (custom port)
#
# Requires: server running, curl, jq (optional, for pretty output)

set -euo pipefail

BASE="${BASE:-http://localhost:${PORT:-3001}}"
PLAT_KEY="${PLATFORM_ADMIN_KEY:-plat_local_dev_key}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

check_server() {
  if ! curl -sf "$BASE/health" > /dev/null 2>&1; then
    echo "Error: Server not reachable at $BASE/health"
    echo "Start it first: npm run db && npm run dev"
    exit 1
  fi
}

api() {
  local method="$1" path="$2" auth="$3"
  shift 3
  curl -sf -X "$method" "$BASE$path" \
    -H "Authorization: Bearer $auth" \
    -H "Content-Type: application/json" \
    "$@" 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# Seed
# ---------------------------------------------------------------------------

echo ""
echo -e "${CYAN}Seeding local dev database...${NC}"
echo -e "  Server: $BASE"
echo ""

check_server

# ── 1. Create "forma" tenant (dogfood / platform tenant) ──────────────────

echo -e "${YELLOW}1. Creating 'forma' tenant...${NC}"
FORMA_RESULT=$(api POST /platform/tenants "$PLAT_KEY" \
  -d '{
    "name": "Forma Dev",
    "slug": "forma",
    "owner_email": "admin@local.dev",
    "allowed_providers": ["email"]
  }')

FORMA_MGMT_KEY=$(echo "$FORMA_RESULT" | grep -o '"api_key":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
if [ -n "$FORMA_MGMT_KEY" ]; then
  echo -e "  ${GREEN}Created.${NC} mgmt key: $FORMA_MGMT_KEY"
else
  echo "  Already exists or created (no new key returned)"
  FORMA_MGMT_KEY=""
fi

# ── 2. Create "acme" tenant (test customer) ────────────────────────────────

echo -e "${YELLOW}2. Creating 'acme' test tenant...${NC}"
ACME_RESULT=$(api POST /platform/tenants "$PLAT_KEY" \
  -d '{
    "name": "Acme Corp",
    "slug": "acme",
    "owner_email": "admin@acme.test",
    "allowed_providers": ["email"]
  }')

ACME_MGMT_KEY=$(echo "$ACME_RESULT" | grep -o '"api_key":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
if [ -n "$ACME_MGMT_KEY" ]; then
  echo -e "  ${GREEN}Created.${NC} mgmt key: $ACME_MGMT_KEY"
else
  echo "  Already exists or created (no new key returned)"
  ACME_MGMT_KEY=""
fi

# ── 3. Register test users on "forma" tenant ───────────────────────────────

echo -e "${YELLOW}3. Registering test users on 'forma' tenant...${NC}"

# Admin user
ADMIN_RESULT=$(curl -sf -X POST "$BASE/auth/register" \
  -H "X-Tenant-Slug: forma" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "admin@local.dev",
    "password": "Test1234!",
    "name": "Admin User"
  }' 2>/dev/null || true)

if echo "$ADMIN_RESULT" | grep -q "access_token\|user_id" 2>/dev/null; then
  echo -e "  ${GREEN}admin@local.dev${NC} registered"
else
  echo "  admin@local.dev — already exists or registered"
fi

# Regular user
USER_RESULT=$(curl -sf -X POST "$BASE/auth/register" \
  -H "X-Tenant-Slug: forma" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@local.dev",
    "password": "Test1234!",
    "name": "Regular User"
  }' 2>/dev/null || true)

if echo "$USER_RESULT" | grep -q "access_token\|user_id" 2>/dev/null; then
  echo -e "  ${GREEN}user@local.dev${NC} registered"
else
  echo "  user@local.dev — already exists or registered"
fi

# ── 4. Promote admin user ──────────────────────────────────────────────────

echo -e "${YELLOW}4. Promoting admin@local.dev to admin role...${NC}"

docker compose exec -T db psql -U gatewasm -d gatewasm -c "
  INSERT INTO user_roles (user_id, role_id, tenant_id)
  SELECT u.id, 'role_admin', u.tenant_id
  FROM users u WHERE u.email = 'admin@local.dev'
  ON CONFLICT DO NOTHING;
" > /dev/null 2>&1 && echo -e "  ${GREEN}Done${NC}" || echo "  Already has admin role"

# ── 5. Register test user on "acme" tenant ─────────────────────────────────

echo -e "${YELLOW}5. Registering test user on 'acme' tenant...${NC}"

ACME_USER_RESULT=$(curl -sf -X POST "$BASE/auth/register" \
  -H "X-Tenant-Slug: acme" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "john@acme.test",
    "password": "Test1234!",
    "name": "John Doe"
  }' 2>/dev/null || true)

if echo "$ACME_USER_RESULT" | grep -q "access_token\|user_id" 2>/dev/null; then
  echo -e "  ${GREEN}john@acme.test${NC} registered"
else
  echo "  john@acme.test — already exists or registered"
fi

# ── Summary ────────────────────────────────────────────────────────────────

echo ""
echo -e "${GREEN}=== Seed Complete ===${NC}"
echo ""
echo "  Tenants:"
echo "    forma  — $BASE/t/forma/login"
echo "    acme   — $BASE/t/acme/login"
echo ""
echo "  Test Accounts (password: Test1234!):"
echo "    admin@local.dev   — forma tenant, admin role"
echo "    user@local.dev    — forma tenant, regular user"
echo "    john@acme.test    — acme tenant, regular user"
echo ""
echo "  Platform admin:"
echo "    $BASE/platform/login"
echo ""
if [ -n "$FORMA_MGMT_KEY" ]; then
  echo "  Management keys:"
  echo "    forma: $FORMA_MGMT_KEY"
  [ -n "$ACME_MGMT_KEY" ] && echo "    acme:  $ACME_MGMT_KEY"
  echo ""
fi
