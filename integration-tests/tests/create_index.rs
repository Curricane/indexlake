use indexlake::{LakeClient, catalog::Catalog, index::Index, storage::Storage};
use indexlake_integration_tests::{
    catalog_postgres, catalog_sqlite, data::prepare_testing_table, init_env_logger, storage_fs,
    storage_s3,
};
use std::sync::Arc;

use indexlake::table::IndexCreation;
use indexlake_index_hash::{HashIndex, HashIndexParams};

#[rstest::rstest]
#[case(async { catalog_sqlite() }, storage_fs())]
#[case(async { catalog_postgres().await }, storage_s3())]
#[tokio::test(flavor = "multi_thread")]
async fn duplicated_index_name(
    #[future(awt)]
    #[case]
    catalog: Arc<dyn Catalog>,
    #[case] storage: Arc<Storage>,
) -> Result<(), Box<dyn std::error::Error>> {
    init_env_logger();

    let mut client = LakeClient::new(catalog, storage);
    client.register_index(Arc::new(HashIndex))?;

    let mut table = prepare_testing_table(&client, "duplicated_index_name").await?;

    let index_creation = IndexCreation {
        name: "test_index".to_string(),
        kind: HashIndex.kind().to_string(),
        key_columns: vec!["name".to_string()],
        include_columns: vec!["age".to_string()],
        params: Arc::new(HashIndexParams),
    };

    table.create_index(index_creation.clone()).await?;

    let result = table.create_index(index_creation).await;
    assert!(result.is_err());

    Ok(())
}

#[rstest::rstest]
#[case(async { catalog_sqlite() }, storage_fs())]
#[case(async { catalog_postgres().await }, storage_s3())]
#[tokio::test(flavor = "multi_thread")]
async fn unsupported_index_kind(
    #[future(awt)]
    #[case]
    catalog: Arc<dyn Catalog>,
    #[case] storage: Arc<Storage>,
) -> Result<(), Box<dyn std::error::Error>> {
    init_env_logger();

    let mut client = LakeClient::new(catalog, storage);
    client.register_index(Arc::new(HashIndex))?;

    let mut table = prepare_testing_table(&client, "unsupported_index_kind").await?;

    let index_creation = IndexCreation {
        name: "test_index".to_string(),
        kind: "unsupported_index_kind".to_string(),
        key_columns: vec!["name".to_string()],
        include_columns: vec!["age".to_string()],
        params: Arc::new(HashIndexParams),
    };

    let result = table.create_index(index_creation).await;
    assert!(result.is_err());

    Ok(())
}
