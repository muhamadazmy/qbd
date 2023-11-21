-- kv table and main entrypoint of the schema
CREATE TABLE IF NOT EXISTS kv (
    key INTEGER PRIMARY KEY,
    value BLOB
);
