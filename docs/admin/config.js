// Production configuration loader for Admin Dashboard
// Loads from window.__AIGEN_CONFIG or ./config.json if present; falls back to empty config
(function () {
  async function fetchJson(url) {
    try {
      const res = await fetch(url, { cache: 'no-store' });
      if (!res.ok) return null;
      return await res.json();
    } catch { return null; }
  }
  const globalCfg = typeof window !== 'undefined' ? (window.__AIGEN_CONFIG || null) : null;
  const state = { loaded: false, config: {} };
  window.__aigenConfigState = state;
  window.loadAigenConfig = async function () {
    if (state.loaded) return state.config;
    const fileCfg = await fetchJson('config.json'); // optional
    const cfg = fileCfg || globalCfg || {};
    state.config = cfg;
    state.loaded = true;
    // expose short alias
    window.__aigenConfig = state.config;
    return state.config;
  };
})(); 
