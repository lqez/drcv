#!/bin/bash

set -e

echo "🚀 Setting up DRCV Tunnel Server on Cloudflare Workers"

# 색상 정의
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Wrangler 설치 확인
if ! command -v wrangler &> /dev/null; then
    echo -e "${RED}❌ Wrangler CLI not found${NC}"
    echo "Install it with: npm install -g wrangler"
    exit 1
fi

echo -e "${GREEN}✅ Wrangler CLI found${NC}"

# 로그인 확인
echo -e "${YELLOW}🔐 Checking Cloudflare authentication...${NC}"
if ! wrangler whoami &> /dev/null; then
    echo -e "${YELLOW}Please login to Cloudflare:${NC}"
    wrangler login
fi

echo -e "${GREEN}✅ Authenticated with Cloudflare${NC}"

# KV Namespace 생성
echo -e "${YELLOW}📦 Creating KV namespace...${NC}"
KV_ID=$(wrangler kv:namespace create "TUNNEL_MAPPINGS" --env production | grep -o 'id = "[^"]*"' | cut -d'"' -f2)

if [ -n "$KV_ID" ]; then
    echo -e "${GREEN}✅ KV namespace created: $KV_ID${NC}"
    
    # wrangler.toml 업데이트
    sed -i.bak "s/YOUR_KV_NAMESPACE_ID/$KV_ID/g" wrangler.toml
    rm wrangler.toml.bak
    echo -e "${GREEN}✅ wrangler.toml updated${NC}"
else
    echo -e "${RED}❌ Failed to create KV namespace${NC}"
    exit 1
fi

# Secrets 설정
echo -e "${YELLOW}🔑 Setting up secrets...${NC}"
echo "Please enter your Cloudflare Zone ID (found in your domain's dashboard):"
read -r ZONE_ID
echo "Please enter your Cloudflare API Token (needs Zone:Edit permissions):"
read -rs API_TOKEN

echo "$ZONE_ID" | wrangler secret put CLOUDFLARE_ZONE_ID
echo "$API_TOKEN" | wrangler secret put CLOUDFLARE_API_TOKEN

echo -e "${GREEN}✅ Secrets configured${NC}"

# 배포
echo -e "${YELLOW}🚢 Deploying to Cloudflare Workers...${NC}"
wrangler deploy

echo -e "${GREEN}🎉 Setup completed!${NC}"
echo -e "${YELLOW}Your tunnel server is now running at: https://drcv-tunnel-server.YOUR_SUBDOMAIN.workers.dev${NC}"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Update your DRCV client's tunnel server URL"
echo "2. Make sure your domain's DNS is managed by Cloudflare"
echo "3. Test the tunnel functionality"