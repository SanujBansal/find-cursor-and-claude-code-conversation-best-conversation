-- Phase 7: add priority and example_conversation_id to learning_suggestions
ALTER TABLE learning_suggestions ADD COLUMN priority TEXT NOT NULL DEFAULT 'medium';
ALTER TABLE learning_suggestions ADD COLUMN example_conversation_id TEXT;
