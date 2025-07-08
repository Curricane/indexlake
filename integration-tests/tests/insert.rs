use arrow::array::{Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use indexlake::{
    LakeClient,
    catalog::Catalog,
    storage::Storage,
    table::{TableConfig, TableCreation},
};
use indexlake_integration_tests::utils::full_table_scan;
use indexlake_integration_tests::{
    catalog_postgres, catalog_sqlite, init_env_logger, storage_fs, storage_s3,
};
use std::sync::Arc;

#[rstest::rstest]
#[case(async { catalog_sqlite() }, storage_fs())]
#[case(async { catalog_postgres().await }, storage_s3())]
#[tokio::test(flavor = "multi_thread")]
async fn insert_table(
    #[future(awt)]
    #[case]
    catalog: Arc<dyn Catalog>,
    #[case] storage: Arc<Storage>,
) -> Result<(), Box<dyn std::error::Error>> {
    init_env_logger();

    let client = LakeClient::new(catalog, storage);

    let namespace_name = "test_namespace";
    client.create_namespace(namespace_name).await?;

    let table_schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]));
    let table_name = "test_table";
    let table_creation = TableCreation {
        namespace_name: namespace_name.to_string(),
        table_name: table_name.to_string(),
        schema: table_schema.clone(),
        config: TableConfig::default(),
    };
    client.create_table(table_creation).await?;

    let table = client.load_table(namespace_name, table_name).await?;

    let record_batch = RecordBatch::try_new(
        table_schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 2])),
            Arc::new(StringArray::from(vec!["Alice", "Bob"])),
        ],
    )
    .unwrap();

    table.insert(&record_batch).await?;

    let table_str = full_table_scan(&table).await?;
    println!("{}", table_str);
    assert_eq!(
        table_str,
        r#"+-------------------+----+-------+
| _indexlake_row_id | id | name  |
+-------------------+----+-------+
| 1                 | 1  | Alice |
| 2                 | 2  | Bob   |
+-------------------+----+-------+"#,
    );

    Ok(())
}
