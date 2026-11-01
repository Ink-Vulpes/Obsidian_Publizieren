ALTER TABLE user_permissions ADD COLUMN can_propose BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE IF NOT EXISTS proposals (
    id INT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    note_slug VARCHAR(500) NOT NULL,
    proposed_by INT UNSIGNED NOT NULL,
    proposer_username VARCHAR(255) NOT NULL,
    proposed_content MEDIUMTEXT NOT NULL,
    message VARCHAR(500) DEFAULT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_created (created_at)
)
