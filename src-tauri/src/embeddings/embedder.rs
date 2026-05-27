use crate::azure::{create_embeddings, AzureOpenAIConfig};

pub const DEFAULT_EMBEDDING_MODEL: &str = "text-embedding-3-small";

/// Embed a batch of texts using the configured Azure OpenAI deployment.
/// Processes in batches of up to 20; returns one Vec<f32> per input text.
pub async fn embed_texts(
    config: &AzureOpenAIConfig,
    texts: &[String],
    deployment: &str,
) -> Result<Vec<Vec<f32>>, String> {
    create_embeddings(config, deployment, texts).await
}

/// Serialize a float32 embedding to raw little-endian bytes for SQLite BLOB storage.
pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

/// Deserialize a SQLite BLOB back to a Vec<f32>.
pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}
