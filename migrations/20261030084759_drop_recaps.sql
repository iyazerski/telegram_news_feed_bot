-- AI recaps were removed along with the recapper service, so drop their story history and settings.

DROP TABLE IF EXISTS sent_recap_story;

DELETE FROM app_setting
WHERE key IN ('recap_topics', 'recap_language', 'recap_interval_seconds', 'recap_enabled', 'recap_next_run_at');
