/// Effort weight for a conversation based on message and tool-call volume.
/// Clamps to [1.0, 3.0] so short chats and marathon sessions both contribute.
pub fn effort_weight(message_count: i64, tool_call_count: i64) -> f64 {
    let x = (1 + message_count + tool_call_count) as f64;
    let w = 1.0 + x.log10();
    w.clamp(1.0, 3.0)
}

/// Effort-weighted average of (score, weight) pairs.
/// Returns 0.0 when the input is empty.
pub fn weighted_average(scores_and_weights: &[(f64, f64)]) -> f64 {
    let sum_wt: f64 = scores_and_weights.iter().map(|(_, w)| w).sum();
    if sum_wt == 0.0 {
        return 0.0;
    }
    scores_and_weights
        .iter()
        .map(|(s, w)| s * w)
        .sum::<f64>()
        / sum_wt
}

/// Weekly score = simple mean of the daily scores for each active day.
/// Returns 0.0 when there are no active days.
pub fn compute_weekly_score(daily_scores: &[f64]) -> f64 {
    if daily_scores.is_empty() {
        return 0.0;
    }
    daily_scores.iter().sum::<f64>() / daily_scores.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_weight_clamps_at_minimum() {
        let w = effort_weight(0, 0);
        assert!((w - 1.0).abs() < 0.001, "min effort weight should be 1.0");
    }

    #[test]
    fn effort_weight_clamps_at_maximum() {
        let w = effort_weight(10000, 10000);
        assert!(w <= 3.0 + 0.001, "max effort weight should be 3.0");
    }

    #[test]
    fn weighted_average_uniform_weights() {
        let pairs = vec![(4.0_f64, 1.0_f64), (2.0_f64, 1.0_f64)];
        let avg = weighted_average(&pairs);
        assert!((avg - 3.0).abs() < 0.001, "uniform weighted avg of 4 and 2 should be 3.0");
    }

    #[test]
    fn weekly_score_is_average_of_daily() {
        let days = vec![4.0_f64, 2.0_f64, 3.0_f64];
        let w = compute_weekly_score(&days);
        assert!((w - 3.0).abs() < 0.001);
    }
}
