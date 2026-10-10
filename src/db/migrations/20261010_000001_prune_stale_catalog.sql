-- October 2026 catalog refresh: prune retired upstream model IDs and refresh
-- unmodified seeded smart model groups. Every statement below only touches
-- rows that still match the exact previously-seeded values, so operator
-- edits and live discovery rows are never modified.

-- 1. Remove retired stealth preview groups (only when still pristine).
DELETE FROM model_groups
WHERE name = 'preview:ox-alpha'
  AND target_json = '[{"model":"stealth/ox-alpha","provider":"openrouter"}]';

DELETE FROM model_groups
WHERE name = 'preview:union-alpha'
  AND target_json = '[{"model":"stealth/union-alpha","provider":"openrouter"}]';

-- 2. Refresh unmodified seeded groups to the October 2026 lineup.
UPDATE model_groups
SET target_json = '[{"model":"openai/gpt-oss-20b","provider":"groq"},{"model":"gemini-2.5-flash-lite","provider":"google"},{"model":"llama3.1-8b","provider":"cerebras"},{"model":"llama3.2","provider":"ollama"}]',
    updated_at = datetime('now')
WHERE name = 'fast:chat'
  AND target_json = '[{"model":"llama3-8b-8192","provider":"groq"},{"model":"llama3.1-8b","provider":"cerebras"},{"model":"gemini-2.0-flash","provider":"google"},{"model":"llama3.2","provider":"ollama"}]';

UPDATE model_groups
SET target_json = '[{"model":"cohere/north-mini-code:free","provider":"openrouter"},{"model":"deepseek-ai/DeepSeek-V3","provider":"siliconflow"},{"model":"qwen2.5-coder:7b","provider":"ollama"},{"model":"open-mistral-nemo","provider":"mistral"}]',
    updated_at = datetime('now')
WHERE name = 'cheap:code'
  AND target_json = '[{"model":"qwen2.5-coder:7b","provider":"ollama"},{"model":"open-mistral-nemo","provider":"mistral"},{"model":"deepseek-ai/DeepSeek-V3","provider":"siliconflow"}]';

UPDATE model_groups
SET target_json = '[{"model":"gpt-oss-120b","provider":"cerebras"},{"model":"openai/gpt-oss-120b","provider":"groq"},{"model":"deepseek-ai/DeepSeek-V3","provider":"siliconflow"},{"model":"deepseek-v4-pro","provider":"deepseek"},{"model":"grok-4.20-reasoning","provider":"xai"}]',
    updated_at = datetime('now')
WHERE name = 'reasoning:deep'
  AND target_json = '[{"model":"grok-4.20-reasoning","provider":"xai"},{"model":"deepseek-v4-pro","provider":"deepseek"},{"model":"deepseek-ai/DeepSeek-V3","provider":"siliconflow"}]';

UPDATE model_groups
SET target_json = '[{"model":"gemini-2.5-flash","provider":"google"},{"model":"google/gemma-3-27b-it:free","provider":"openrouter"},{"model":"gpt-4o-mini","provider":"github-models"}]',
    updated_at = datetime('now')
WHERE name = 'vision:balanced'
  AND target_json = '[{"model":"gemini-2.0-flash","provider":"google"},{"model":"gpt-4o-mini","provider":"github-models"}]';

-- 3. Remove retired curated model rows. The metadata guard keeps rows that
-- live discovery has since refreshed.
DELETE FROM models
WHERE metadata_json LIKE '%"source":"curated"%'
  AND (
    (provider_id = 'groq' AND upstream_model_id IN ('llama3-70b-8192', 'llama3-8b-8192', 'mixtral-8x7b-32768'))
    OR (provider_id = 'google' AND upstream_model_id IN ('gemini-1.5-flash', 'gemini-2.0-flash'))
    OR (provider_id = 'openrouter' AND upstream_model_id IN ('stealth/ox-alpha', 'stealth/union-alpha'))
  );
