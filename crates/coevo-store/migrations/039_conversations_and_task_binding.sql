CREATE TABLE IF NOT EXISTS conversation_threads (
    conversation_id TEXT PRIMARY KEY NOT NULL,
    opc_id TEXT NOT NULL DEFAULT '',
    user_id TEXT NOT NULL DEFAULT '',
    title TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','archived')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_conversation_threads_updated ON conversation_threads(updated_at_ms);

CREATE TABLE IF NOT EXISTS conversation_messages (
    message_id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user','assistant','system')),
    content TEXT NOT NULL DEFAULT '',
    linked_work_order_id TEXT,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(conversation_id) REFERENCES conversation_threads(conversation_id)
);
CREATE INDEX idx_conversation_messages_thread ON conversation_messages(conversation_id, created_at_ms);

ALTER TABLE work_orders ADD COLUMN conversation_id TEXT DEFAULT NULL;
CREATE INDEX idx_wo_conversation ON work_orders(conversation_id);
