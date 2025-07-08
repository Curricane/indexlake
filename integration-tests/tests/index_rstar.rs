use arrow::{
    array::BinaryArray,
    datatypes::{DataType, Field, Schema},
};
use indexlake::{
    LakeClient,
    catalog::Catalog,
    index::Index,
    storage::Storage,
    table::{TableConfig, TableCreation},
};
use indexlake_integration_tests::{catalog_sqlite, init_env_logger, storage_fs};
use std::sync::Arc;

use arrow::array::{Int32Array, RecordBatch};
use geo::{Geometry, Point};
use geozero::{CoordDimensions, ToWkb};
use indexlake::table::IndexCreation;
use indexlake_index_rstar::{RStarIndex, RStarIndexParams, WkbDialect};
use indexlake_integration_tests::data::create_namespace_if_not_exists;

#[rstest::rstest]
#[case(async { catalog_sqlite() }, storage_fs())]
// #[case(async { catalog_postgres().await }, storage_s3())]
#[tokio::test(flavor = "multi_thread")]
async fn create_rstar_index(
    #[future(awt)]
    #[case]
    catalog: Arc<dyn Catalog>,
    #[case] storage: Arc<Storage>,
) -> Result<(), Box<dyn std::error::Error>> {
    init_env_logger();

    let mut client = LakeClient::new(catalog, storage);
    client.register_index(Arc::new(RStarIndex))?;

    let namespace_name = "test_namespace";
    create_namespace_if_not_exists(&client, namespace_name).await?;

    let table_schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int32, false),
        Field::new("geom", DataType::Binary, false),
    ]));
    let table_config = TableConfig {
        inline_row_count_limit: 3,
        parquet_row_group_size: 2,
    };
    let table_name = "create_rstar_index";
    let table_creation = TableCreation {
        namespace_name: namespace_name.to_string(),
        table_name: table_name.to_string(),
        schema: table_schema.clone(),
        config: table_config,
    };
    client.create_table(table_creation).await?;
    let mut table = client.load_table(namespace_name, table_name).await?;

    let index_creation = IndexCreation {
        name: "rstar_index".to_string(),
        kind: RStarIndex.kind().to_string(),
        key_columns: vec!["geom".to_string()],
        include_columns: vec![],
        params: Arc::new(RStarIndexParams {
            wkb_dialect: WkbDialect::Wkb,
        }),
    };
    table.create_index(index_creation.clone()).await?;

    let record_batch = RecordBatch::try_new(
        table_schema.clone(),
        vec![
            Arc::new(Int32Array::from(vec![1, 2, 3, 4])),
            Arc::new(BinaryArray::from(vec![
                Geometry::from(Point::new(10.0, 10.0))
                    .to_wkb(CoordDimensions::xy())?
                    .as_slice(),
                Geometry::from(Point::new(11.0, 11.0))
                    .to_wkb(CoordDimensions::xy())?
                    .as_slice(),
                Geometry::from(Point::new(12.0, 12.0))
                    .to_wkb(CoordDimensions::xy())?
                    .as_slice(),
                Geometry::from(Point::new(13.0, 13.0))
                    .to_wkb(CoordDimensions::xy())?
                    .as_slice(),
            ])),
        ],
    )?;
    table.insert(&record_batch).await?;
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    Ok(())
}
