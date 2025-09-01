# DRCV Tunnel Server

Cloudflare Workers 기반 터널 서버로 P2P 직접 연결을 위한 서브도메인 할당 및 DNS 관리를 담당합니다.

## 구조

```
외부 사용자 → {hash}.drcv.app → 클라이언트 직접 연결
               ↑
            터널 서버 (DNS 관리만)
```

## 설정

### 1. 초기 설정 (한 번만)

```bash
cd tunnel-server
npm install -g wrangler
./setup.sh
```

설정 과정에서 입력이 필요한 항목:
- **Cloudflare Zone ID**: `drcv.app` 도메인의 Zone ID
- **API Token**: Zone:Edit 권한이 있는 Cloudflare API 토큰

### 2. 배포

```bash
./deploy.sh
```

## API 엔드포인트

### POST /register
클라이언트 등록 및 서브도메인 할당

**요청:**
```json
{
  "external_ip": "1.2.3.4",
  "port": 8080
}
```

**응답:**
```json
{
  "success": true,
  "subdomain": "abc123",
  "expires_in": 86400
}
```

### GET /health
서버 상태 확인

## 환경 변수

- `CLOUDFLARE_ZONE_ID`: DNS Zone ID
- `CLOUDFLARE_API_TOKEN`: API 토큰
- `TUNNEL_MAPPINGS`: KV Namespace (자동 생성)

## 비용

- Cloudflare Workers: $5/월 (유료 플랜)
- DNS API 사용: 무료
- KV Storage: 무료 (1GB까지)

## 제한사항

- 서브도메인 만료: 24시간
- DNS TTL: 60초 (빠른 전파)
- Workers CPU 시간: 50ms (유료 플랜에서 무제한)

## 보안

- IP 주소 형식 검증
- CORS 헤더 설정
- 입력값 검증
- 만료 시간 제한