#!/bin/bash

set -e

echo "🚢 Deploying DRCV Tunnel Server..."

# 색상 정의
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# 현재 디렉토리 확인
if [ ! -f "worker.js" ]; then
    echo -e "${RED}❌ worker.js not found. Run from tunnel-server directory${NC}"
    exit 1
fi

# Wrangler 설치 확인
if ! command -v wrangler &> /dev/null; then
    echo -e "${YELLOW}📦 Installing Wrangler...${NC}"
    npm install
fi

# 배포
echo -e "${YELLOW}🚀 Deploying to Cloudflare Workers...${NC}"
wrangler deploy

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✅ Deployment successful!${NC}"
    echo ""
    echo -e "${YELLOW}🌐 Your tunnel server is live${NC}"
    echo "Update your DRCV client to use this tunnel server URL"
else
    echo -e "${RED}❌ Deployment failed${NC}"
    exit 1
fi