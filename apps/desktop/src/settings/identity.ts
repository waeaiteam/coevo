const TENANT_ID_KEY = "coevo-tenant-id";
const OPC_ID_KEY = "coevo-opc-id";
const USER_ID_KEY = "coevo-user-id";
const OPC_NAME_KEY = "coevo-opc-name";
const USER_NAME_KEY = "coevo-user-name";
const LANGUAGE_KEY = "coevo-language";

export type LocalIdentity = {
  tenantId: string;
  opcId: string;
  userId: string;
  opcName: string;
  userName: string;
  language: "en" | "zh";
};

function uuidv4(): string {
  if (typeof crypto !== "undefined" && crypto.randomUUID) return crypto.randomUUID();
  return "10000000-1000-4000-8000-100000000000".replace(/[018]/g, (c) =>
    (Number(c) ^ (Math.random() * 16 >> Number(c) / 4)).toString(16)
  );
}

function read(key: string): string {
  try { return localStorage.getItem(key) || ""; } catch { return ""; }
}

function write(key: string, value: string) {
  try { localStorage.setItem(key, value); } catch { /* ignore */ }
}

function ensureUuid(key: string): string {
  const existing = read(key);
  if (existing) return existing;
  const next = uuidv4();
  write(key, next);
  return next;
}

export function getTenantId(): string {
  return ensureUuid(TENANT_ID_KEY);
}

export function getOpcId(): string {
  return ensureUuid(OPC_ID_KEY);
}

export function getUserId(): string {
  const existing = read(USER_ID_KEY);
  if (existing) return existing;
  write(USER_ID_KEY, "default-founder");
  return "default-founder";
}

export function getLocalIdentity(): LocalIdentity {
  return {
    tenantId: getTenantId(),
    opcId: getOpcId(),
    userId: getUserId(),
    opcName: read(OPC_NAME_KEY) || "My OPC",
    userName: read(USER_NAME_KEY) || "Founder",
    language: (read(LANGUAGE_KEY) === "zh" ? "zh" : "en"),
  };
}

export function createLocalOpc(input: { opcName: string; userName: string; language: "en" | "zh" }): LocalIdentity {
  write(OPC_NAME_KEY, input.opcName.trim() || "My OPC");
  write(USER_NAME_KEY, input.userName.trim() || "Founder");
  write(LANGUAGE_KEY, input.language);
  return getLocalIdentity();
}
