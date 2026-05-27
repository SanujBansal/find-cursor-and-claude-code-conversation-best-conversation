-- Remove semantic search and learning suggestions storage
DROP TABLE IF EXISTS chunk_embeddings;
DROP TABLE IF EXISTS conversation_chunks;
DROP TABLE IF EXISTS learning_suggestions;
DELETE FROM jobs WHERE job_type IN ('embed', 'suggest');
