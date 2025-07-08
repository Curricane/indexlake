use arrow::datatypes::{DataType, Field, Schema};
use indexlake::catalog::INTERNAL_ROW_ID_FIELD_REF;
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

use arrow::array::{
    BinaryArray, BooleanArray, Float32Array, Float64Array, Int16Array, Int32Array, Int64Array,
    RecordBatch, StringArray,
};

#[rstest::rstest]
#[case(async { catalog_sqlite() }, storage_fs())]
#[case(async { catalog_postgres().await }, storage_s3())]
#[tokio::test(flavor = "multi_thread")]
async fn create_table(
    #[future(awt)]
    #[case]
    catalog: Arc<dyn Catalog>,
    #[case] storage: Arc<Storage>,
) -> Result<(), Box<dyn std::error::Error>> {
    init_env_logger();

    let client = LakeClient::new(catalog, storage);

    let namespace_name = "test_namespace";
    let expected_namespace_id = client.create_namespace(namespace_name).await?;

    let expected_schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]));

    let table_name = "test_table";
    let table_creation = TableCreation {
        namespace_name: namespace_name.to_string(),
        table_name: table_name.to_string(),
        schema: expected_schema.clone(),
        config: TableConfig::default(),
    };

    let expected_table_id = client.create_table(table_creation).await?;

    let table = client.load_table(namespace_name, table_name).await?;
    println!("table: {:?}", table);
    assert_eq!(table.namespace_id, expected_namespace_id);
    assert_eq!(table.namespace_name, namespace_name);
    assert_eq!(table.table_id, expected_table_id);
    assert_eq!(table.table_name, table_name);

    let mut fields = vec![INTERNAL_ROW_ID_FIELD_REF.clone()];
    fields.extend(expected_schema.fields.iter().map(|f| f.clone()));
    let expected_schema = Schema::new(fields);

    assert_eq!(table.schema.as_ref(), &expected_schema);

    Ok(())
}

#[rstest::rstest]
#[case(async { catalog_sqlite() }, storage_fs())]
#[case(async { catalog_postgres().await }, storage_s3())]
#[tokio::test(flavor = "multi_thread")]
async fn table_data_types(
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
        Field::new("boolean_col", DataType::Boolean, true),
        Field::new("int16_col", DataType::Int16, true),
        Field::new("int32_col", DataType::Int32, true),
        Field::new("int64_col", DataType::Int64, true),
        Field::new("float32_col", DataType::Float32, true),
        Field::new("float64_col", DataType::Float64, true),
        Field::new("utf8_col", DataType::Utf8, true),
        Field::new("binary_col", DataType::Binary, true),
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
            Arc::new(BooleanArray::from(vec![true])),
            Arc::new(Int16Array::from(vec![1])),
            Arc::new(Int32Array::from(vec![2])),
            Arc::new(Int64Array::from(vec![3])),
            Arc::new(Float32Array::from(vec![1.1])),
            Arc::new(Float64Array::from(vec![2.2])),
            Arc::new(StringArray::from(vec!["utf8"])),
            Arc::new(BinaryArray::from_vec(vec![&vec![0u8, 1u8]])),
        ],
    )?;
    table.insert(&record_batch).await?;

    let table_str = full_table_scan(&table).await?;
    println!("{}", table_str);
    assert_eq!(
        table_str,
        r#"+-------------------+-------------+-----------+-----------+-----------+-------------+-------------+----------+------------+
| _indexlake_row_id | boolean_col | int16_col | int32_col | int64_col | float32_col | float64_col | utf8_col | binary_col |
+-------------------+-------------+-----------+-----------+-----------+-------------+-------------+----------+------------+
| 1                 | true        | 1         | 2         | 3         | 1.1         | 2.2         | utf8     | 0001       |
+-------------------+-------------+-----------+-----------+-----------+-------------+-------------+----------+------------+"#,
    );

    Ok(())
}

#[rstest::rstest]
#[case(async { catalog_sqlite() }, storage_fs())]
#[case(async { catalog_postgres().await }, storage_s3())]
#[tokio::test(flavor = "multi_thread")]
async fn duplicated_table_name(
    #[future(awt)]
    #[case]
    catalog: Arc<dyn Catalog>,
    #[case] storage: Arc<Storage>,
) -> Result<(), Box<dyn std::error::Error>> {
    init_env_logger();

    let client = LakeClient::new(catalog, storage);

    let namespace_name = "test_namespace";
    client.create_namespace(namespace_name).await?;

    let expected_schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]));

    let table_name = "test_table";
    let table_creation = TableCreation {
        namespace_name: namespace_name.to_string(),
        table_name: table_name.to_string(),
        schema: expected_schema.clone(),
        config: TableConfig::default(),
    };

    client.create_table(table_creation.clone()).await?;
    let result = client.create_table(table_creation).await;
    assert!(result.is_err());

    Ok(())
}
