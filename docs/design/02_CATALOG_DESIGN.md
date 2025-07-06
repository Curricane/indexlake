# IndexLake Design - 02: Catalog Service

The Catalog is the central nervous system of IndexLake. It is responsible for storing, managing, and serving all metadata required for the system to operate. This document details its design, schema, and transactional model.

## 1. Design Principles

- **Transactional Integrity**: All metadata changes must be atomic. Creating a table, for instance, involves inserting into multiple metadata tables; this entire operation must succeed or fail as a single unit.
- **Backend Agnostic**: The core system logic is completely decoupled from the specific database used for the catalog. This is achieved via the `Catalog` and `Transaction` traits.
- **Centralized SQL Logic**: All SQL statements are managed within the `catalog::helper` module, creating a dedicated Data Access Layer (DAL). This prevents SQL logic from leaking into other parts of the codebase and simplifies maintenance.
- **Resource Safety**: Transactions are managed using the RAII pattern. The `Transaction` trait requires implementors to also implement `Drop`, ensuring that any transaction not explicitly committed is automatically rolled back, even in the event of a `panic`.

## 2. Core Abstractions

### 2.1. `Catalog` Trait

This is the primary interface for interacting with the metadata store.

```rust
pub trait Catalog: Debug + Send + Sync {
    // Identifies the database backend (e.g., Postgres, Sqlite).
    fn database(&self) -> CatalogDatabase;

    // Performs a single, non-transactional query.
    async fn query(&self, sql: &str, schema: CatalogSchemaRef) -> ILResult<RowStream<'static>>;

    // Begins a new transaction, returning a transactional handle.
    async fn transaction(&self) -> ILResult<Box<dyn Transaction>>;
}
```

### 2.2. `Transaction` Trait

This trait encapsulates all operations that can be performed within a single atomic transaction.

```rust
pub trait Transaction: Debug + Send {
    // Methods for querying and executing statements within the transaction.
    async fn query<'a>(...) -> ILResult<RowStream<'a>>;
    async fn execute(&mut self, sql: &str) -> ILResult<usize>;
    async fn execute_batch(&mut self, sqls: &[String]) -> ILResult<()>;

    // Finalizing the transaction.
    async fn commit(&mut self) -> ILResult<()>;
    async fn rollback(&mut self) -> ILResult<()>;
}
```
Crucially, implementors like `PostgresTransaction` and `SqliteTransaction` have a `Drop` implementation that calls `rollback()` if the transaction was not explicitly committed.

## 3. Metadata Schema

The catalog's state is maintained across a set of relational tables.

- **`indexlake_namespace`**: Stores namespace information.
  - `namespace_id` (BIGINT, PK): Unique identifier.
  - `namespace_name` (VARCHAR): User-defined name for the namespace.

- **`indexlake_table`**: Stores information about each table.
  - `table_id` (BIGINT, PK): Unique identifier.
  - `table_name` (VARCHAR): User-defined name of the table.
  - `namespace_id` (BIGINT): Foreign key to `indexlake_namespace`.
  - `config` (VARCHAR/TEXT): JSON string containing table-specific configurations (`TableConfig`).

- **`indexlake_field`**: Stores the schema for each table.
  - `field_id` (BIGINT, PK): Unique identifier for the column.
  - `table_id` (BIGINT): Foreign key to `indexlake_table`.
  - `field_name` (VARCHAR): Name of the column.
  - `data_type` (VARCHAR): Arrow data type as a string.
  - `nullable` (BOOLEAN): Whether the column can contain nulls.
  - `metadata` (VARCHAR/TEXT): JSON string for Arrow field-level metadata.

- **`indexlake_index`**: Stores definitions of secondary indices.
  - `index_id` (BIGINT, PK): Unique identifier for the index.
  - `index_name` (VARCHAR): User-defined name of the index.
  - `index_kind` (VARCHAR): The type of index (e.g., "rstar", "hash").
  - `table_id` (BIGINT): Foreign key to `indexlake_table`.
  - `key_field_ids` (VARCHAR): Comma-separated list of `field_id`s that form the index key.
  - `include_field_ids` (VARCHAR): Comma-separated list of `field_id`s for columns to include in the index.
  - `params` (VARCHAR/TEXT): JSON string containing index-specific parameters.

- **`indexlake_data_file`**: Tracks data files stored externally (e.g., in S3).
  - `data_file_id` (BIGINT, PK): Unique identifier.
  - `table_id` (BIGINT): Foreign key to `indexlake_table`.
  - `relative_path` (VARCHAR): Path to the file in the `Storage` layer.
  - `file_size_bytes` (BIGINT): Size of the file.
  - `record_count` (BIGINT): Number of rows in the file.
  - `row_ids` (BLOB/BYTEA): A packed binary representation of the `_indexlake_row_id`s contained in this file.

- **`indexlake_index_file`**: Links index artifacts to data files.
  - `index_file_id` (BIGINT, PK): Unique identifier.
  - `index_id` (BIGINT): Foreign key to `indexlake_index`.
  - `data_file_id` (BIGINT): Foreign key to `indexlake_data_file`.
  - `relative_path` (VARCHAR): Path to the index file in the `Storage` layer.

### 3.1. Per-Table Dynamic Tables

For each user-created table, two additional tables are dynamically created within the catalog database itself. This is a key design choice for the hybrid storage model.

- **`indexlake_row_metadata_{table_id}`**: The "address book" for the table.
  - `_indexlake_row_id` (BIGINT, PK): The globally unique, monotonically increasing ID for a row.
  - `location` (VARCHAR): The physical location of the row's data. Can be `"inline"` or a path like `"parquet:path/to/file.parquet:row_group:offset"`.
  - `deleted` (BOOLEAN): A soft-delete flag.

- **`indexlake_inline_row_{table_id}`**: Stores actual row data for "inline" rows.
  - `_indexlake_row_id` (BIGINT, PK): Foreign key to the metadata table.
  - `...user_columns`: The remaining columns mirror the user-defined table schema.

This dynamic table creation allows the catalog to directly store and serve data for small or recently-inserted rows, providing very low query latency.
