-- Replace v2 AI-output rubric (6 dimensions) with v3 hiring rubric (8 dimensions).
-- Existing scored rows are incomparable across rubrics and are deleted.
-- Aggregates derived from `scores` are also wiped and will be recomputed
-- by `refresh_analytics` on the next scoring run.

DELETE FROM daily_scores;
DELETE FROM weekly_scores;
DELETE FROM scores;

ALTER TABLE scores DROP COLUMN task_completion;
ALTER TABLE scores DROP COLUMN technical_correctness;
ALTER TABLE scores DROP COLUMN workflow_quality;
ALTER TABLE scores DROP COLUMN tool_use_and_context;
ALTER TABLE scores DROP COLUMN communication_clarity;
ALTER TABLE scores DROP COLUMN learning_leverage;

ALTER TABLE scores ADD COLUMN conceptual_knowledge   REAL NOT NULL DEFAULT 0 CHECK (conceptual_knowledge   >= 0 AND conceptual_knowledge   <= 5);
ALTER TABLE scores ADD COLUMN attention_to_detail    REAL NOT NULL DEFAULT 0 CHECK (attention_to_detail    >= 0 AND attention_to_detail    <= 5);
ALTER TABLE scores ADD COLUMN problem_decomposition  REAL NOT NULL DEFAULT 0 CHECK (problem_decomposition  >= 0 AND problem_decomposition  <= 5);
ALTER TABLE scores ADD COLUMN critical_evaluation    REAL NOT NULL DEFAULT 0 CHECK (critical_evaluation    >= 0 AND critical_evaluation    <= 5);
ALTER TABLE scores ADD COLUMN robustness_awareness   REAL NOT NULL DEFAULT 0 CHECK (robustness_awareness   >= 0 AND robustness_awareness   <= 5);
ALTER TABLE scores ADD COLUMN debugging_skill        REAL NOT NULL DEFAULT 0 CHECK (debugging_skill        >= 0 AND debugging_skill        <= 5);
ALTER TABLE scores ADD COLUMN prompt_specificity     REAL NOT NULL DEFAULT 0 CHECK (prompt_specificity     >= 0 AND prompt_specificity     <= 5);
ALTER TABLE scores ADD COLUMN scope_discipline       REAL NOT NULL DEFAULT 0 CHECK (scope_discipline       >= 0 AND scope_discipline       <= 5);
