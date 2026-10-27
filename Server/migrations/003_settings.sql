CREATE TABLE IF NOT EXISTS app_settings (
    `key`   VARCHAR(255) NOT NULL,
    `value` TEXT         NOT NULL,
    PRIMARY KEY (`key`)
);
-- Default: use 'published' as the frontmatter flag field name.
-- An empty value means "no filter — show all notes".
INSERT IGNORE INTO app_settings (`key`, `value`) VALUES ('publish_field', 'published');
