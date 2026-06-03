import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { immer } from 'zustand/middleware/immer';

export interface Message {
  message_id: string;
  conversation_id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  created_at: string;
  linked_work_order_id?: string;
}

export interface Conversation {
  conversation_id: string;
  title: string;
  messages: Message[];
  created_at: string;
  updated_at: string;
}

interface ConversationState {
  conversations: Record<string, Conversation>;
  activeConversationId: string | null;

  addConversation: (conversation: Conversation) => void;
  updateConversation: (id: string, updates: Partial<Conversation>) => void;
  setActiveConversation: (id: string | null) => void;
  addMessage: (conversationId: string, message: Message) => void;
  updateMessage: (conversationId: string, messageId: string, updates: Partial<Message>) => void;
  deleteConversation: (id: string) => void;
  clearConversations: () => void;
}

export const useConversationStore = create<ConversationState>()(
  persist(
    immer((set) => ({
      conversations: {},
      activeConversationId: null,

      addConversation: (conversation: Conversation) =>
        set((state: ConversationState) => {
          state.conversations[conversation.conversation_id] = conversation;
        }),

      updateConversation: (id: string, updates: Partial<Conversation>) =>
        set((state: ConversationState) => {
          if (state.conversations[id]) {
            Object.assign(state.conversations[id], updates);
          }
        }),

      setActiveConversation: (id: string | null) =>
        set((state: ConversationState) => {
          state.activeConversationId = id;
        }),

      addMessage: (conversationId: string, message: Message) =>
        set((state: ConversationState) => {
          const conversation = state.conversations[conversationId];
          if (conversation) {
            conversation.messages.push(message);
            conversation.updated_at = new Date().toISOString();
          }
        }),

      updateMessage: (conversationId: string, messageId: string, updates: Partial<Message>) =>
        set((state: ConversationState) => {
          const conversation = state.conversations[conversationId];
          if (conversation) {
            const message = conversation.messages.find((m: Message) => m.message_id === messageId);
            if (message) {
              Object.assign(message, updates);
            }
          }
        }),

      deleteConversation: (id: string) =>
        set((state: ConversationState) => {
          delete state.conversations[id];
          if (state.activeConversationId === id) {
            state.activeConversationId = null;
          }
        }),

      clearConversations: () =>
        set((state: ConversationState) => {
          state.conversations = {};
          state.activeConversationId = null;
        }),
    })),
    {
      name: 'coevo-conversation-storage',
    }
  )
);
