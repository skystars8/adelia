-- Adelia intentionally does not store addresses or address-derived identifiers.
ALTER TABLE posts DROP COLUMN IF EXISTS ip_hash;
ALTER TABLE reports DROP COLUMN IF EXISTS reporter_ip_hash;
DROP TABLE IF EXISTS bans;

-- With no reporter identifier, one open report per post prevents duplicate queue spam.
DELETE FROM reports AS duplicate
USING reports AS keeper
WHERE duplicate.status = 'open'
  AND keeper.status = 'open'
  AND duplicate.post_id = keeper.post_id
  AND duplicate.id > keeper.id;

CREATE UNIQUE INDEX IF NOT EXISTS reports_one_open_per_post_idx
    ON reports (post_id)
    WHERE status = 'open';
