-- Migration 037: Add session_id column to worker_events for session-scoped events
ALTER TABLE worker_events ADD COLUMN session_id TEXT NOT NULL DEFAULT '';
