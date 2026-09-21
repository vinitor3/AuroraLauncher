// Deterministic loopback-only catalog for visual QA. No account, API or personal files.
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { deflateSync } from 'node:zlib';

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const b of bytes) {
    crc ^= b;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const name = Buffer.from(type), length = Buffer.alloc(4), crc = Buffer.alloc(4);
  length.writeUInt32BE(data.length); crc.writeUInt32BE(crc32(Buffer.concat([name, data])));
  return Buffer.concat([length, name, data, crc]);
}
function skin(index) {
  const height = index === 24 ? 32 : 64;
  const pixels = Buffer.alloc(height * (64 * 4 + 1));
  const color = [(index * 79 + 40) % 220 + 20, (index * 43 + 80) % 220 + 20, (index * 137 + 10) % 220 + 20];
  function rect(x, y, w, h, rgb) {
    for (let yy = y; yy < Math.min(y + h, height); yy++) for (let xx = x; xx < x + w; xx++) {
      const offset = yy * 257 + 1 + xx * 4;
      pixels.set([...rgb, 255], offset);
    }
  }
  rect(0, 0, 32, 16, [215, 166, 122]);
  rect(0, 0, 32, 8, color); rect(8, 8, 8, 2, color);
  rect(10, 11, 1, 1, [20, 20, 20]); rect(13, 11, 1, 1, [20, 20, 20]);
  rect(16, 16, 24, 16, color); rect(40, 16, 16, 16, color);
  rect(0, 16, 16, 16, color.map(c => Math.floor(c / 2)));
  rect(16, 48, 16, 16, color.map(c => Math.floor(c / 2))); rect(32, 48, 16, 16, color);
  rect(23, 20, 2, 12, [240, 240, 240]);
  const header = Buffer.alloc(13); header.writeUInt32BE(64); header.writeUInt32BE(height, 4); header[8] = 8; header[9] = 6;
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]), chunk('IHDR', header), chunk('IDAT', deflateSync(pixels)), chunk('IEND', Buffer.alloc(0))]);
}
const fixtures = Array.from({ length: 25 }, (_, index) => {
  const png = skin(index);
  return { index, png, hash: createHash('sha256').update(png).digest('hex') };
});
const b64 = text => Buffer.from(text).toString('base64url');
const counts = new Map();
const server = createServer((request, response) => {
  const url = new URL(request.url, 'http://127.0.0.1');
  counts.set(url.pathname, (counts.get(url.pathname) || 0) + 1);
  if (url.pathname === '/counts') { response.end(JSON.stringify(Object.fromEntries(counts))); return; }
  const match = /^\/skin\/(\d+)\.png$/.exec(url.pathname);
  if (match && fixtures[Number(match[1])]) { response.setHeader('Content-Type', 'image/png'); response.end(fixtures[Number(match[1])].png); return; }
  const query = url.searchParams.get('query') || '';
  const page = Math.max(0, Number(url.searchParams.get('page')) || 0);
  const size = Math.max(1, Math.min(12, Number(url.searchParams.get('pageSize')) || 12));
  const source = query === 'one' ? fixtures.slice(0, 1) : query === 'none' ? [] : fixtures;
  const batch = source.slice(page * size, (page + 1) * size);
  response.setHeader('Content-Type', 'text/tab-separated-values');
  response.end('AURORA_SKINS_CATALOG_V1\npage\t' + page + '\tmore\t' + ((page + 1) * size < source.length ? 1 : 0) + '\n'
    + batch.map(item => ['fixture-' + item.index, b64('QA ' + (item.index + 1)), b64('Aurora QA'), item.index % 2 ? 'slim' : 'classic', item.hash, b64('/skin/' + item.index + '.png'), b64('fixture')].join('\t')).join('\n') + '\n');
});
server.listen(Number(process.env.AURORA_QA_PORT || 18741), '127.0.0.1', () => console.log('Aurora grid fixture: http://127.0.0.1:' + server.address().port + '/v1/skins/catalog'));
