// A slow request delays the next tick instead of accumulating work.
export function startPolling(refresh, onError, intervalMs = 3000) {
  let timer;
  let running = false;
  async function tick() {
    if (document.hidden || running) return;
    running = true;
    try {
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      running = false;
      schedule();
    }
  }
  function schedule() {
    clearTimeout(timer);
    if (!document.hidden && !running) timer = setTimeout(tick, intervalMs);
  }
  document.addEventListener('visibilitychange', () => {
    clearTimeout(timer);
    if (!document.hidden) tick();
  });
  schedule();
}
