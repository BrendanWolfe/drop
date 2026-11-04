export type ProxyHeaderField = {
  name: string;
  value: string;
  savedName?: string;
};

export type ProxyAuthForm = {
  enabled: boolean;
  remember: boolean;
  headers: ProxyHeaderField[];
};

export type ProxyAuthStatus = {
  cleanupPending?: boolean;
  enabled: boolean;
  remember: boolean;
  headers: { name: string; hasValue: boolean }[];
};

export type ConnectionSettings = {
  url: string;
  proxyAuth: ProxyAuthStatus | null;
};

export function proxyAuthForm(status?: ProxyAuthStatus | null): ProxyAuthForm {
  return {
    enabled: status?.enabled ?? false,
    remember: status?.remember ?? true,
    headers: status?.headers.map((header) => ({
      name: header.name,
      value: "",
      savedName: header.hasValue ? header.name : undefined,
    })) ?? [
      { name: "P-Access-Token-Id", value: "" },
      { name: "P-Access-Token", value: "" },
    ],
  };
}

export function hasSavedValue(header: ProxyHeaderField): boolean {
  return header.savedName?.toLowerCase() === header.name.toLowerCase();
}

export function proxyAuthPayload(form: ProxyAuthForm) {
  return {
    enabled: form.enabled,
    remember: form.remember,
    headers: form.enabled ? form.headers.map((header) => ({
      name: header.name,
      // Null preserves an existing value. Never send a placeholder as a token.
      value: header.value === "" && hasSavedValue(header) ? null : header.value,
    })) : [],
  };
}
