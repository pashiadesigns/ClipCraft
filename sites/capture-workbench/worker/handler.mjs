// The existing owner-private Sites access boundary protects every route, including
// the narrowly scoped release uploader. No credential is embedded in this Worker.
// Revisit write authorization before any explicitly requested audience change.
export function createHandler(assets, catalog) {
  const entries = new Map(catalog.map(asset => [asset.file, asset]));
  const response = (text, status = 200, headers = {}) => new Response(text, {
    status, headers: {'Cache-Control':'private, no-store', 'X-Content-Type-Options':'nosniff', ...headers}
  });
  const same = (object, entry) => object?.size === entry.bytes && object.customMetadata?.sha256 === entry.sha256;
  return {
    async fetch(request, env) {
      const url = new URL(request.url);
      const api = url.pathname.startsWith('/api/release-files/');
      const download = url.pathname.startsWith('/downloads/');
      const filename = url.pathname.slice(api ? '/api/release-files/'.length : '/downloads/'.length);
      const entry = (api || download) && entries.get(filename);
      if (url.pathname === '/api/release-storage' && request.method === 'GET') {
        return response(JSON.stringify({bucket:!!env.BUCKET,staticFallback:!!env.ASSETS,files:entries.size}), 200, {'Content-Type':'application/json'});
      }
      if (entry) {
        const key = `releases/${filename}`;
        if (api && request.method === 'PUT') {
          if (!env.BUCKET) return response('Storage unavailable',503);
          if (request.headers.get('Origin') && request.headers.get('Origin') !== url.origin) return response('Cross-origin write rejected',403);
          if (request.headers.get('Content-Type') !== 'application/zip' || Number(request.headers.get('Content-Length')) !== entry.bytes) return response('Exact release size and ZIP type required',400);
          if (entry.bytes > 24*1024*1024 || !request.body) return response('Invalid release body',413);
          const reader = request.body.getReader(), chunks = [];
          let length = 0;
          while (true) {
            const {done,value} = await reader.read();
            if (done) break;
            length += value.byteLength;
            if (length > entry.bytes) { await reader.cancel(); return response('Release too large',413); }
            chunks.push(value);
          }
          if (length !== entry.bytes) return response('Incomplete release',400);
          const body = new Uint8Array(length); let offset = 0;
          for (const chunk of chunks) { body.set(chunk,offset); offset += chunk.length; }
          const digest = [...new Uint8Array(await crypto.subtle.digest('SHA-256',body))].map(x=>x.toString(16).padStart(2,'0')).join('');
          if (digest !== entry.sha256) return response('Release checksum mismatch',422);
          const existing = await env.BUCKET.head(key);
          if (existing) return response(same(existing,entry) ? 'Already stored' : 'Immutable release conflict', same(existing,entry) ? 200 : 409);
          const stored = await env.BUCKET.put(key,body,{onlyIf:new Headers({'If-None-Match':'*'}),sha256:entry.sha256,customMetadata:{sha256:entry.sha256},httpMetadata:{contentType:'application/zip'}});
          if (!stored) {
            const concurrent = await env.BUCKET.head(key);
            return response(same(concurrent,entry) ? 'Already stored' : 'Immutable release conflict',same(concurrent,entry) ? 200 : 409);
          }
          return response('Stored',201);
        }
        if (!['GET','HEAD'].includes(request.method)) return response('Method not allowed',405,{'Allow': api ? 'GET, HEAD, PUT' : 'GET, HEAD'});
        const metadata = await env.BUCKET?.head(key);
        if (!metadata) {
          // During migration all historical files remain in the deployment.
          if (!api && env.ASSETS) return env.ASSETS.fetch(request);
          return response('Release not stored',404);
        }
        if (!same(metadata,entry)) return response('Release integrity conflict',409);
        const headers = {'Content-Type':'application/zip','Content-Disposition':`attachment; filename="${filename}"`,'Content-Length':String(entry.bytes),'ETag':`"${entry.sha256}"`,'X-ClipCraft-SHA256':entry.sha256,'X-ClipCraft-Storage':'r2','Accept-Ranges':'bytes'};
        if (request.headers.get('If-None-Match') === headers.ETag) return response(null,304,{'ETag':headers.ETag});
        if (request.method === 'HEAD') return response(null,200,headers);
        let range;
        const requestedRange = request.headers.get('Range');
        if (requestedRange && (!request.headers.has('If-Range') || request.headers.get('If-Range') === headers.ETag)) {
          const match = /^bytes=(\d*)-(\d*)$/.exec(requestedRange);
          if (!match || (!match[1] && !match[2])) return response(null,416,{'Content-Range':`bytes */${entry.bytes}`});
          const start = match[1] ? Number(match[1]) : Math.max(0,entry.bytes-Number(match[2]));
          const end = match[1] && match[2] ? Math.min(entry.bytes-1,Number(match[2])) : entry.bytes-1;
          if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start > end || start >= entry.bytes) return response(null,416,{'Content-Range':`bytes */${entry.bytes}`});
          range = {offset:start,length:end-start+1};
          headers['Content-Range'] = `bytes ${start}-${end}/${entry.bytes}`;
          headers['Content-Length'] = String(range.length);
        }
        const object = await env.BUCKET.get(key,range ? {range} : undefined);
        if (!object?.body) return response('Release temporarily unavailable',503);
        return response(object.body,range ? 206 : 200,headers);
      }
      if (api || url.pathname.startsWith('/api/')) return response('Not found',404);
      if (!['GET','HEAD'].includes(request.method)) return response('Method not allowed',405,{'Allow':'GET, HEAD'});
      const asset = assets[url.pathname === '/' ? '/index.html' : url.pathname];
      if (!asset) return response('Not found',404);
      let bytes = Uint8Array.from(atob(asset.base64),c=>c.charCodeAt(0));
      if (asset.encoding === 'gzip') bytes = new Uint8Array(await new Response(new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'))).arrayBuffer());
      return response(request.method === 'HEAD' ? null : bytes,200,{'Content-Type':asset.type,'Content-Length':String(bytes.length)});
    }
  };
}
