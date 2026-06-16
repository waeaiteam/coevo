const ACTIVE_CONVERSATION_KEY = "coevo-active-conversation-id";
const MISSION_STATE_PREFIX = "coevo-missionchat-state";
export const MISSION_SESSION_CLEARED_EVENT = "coevo:mission-session-cleared";

export function missionStateKey(opcId: string, userId: string) {
  return `${MISSION_STATE_PREFIX}:${opcId}:${userId}`;
}

export function conversationStorageKey(opcId: string, userId: string) {
  return `${ACTIVE_CONVERSATION_KEY}:${opcId}:${userId}`;
}

export function readActiveConversationId(opcId: string, userId: string): string {
  try {
    const scoped = localStorage.getItem(conversationStorageKey(opcId, userId));
    if (scoped) return scoped;
    return "";
  } catch {
    return "";
  }
}

export function writeActiveConversationId(opcId: string, userId: string, conversationId: string) {
  try {
    const scopedKey = conversationStorageKey(opcId, userId);
    if (conversationId.trim()) {
      localStorage.setItem(scopedKey, conversationId);
      localStorage.setItem(ACTIVE_CONVERSATION_KEY, conversationId);
    } else {
      localStorage.removeItem(scopedKey);
      localStorage.removeItem(ACTIVE_CONVERSATION_KEY);
    }
  } catch {
    // Ignore local persistence failures.
  }
}

export function clearMissionSession(opcId: string, userId: string) {
  try {
    localStorage.removeItem(missionStateKey(opcId, userId));
    localStorage.removeItem(conversationStorageKey(opcId, userId));
    localStorage.removeItem(ACTIVE_CONVERSATION_KEY);
    window.dispatchEvent(new CustomEvent(MISSION_SESSION_CLEARED_EVENT, {
      detail: { opcId, userId },
    }));
  } catch {
    // Ignore local persistence failures.
  }
}
