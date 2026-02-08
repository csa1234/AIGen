// Minimal test runner
(function () {
  const resultsEl = document.getElementById('results');
  const cases = [];
  function it(name, fn) { cases.push({ name, fn }); }
  function expect(val) {
    return {
      toBe: (exp) => { if (val !== exp) throw new Error(`Expected ${val} to be ${exp}`); },
      toEqual: (exp) => {
        const a = JSON.stringify(val); const b = JSON.stringify(exp);
        if (a !== b) throw new Error(`Expected ${a} to equal ${b}`);
      },
      toBeTruthy: () => { if (!val) throw new Error(`Expected truthy value`); },
      toBeFalsy: () => { if (val) throw new Error(`Expected falsy value`); }
    };
  }
  async function run() {
    for (const c of cases) {
      const el = document.createElement('div');
      el.className = 'case';
      try {
        const r = c.fn();
        if (r && typeof r.then === 'function') await r;
        el.classList.add('pass');
        el.textContent = `✓ ${c.name}`;
      } catch (e) {
        el.classList.add('fail');
        el.textContent = `✗ ${c.name} — ${e.message}`;
      }
      resultsEl.appendChild(el);
    }
  }
  window.__test = { it, expect, run };
})(); 
