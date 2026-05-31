use sqlx::postgres::PgPoolOptions;
use sqlx::{FromRow, PgPool, Row};

pub struct DbPool {
    pool: PgPool,
}

#[derive(Debug, FromRow)]
pub struct ApiKeyRow {
    pub id: uuid::Uuid,
    pub key_hash: String,
    pub key_prefix: String,
    pub name: Option<String>,
    pub tpm_limit: Option<i32>,
    pub rpm_limit: Option<i32>,
    pub total_spend: f64,
    pub is_active: bool,
}

impl DbPool {
    pub async fn new(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn find_key_by_hash(&self, hash: &str) -> Option<ApiKeyRow> {
        sqlx::query_as::<_, ApiKeyRow>(
            "SELECT id, key_hash, key_prefix, name, tpm_limit, rpm_limit, total_spend, is_active \
             FROM api_keys WHERE key_hash = $1 AND is_active = true"
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
    }

    pub async fn insert_key(&self, hash: &str, prefix: &str) -> Result<uuid::Uuid, sqlx::Error> {
        let row = sqlx::query(
            "INSERT INTO api_keys (key_hash, key_prefix) VALUES ($1, $2) RETURNING id"
        )
        .bind(hash)
        .bind(prefix)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("id"))
    }

    pub async fn list_keys(&self) -> Result<Vec<ApiKeyRow>, sqlx::Error> {
        sqlx::query_as::<_, ApiKeyRow>(
            "SELECT id, key_hash, key_prefix, name, tpm_limit, rpm_limit, total_spend, is_active \
             FROM api_keys WHERE is_active = true ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn deactivate_key(&self, id: uuid::Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE api_keys SET is_active = false WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
