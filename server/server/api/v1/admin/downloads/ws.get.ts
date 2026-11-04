import aclManager from "~/server/internal/acls";
import { downloadRegistry } from "~/server/internal/downloads/registry";

const timers = new Map<string, ReturnType<typeof setInterval>>();
const openPeers = new Set<string>();

export default defineWebSocketHandler({
  async open(peer) {
    openPeers.add(peer.id);
    const request = { headers: peer.request?.headers ?? new Headers() };
    if (!(await aclManager.allowSystemACL(request, ["downloads:read"]))) {
      peer.send("unauthenticated");
      peer.close(1008, "Administrator access required");
      openPeers.delete(peer.id);
      return;
    }
    if (!openPeers.has(peer.id)) return;
    peer.send(JSON.stringify(downloadRegistry.snapshot()));
    const timer = setInterval(async () => {
      try {
        // Recheck revocation/expiry rather than keeping an admin socket forever.
        if (!(await aclManager.allowSystemACL(request, ["downloads:read"]))) {
          peer.close(1008, "Session expired");
          clearInterval(timer);
          timers.delete(peer.id);
          return;
        }
        peer.send(JSON.stringify(downloadRegistry.snapshot()));
      } catch {
        clearInterval(timer);
        timers.delete(peer.id);
        peer.close(1011, "Unable to refresh downloads");
      }
    }, 2000);
    timer.unref?.();
    timers.set(peer.id, timer);
  },
  close(peer) {
    openPeers.delete(peer.id);
    clearInterval(timers.get(peer.id));
    timers.delete(peer.id);
  },
  error(peer) {
    openPeers.delete(peer.id);
    clearInterval(timers.get(peer.id));
    timers.delete(peer.id);
  },
});
