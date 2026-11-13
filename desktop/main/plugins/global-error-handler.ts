import { invoke } from "@tauri-apps/api/core";

export default defineNuxtPlugin((nuxtApp) => {
  const recent = new Map<string, number>();
  let reports = 0;
  let windowStarted = Date.now();

  function report(source: string, error: unknown, context = "") {
    const message = error instanceof Error ? error.message : String(error);
    const details =
      error instanceof Error ? `${context}\n${error.stack ?? ""}` : context;
    const now = Date.now();
    if (now - windowStarted >= 60_000) {
      reports = 0;
      windowStarted = now;
      recent.clear();
    }
    const key = `${source}:${message}`;
    if (reports >= 20 || now - (recent.get(key) ?? 0) < 30_000) return;
    recent.set(key, now);
    reports++;
    // Never log/rethrow a reporting failure: that would create a report loop.
    void invoke("report_client_error", { source, message, details }).catch(
      () => {},
    );
  }

  nuxtApp.hook("vue:error", (error, _instance, info) =>
    report("vue", error, info),
  );
  const onError = (event: ErrorEvent) =>
    report("javascript", event.error ?? event.message);
  const onRejection = (event: PromiseRejectionEvent) =>
    report("unhandled-rejection", event.reason);
  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onRejection);
  if (import.meta.hot)
    import.meta.hot.dispose(() => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    });
});
