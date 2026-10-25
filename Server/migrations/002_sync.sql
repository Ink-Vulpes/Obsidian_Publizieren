-- Track every file stored in the vault (checksum, size, who uploaded it, from which device)
CREATE TABLE IF NOT EXISTS vault_files (
    id          INT UNSIGNED  AUTO_INCREMENT PRIMARY KEY,
    path        VARCHAR(1000) NOT NULL,
    checksum    VARCHAR(64)   NOT NULL,
    size        BIGINT        NOT NULL DEFAULT 0,
    modified_at DATETIME      NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    uploaded_by INT UNSIGNED,
    device_id   VARCHAR(128),
    UNIQUE KEY uq_vault_path (path(768)),
    FOREIGN KEY (uploaded_by) REFERENCES users(id) ON DELETE SET NULL
);

-- Register and track sync devices (PC, phone, …)
CREATE TABLE IF NOT EXISTS sync_devices (
    id          INT UNSIGNED  AUTO_INCREMENT PRIMARY KEY,
    device_id   VARCHAR(128)  NOT NULL,
    device_name VARCHAR(255)  NOT NULL DEFAULT '',
    user_id     INT UNSIGNED,
    last_seen   DATETIME      NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    created_at  DATETIME      NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY uq_device_id (device_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
);
