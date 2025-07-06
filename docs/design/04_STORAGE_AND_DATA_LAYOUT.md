# IndexLake Design - 04: Storage and Data Layout

This document details the physical storage layer of IndexLake, including the abstraction over different storage backends and the hybrid data layout strategy that combines inline and external file storage.

## 1. Storage Abstraction

The `Storage` layer is responsible for all interactions with the physical file system, whether it's a local disk or a cloud object store.

### 1.1. `Storage` Enum and OpenDAL

- **Abstraction**: The `Storage` enum is the primary abstraction. It contains variants for each supported backend, such as `Fs(FsStorage)` and `S3(S3Storage)`.
- **Engine**: It uses the `opendal` library as its core engine. OpenDAL provides a unified API for accessing a wide variety of storage services, which makes the IndexLake storage layer extremely flexible and easy to extend to new backends.
- **Interface**: The `Storage` enum exposes a simple, high-level interface for file operations:
  - `create_file(path)`: Returns a writable `OutputFile`.
  - `open_file(path)`: Returns a readable `InputFile`.
  - `delete(path)`, `exists(path)`, `remove_dir_all(path)`: Standard file manipulation utilities.

### 1.2. `InputFile` and `OutputFile`

These structs are wrappers around OpenDAL's `Reader` and `Writer` types. They are designed to be passed to other parts of the system, like the Parquet reader/writer, and they implement the necessary `async_reader::AsyncFileReader` and `async_writer::AsyncFileWriter` traits for seamless integration.

## 2. Hybrid Data Layout Strategy

A key innovation in IndexLake is its two-tiered approach to storing row data. This strategy is designed to optimize for both low-latency queries on small or recent data and scalable, cost-effective storage for large historical data.

The physical location of every single row in the system is tracked by the **`indexlake_row_metadata_{table_id}`** table in the Catalog. The `location` column in this table is the key to the entire strategy.

### 2.1. Tier 1: Inline Storage

- **Mechanism**: When new data is inserted into a table via `table.insert()`, it is initially written directly into the **`indexlake_inline_row_{table_id}`** table within the Catalog's own database (e.g., PostgreSQL or SQLite).
- **`location` value**: For these rows, the `location` in the metadata table is set to the string `"inline"`.
- **Advantages**:
    - **Low Latency**: Reading this data is extremely fast, as it's a simple SQL `SELECT` from a local or network-attached database, avoiding the overhead of accessing an object store.
    - **Transactional Consistency**: Since the data is in the same database as the rest of the metadata, the `INSERT` operation is fully transactional with the metadata updates.
- **Use Case**: Ideal for OLTP-like workloads, small tables, or "write-and-read-immediately" scenarios.

### 2.2. Tier 2: External Parquet Files

- **Mechanism**: When the number of rows in the inline table for a given table exceeds a configurable threshold (`inline_row_count_limit`), a background `DumpTask` is triggered.
- **The Dump Process**:
    1. The `DumpTask` starts a new transaction in the Catalog.
    2. It reads a batch of rows from the `indexlake_inline_row_{table_id}` table.
    3. It writes these rows into a new **Apache Parquet file** in the `Storage` layer (e.g., on S3). Parquet is chosen for its high compression ratios and efficient columnar format, which is ideal for analytic queries.
    4. As the Parquet file is being written, the task keeps track of each row's new physical location.
    5. The `location` for each dumped row is updated in the `indexlake_row_metadata_{table_id}` table from `"inline"` to a structured path, e.g., `"parquet:namespace/table/file.parquet:0:123"`, which means: "in this parquet file, in the 0-th row group, at an offset of 123 rows".
    6. The original rows are then deleted from the `indexlake_inline_row_{table_id}` table.
    7. The transaction is committed.
- **Advantages**:
    - **Scalability & Cost**: Storing bulk data in Parquet files on an object store is highly scalable and cost-effective.
    - **Columnar Performance**: The columnar nature of Parquet means that queries only need to read the specific columns they require, drastically reducing I/O.

## 3. Querying the Hybrid Layout

When a `scan` operation is executed:

1.  The query planner first queries the `indexlake_inline_row_{table_id}` table to get results from the inline data.
2.  Simultaneously, it queries the `indexlake_row_metadata_{table_id}` table to find all rows whose `location` is *not* `"inline"`.
3.  It groups these external locations by their file path.
4.  For each Parquet file, it constructs a `RowSelection` filter based on the row group and offset information from the `location` strings.
5.  It then reads the Parquet files using these precise selections, ensuring only the required data is pulled from storage.
6.  Finally, the results from the inline scan and the external file scan are merged to produce the final result for the user.

This entire process is transparent to the end-user, who simply queries the table and receives a complete result set, regardless of where the data is physically stored.
