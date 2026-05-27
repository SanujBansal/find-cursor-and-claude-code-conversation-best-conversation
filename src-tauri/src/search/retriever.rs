/// Cosine similarity between two equal-length float vectors.
/// Returns 0.0 if either vector has zero magnitude.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

pub struct ChunkRecord {
    pub conversation_db_id: i64,
    pub conversation_title: String,
    pub project_path: Option<String>,
    pub source_type: String,
    pub chunk_text: String,
    pub embedding: Vec<f32>,
}

pub struct RankedChunk {
    pub conversation_db_id: i64,
    pub conversation_title: String,
    pub project_path: Option<String>,
    pub source_type: String,
    pub chunk_text: String,
    pub similarity: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0_f32, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0_f32, 0.0, 0.0];
        let b = vec![0.0_f32, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_zero_vector() {
        let a = vec![0.0_f32, 0.0, 0.0];
        let b = vec![1.0_f32, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }
}

/// Pure in-memory cosine similarity ranking — sufficient for hundreds of transcripts.
pub fn rank_chunks(records: &[ChunkRecord], query_embedding: &[f32], limit: usize) -> Vec<RankedChunk> {
    let mut scored: Vec<(f32, usize)> = records
        .iter()
        .enumerate()
        .map(|(i, rec)| (cosine_similarity(query_embedding, &rec.embedding), i))
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    scored
        .into_iter()
        .map(|(sim, i)| {
            let rec = &records[i];
            RankedChunk {
                conversation_db_id: rec.conversation_db_id,
                conversation_title: rec.conversation_title.clone(),
                project_path: rec.project_path.clone(),
                source_type: rec.source_type.clone(),
                chunk_text: rec.chunk_text.clone(),
                similarity: sim,
            }
        })
        .collect()
}
