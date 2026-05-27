-- Add cache_key column to scores for cache invalidation logic
ALTER TABLE scores ADD COLUMN cache_key TEXT NOT NULL DEFAULT '';
