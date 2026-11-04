import assert from "node:assert/strict";
import test from "node:test";
import { hasSavedValue, proxyAuthForm, proxyAuthPayload } from "../utils/proxy-auth.ts";

test("proxy authentication is off by default with Pangolin header names", () => {
  const form = proxyAuthForm();
  assert.equal(form.enabled, false);
  assert.deepEqual(form.headers.map((header) => header.name), ["P-Access-Token-Id", "P-Access-Token"]);
  assert.deepEqual(proxyAuthPayload(form).headers, []);
});

test("saved credentials remain backend-only and blank inputs preserve them", () => {
  const form = proxyAuthForm({ enabled: true, remember: true, headers: [{ name: "X-Token", hasValue: true }] });
  assert.equal(form.headers[0]!.value, "");
  assert.equal(proxyAuthPayload(form).headers[0]!.value, null);
  form.headers[0]!.value = "new-secret";
  assert.equal(proxyAuthPayload(form).headers[0]!.value, "new-secret");
});

test("renaming a header requires a new value and cannot reuse a different saved secret", () => {
  const form = proxyAuthForm({ enabled: true, remember: false, headers: [{ name: "X-Token", hasValue: true }] });
  form.headers[0]!.name = "x-token";
  assert.equal(hasSavedValue(form.headers[0]!), true);
  form.headers[0]!.name = "X-Different-Token";
  assert.equal(hasSavedValue(form.headers[0]!), false);
  assert.equal(proxyAuthPayload(form).headers[0]!.value, "");
});

test("disabling proxy auth sends no secrets and session-only choice is preserved", () => {
  const form = proxyAuthForm();
  form.headers[0]!.value = "secret";
  form.remember = false;
  assert.deepEqual(proxyAuthPayload(form), { enabled: false, remember: false, headers: [] });
});

test("deferred credential cleanup does not re-enable disabled authentication", () => {
  const form = proxyAuthForm({ enabled: false, remember: true, cleanupPending: true,
    headers: [{ name: "X-Token", hasValue: false }] });
  assert.equal(form.enabled, false);
  assert.deepEqual(proxyAuthPayload(form).headers, []);
  assert.equal(hasSavedValue(form.headers[0]!), false);
});
