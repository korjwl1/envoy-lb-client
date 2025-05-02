// combined-server.js
'use strict'

const fastify = require('fastify')
const crypto = require('crypto')

// 일반 HTTP/1.1 및 h2c(HTTP/2 Cleartext) 서버
const http11Server = fastify({
    http2: false, // h2c 지원
    logger: false
})

// 타임스탬프 포맷 함수
function formatTimestamp() {
    const now = new Date()
    const hours = now.getHours().toString().padStart(2, '0')
    const minutes = now.getMinutes().toString().padStart(2, '0')
    const seconds = now.getSeconds().toString().padStart(2, '0')
    const milliseconds = now.getMilliseconds().toString().padStart(6, '0')
    
    return `[${hours}:${minutes}:${seconds}.${milliseconds}]`
}

// 랜덤 문자열 생성 함수 (300KB)
function generateRandomString(sizeInKB) {
  const sizeInBytes = sizeInKB * 1024; // KB를 바이트로 변환
  let result = '';
  while (result.length < sizeInBytes) {
      result += crypto.randomBytes(1024).toString('base64'); // 1KB씩 생성
  }
  return result.substring(0, sizeInBytes); // 정확한 크기만큼 반환
}

// HTTP 서버(HTTP/1.1 및 h2c)에 라우트 추가
http11Server.post('/', async (request, reply) => {
    const myId = request.headers['my_id'] || 'unknown'
    const protocol = request.headers[':scheme'] ? 'HTTP/2' : 'HTTP/1.1'
    console.log(`${formatTimestamp()} ${myId} arrived (${protocol})`)

    const response_body = generateRandomString(300);

    return { status: 'ok', data: response_body }
})

// 서버 시작
const start = async () => {
  try {
    await http11Server.listen({ port: 24420, host: '0.0.0.0' })
    console.log('HTTP server (HTTP/1.1) is running on port 24420')
  } catch (err) {
    console.error(err)
    process.exit(1)
  }
}

start()