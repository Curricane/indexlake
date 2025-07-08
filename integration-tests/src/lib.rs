pub mod data;
mod docker;
pub mod utils;

use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use indexlake::{catalog::Catalog, storage::Storage};
use indexlake_catalog_postgres::PostgresCatalog;
use indexlake_catalog_sqlite::SqliteCatalog;
use opendal::services::S3Config;
use uuid::Uuid;

use crate::docker::DockerCompose;

static ENV_LOGGER: OnceLock<()> = OnceLock::new();

pub fn init_env_logger() {
    // We don't care about the result, it's fine if it's already set.
    unsafe {
        let _ = std::env::set_var(
            "RUST_LOG",
            "info,indexlake=debug,indexlake_catalog_postgres=debug,indexlake_catalog_sqlite=debug,indexlake_index_rstar=debug",
        );
    }
    ENV_LOGGER.get_or_init(env_logger::init);
}

pub fn setup_sqlite_db() -> String {
    let db_path = format!(
        "{}/tmp/sqlite/{}.db",
        env!("CARGO_MANIFEST_DIR"),
        uuid::Uuid::new_v4().to_string()
    );
    std::fs::create_dir_all(PathBuf::from(&db_path).parent().unwrap()).unwrap();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(include_str!("../testdata/sqlite/init_catalog.sql"))
        .unwrap();
    db_path
}

pub struct PostgresTestContext {
    docker_compose: DockerCompose,
    pub catalog: Arc<dyn Catalog>,
}

impl PostgresTestContext {
    pub async fn new() -> Self {
        let project_name = format!("pg-{}", Uuid::new_v4().as_simple());
        let docker_compose = DockerCompose::new(
            &project_name,
            format!("{}/testdata/postgres", env!("CARGO_MANIFEST_DIR")),
        );

        docker_compose.up();
        // A short delay to ensure the service is fully ready.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let port = docker_compose.get_service_port("postgres", 5432);
        let catalog = Arc::new(
            PostgresCatalog::try_new("localhost", port, "postgres", "password", Some("postgres"))
                .await
                .unwrap(),
        );
        Self {
            docker_compose,
            catalog,
        }
    }
}

impl Drop for PostgresTestContext {
    fn drop(&mut self) {
        self.docker_compose.down();
    }
}

pub struct MinioTestContext {
    docker_compose: DockerCompose,
    pub storage: Arc<Storage>,
}

impl MinioTestContext {
    pub fn new() -> Self {
        let project_name = format!("minio-{}", Uuid::new_v4().as_simple());
        let docker_compose = DockerCompose::new(
            &project_name,
            format!("{}/testdata/minio", env!("CARGO_MANIFEST_DIR")),
        );
        docker_compose.up();
        // A short delay to ensure the service is fully ready.
        std::thread::sleep(std::time::Duration::from_secs(2));

        let port = docker_compose.get_service_port("minio", 9000);
        let mut config = S3Config::default();
        config.endpoint = Some(format!("http://127.0.0.1:{}", port));
        config.access_key_id = Some("admin".to_string());
        config.secret_access_key = Some("password".to_string());
        config.region = Some("us-east-1".to_string());
        config.disable_config_load = true;
        config.disable_ec2_metadata = true;
        let storage = Arc::new(Storage::new_s3(config, "indexlake"));
        Self {
            docker_compose,
            storage,
        }
    }
}

impl Drop for MinioTestContext {
    fn drop(&mut self) {
        self.docker_compose.down();
    }
}

pub fn catalog_sqlite() -> Arc<dyn Catalog> {
    let db_path = setup_sqlite_db();
    Arc::new(SqliteCatalog::try_new(db_path).unwrap())
}

pub async fn catalog_postgres() -> Arc<dyn Catalog> {
    let context = PostgresTestContext::new().await;
    context.catalog.clone()
}

pub fn storage_fs() -> Arc<Storage> {
    let home = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), "tmp/fs_storage");
    Arc::new(Storage::new_fs(home))
}

pub fn storage_s3() -> Arc<Storage> {
    let context = MinioTestContext::new();
    context.storage.clone()
}
