/**
 * DRCV Tunnel Server - Cloudflare Workers
 * Handles subdomain registration and DNS management for P2P tunneling
 */

export default {
  async fetch(request, env, ctx) {
    // CORS 헤더
    const corsHeaders = {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type',
    };

    // OPTIONS 요청 처리 (CORS preflight)
    if (request.method === 'OPTIONS') {
      return new Response(null, { headers: corsHeaders });
    }

    try {
      const url = new URL(request.url);
      
      if (request.method === 'POST' && url.pathname === '/register') {
        return await handleRegister(request, env, corsHeaders);
      }
      
      if (request.method === 'GET' && url.pathname === '/health') {
        return new Response('OK', { headers: corsHeaders });
      }
      
      return new Response('Not Found', { 
        status: 404, 
        headers: corsHeaders 
      });
      
    } catch (error) {
      console.error('Error:', error);
      return new Response(JSON.stringify({
        success: false,
        message: 'Internal server error'
      }), {
        status: 500,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' }
      });
    }
  }
};

async function handleRegister(request, env, corsHeaders) {
  try {
    const { port } = await request.json();
    
    // 포트 검증
    if (!port || port < 1 || port > 65535) {
      return new Response(JSON.stringify({
        success: false,
        message: 'Invalid port number'
      }), {
        status: 400,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' }
      });
    }
    
    // 클라이언트의 실제 외부 IP 감지 (CF-Connecting-IP 헤더 사용)
    const external_ip = request.headers.get('CF-Connecting-IP') || 
                       request.headers.get('X-Forwarded-For') || 
                       request.headers.get('X-Real-IP') || 
                       'unknown';
    
    // IP 주소 형식 검증
    if (!isValidIP(external_ip)) {
      return new Response(JSON.stringify({
        success: false,
        message: `Unable to determine valid external IP: ${external_ip}`
      }), {
        status: 400,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' }
      });
    }
    
    console.log(`Client external IP detected: ${external_ip}:${port}`);
    
    // 같은 IP에서 같은 포트로 이미 등록된 터널이 있는지 확인
    const existingKey = `${external_ip}:${port}`;
    const existing = await env.TUNNEL_MAPPINGS.get(existingKey);
    
    let subdomain;
    if (existing) {
      // 기존 터널이 있으면 재사용
      const existingData = JSON.parse(existing);
      subdomain = existingData.subdomain;
      console.log(`Reusing existing tunnel: ${subdomain}.drcv.app for ${existingKey}`);
    } else {
      // 새로운 서브도메인 생성
      subdomain = generateSubdomain();
      console.log(`Creating new tunnel: ${subdomain}.drcv.app for ${existingKey}`);
    }
    
    // DNS 레코드 업데이트
    const dnsSuccess = await updateDNSRecord(env, subdomain, external_ip);
    
    if (!dnsSuccess) {
      return new Response(JSON.stringify({
        success: false,
        message: 'Failed to create DNS record'
      }), {
        status: 500,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' }
      });
    }
    
    // KV에 매핑 정보 저장 (만료 시간: 24시간)
    const tunnelData = {
      external_ip,
      port,
      subdomain,
      created_at: Date.now()
    };
    
    // 서브도메인 -> 터널 정보 매핑
    await env.TUNNEL_MAPPINGS.put(subdomain, JSON.stringify(tunnelData), { expirationTtl: 86400 });
    
    // IP:포트 -> 서브도메인 매핑 (중복 방지용)
    await env.TUNNEL_MAPPINGS.put(`${external_ip}:${port}`, JSON.stringify(tunnelData), { expirationTtl: 86400 });
    
    console.log(`Registered tunnel: ${subdomain}.drcv.app -> ${external_ip}:${port}`);
    
    return new Response(JSON.stringify({
      success: true,
      subdomain: subdomain,
      external_ip: external_ip, // 감지된 IP 반환
      expires_in: 86400
    }), {
      headers: { ...corsHeaders, 'Content-Type': 'application/json' }
    });
    
  } catch (error) {
    console.error('Registration error:', error);
    return new Response(JSON.stringify({
      success: false,
      message: 'Registration failed'
    }), {
      status: 500,
      headers: { ...corsHeaders, 'Content-Type': 'application/json' }
    });
  }
}

async function updateDNSRecord(env, subdomain, ip) {
  const zoneId = env.CLOUDFLARE_ZONE_ID;
  const apiToken = env.CLOUDFLARE_API_TOKEN;
  
  if (!zoneId || !apiToken) {
    console.error('Missing Cloudflare credentials');
    return false;
  }
  
  try {
    // DNS A 레코드 생성
    const response = await fetch(`https://api.cloudflare.com/client/v4/zones/${zoneId}/dns_records`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiToken}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        type: 'A',
        name: `${subdomain}.drcv.app`,
        content: ip,
        ttl: 60, // 1분 TTL (빠른 전파)
        comment: `DRCV tunnel - created at ${new Date().toISOString()}`
      })
    });
    
    const result = await response.json();
    
    if (result.success) {
      console.log(`DNS record created: ${subdomain}.drcv.app -> ${ip}`);
      return true;
    } else {
      console.error('DNS creation failed:', result.errors);
      return false;
    }
    
  } catch (error) {
    console.error('DNS API error:', error);
    return false;
  }
}

function generateSubdomain() {
  // 6자리 랜덤 해시 생성 (소문자 + 숫자)
  const chars = 'abcdefghijklmnopqrstuvwxyz0123456789';
  let result = '';
  for (let i = 0; i < 6; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
}

function isValidIP(ip) {
  const ipRegex = /^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$/;
  return ipRegex.test(ip);
}