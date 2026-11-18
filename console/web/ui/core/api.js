export function createApi({headers, onUnauthorized}) {
  const pendingReads = new Map();

  async function request(path, options) {
    const response = await fetch('/api' + path, {
      ...options,
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/json',
        ...options.headers,
        ...headers(),
      },
    });
    if (response.status === 401) {
      onUnauthorized();
      throw new Error('登录已过期，请重新登录');
    }
    const body = await response.text();
    let data;
    try {
      data = JSON.parse(body);
    } catch {
      throw new Error(body || '服务器未返回有效响应');
    }
    if (!response.ok) throw new Error(data.error || JSON.stringify(data));
    return data;
  }

  return function api(path, { dedupe = true, ...options } = {}) {
    const method = (options.method || 'GET').toUpperCase();
    if (method !== 'GET') {
      // Reads started after a mutation need a fresh server response.
      pendingReads.clear();
      return request(path, options);
    }
    if (!dedupe || options.signal || options.headers) return request(path, options);
    if (!pendingReads.has(path)) {
      const read = request(path, options).finally(() => {
        if (pendingReads.get(path) === read) pendingReads.delete(path);
      });
      pendingReads.set(path, read);
    }
    return pendingReads.get(path);
  };
}
