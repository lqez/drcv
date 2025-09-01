#!/usr/bin/env bash
# ------------------------------------------------------------
# DRCV Tunnel Server – Cloudflare Workers setup (Wrangler 4)
# ------------------------------------------------------------
set -euo pipefail

# ---------- 1. Fancy colors ----------
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'   # No Color

info()    { echo -e "${GREEN}✅ $1${NC}"; }
warn()    { echo -e "${YELLOW}⚠️  $1${NC}"; }
error()   { echo -e "${RED}❌ $1${NC}"; }

# ---------- 2. Check Wrangler ----------
if ! command -v wrangler >/dev/null 2>&1; then
  error "Wrangler CLI not found."
  echo "Install it with: npm install -g wrangler"
  exit 1
fi

WRANGLER_VER=$(wrangler -v | awk '{print $2}')
info "Wrangler CLI found (v${WRANGLER_VER})"

# ---------- 3. Authenticate ----------
warn "Checking Cloudflare authentication..."
if ! wrangler whoami >/dev/null 2>&1; then
  warn "You are not logged in – opening login flow."
  wrangler login
fi
info "Authenticated with Cloudflare."

# ---------- 4. Create KV namespace ----------
warn "Creating KV namespace 'TUNNEL_MAPPINGS'..."
if ! wrangler kv namespace create TUNNEL_MAPPINGS >/dev/null 2>&1; then
  warn "Namespace may already exist."
fi

# KV namespace ID는 list에서 가져옴
KV_ID=$(wrangler kv namespace list | jq -r '.[] | select(.title=="TUNNEL_MAPPINGS") | .id')

if [[ -z "$KV_ID" || "$KV_ID" == "null" ]]; then
  error "Failed to create or locate KV namespace."
  exit 1
fi
info "KV namespace ready (ID: $KV_ID)"

# ---------- 5. Update wrangler.toml ----------
if grep -q "YOUR_KV_NAMESPACE_ID" wrangler.toml; then
  sed -i.bak "s/YOUR_KV_NAMESPACE_ID/$KV_ID/g" wrangler.toml
  rm -f wrangler.toml.bak
  info "wrangler.toml updated with KV ID."
else
  warn "No placeholder found in wrangler.toml – you may need to add the binding manually."
  cat <<EOF

Add this under the appropriate environment (e.g. [env.production]):

[[kv_namespaces]]
binding = "TUNNEL_MAPPINGS"
id = "$KV_ID"

EOF
fi

# ---------- 6. Set Secrets ----------
warn "Setting up Cloudflare secrets (will be stored encrypted)."

read -rp "Enter your Cloudflare Zone ID: " ZONE_ID
read -rsp "Enter your Cloudflare API Token (Zone:Edit required): " API_TOKEN
echo    # newline

echo "$ZONE_ID" | wrangler secret put CLOUDFLARE_ZONE_ID
echo "$API_TOKEN" | wrangler secret put CLOUDFLARE_API_TOKEN
info "Secrets stored."

# ---------- 7. Deploy ----------
warn "Deploying the Worker..."
wrangler deploy
info "Deployment finished."

# ---------- 8. Finish ----------
cat <<EOF

${YELLOW}🎉 Setup completed!${NC}
Your tunnel server should now be reachable at:
https://drcv-tunnel-server.YOUR_SUBDOMAIN.workers.dev

Next steps:
  1️⃣ Update your DRCV client to point at the new URL.
  2️⃣ Make sure your domain’s DNS is managed by Cloudflare.
  3️⃣ Test the tunnel (e.g. curl https://.../health).

EOF
