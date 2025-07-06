# IndexLake Design - 05: Query and DML Execution

This document details the logical flow of how Data Manipulation Language (DML) operations like `INSERT`, `UPDATE`, `DELETE`, and `SCAN` are executed within the IndexLake system. It illustrates how the different architectural layers (Client, Catalog, Storage, Index) collaborate to fulfill a user request.

## 1. `INSERT` Operation

The `INSERT` flow is optimized for low-latency writes by initially placing all new data into the "inline" storage tier.

**Flow:**
1.  **API Call**: `table.insert(record_batch)` is called.
2.  **Transaction Start**: A new `TransactionHelper` is created, starting a transaction with the Catalog service.
3.  **Row ID Generation**: The system queries the catalog for the current maximum `_indexlake_row_id` for the table and generates a new, contiguous block of IDs for the rows in the `record_batch`.
4.  **Metadata Preparation**: A `RowMetadataRecord` is created for each new row. The `location` for each is set to `"inline"`, and `deleted` is `false`.
5.  **Write to Inline Storage**: The user's `record_batch` is augmented with the new `_indexlake_row_id` column. The entire batch is then converted into a multi-row `INSERT` SQL statement and executed against the `indexlake_inline_row_{table_id}` table via the `TransactionHelper`.
6.  **Write to Metadata**: The prepared `RowMetadataRecord`s are inserted into the `indexlake_row_metadata_{table_id}` table.
7.  **Transaction Commit**: The `TransactionHelper` is committed. All changes to the inline data and metadata tables are made permanent atomically.
8.  **Dump Task Trigger**: After the commit, the system checks the total number of rows in the inline table. If it exceeds `inline_row_count_limit`, a background `DumpTask` is spawned to asynchronously migrate these rows to external Parquet files.

## 2. `SCAN` Operation

The `SCAN` operation is designed to transparently query both inline and external data, and to leverage indices when possible.

**Flow:**
1.  **API Call**: `table.scan(table_scan)` is called. `table_scan` contains optional projections, filters, and a limit.
2.  **Filter Analysis**: The `process_scan` logic first analyzes the provided filters.
    a. It calls `assign_index_filters` to check each filter expression against all available indexes on the table (by calling `index.supports_filter()`).
    b. **If a suitable index is found** (not fully implemented in the current codebase, but this is the design):
        i. The flow would switch to `process_index_scan`.
        ii. The index would be used to retrieve a small set of `row_id`s.
        iii. The system would then fetch only these specific rows from their locations (either inline or external).
    c. **If no suitable index is found** (the current primary path):
        i. The flow proceeds with `process_table_scan`.
3.  **Parallel Data Fetching**:
    a. **Inline Scan**: A `SELECT` query is sent to the Catalog to fetch rows from `indexlake_inline_row_{table_id}` that match the filter conditions.
    b. **External Scan**:
        i. A `SELECT` query is sent to the Catalog to get all `RowMetadataRecord`s for the table that are not marked as `deleted` and are not `inline`.
        ii. These locations are grouped by their physical file path.
        iii. For each Parquet file, the `read_parquet_files_by_locations` function is called. It uses the `RowSelection` API of Parquet to read only the specific row groups and offsets needed. The filter predicate is pushed down to the Parquet reader to further minimize I/O.
4.  **Merge and Return**: The streams of `RecordBatch`es from the inline scan and all the external file scans are merged using `futures::stream::select_all`. The resulting combined stream is returned to the user.

## 3. `DELETE` Operation

The `DELETE` operation uses a soft-delete mechanism, marking rows as deleted without immediately removing the data.

**Flow:**
1.  **API Call**: `table.delete(condition)` is called.
2.  **Transaction Start**: A new `TransactionHelper` is created.
3.  **Find Rows to Delete**:
    a. The system first finds all `row_id`s matching the `condition`. This process is similar to a `SCAN`, querying both inline rows and external data files to evaluate the condition and collect the `_indexlake_row_id`s of all matching rows.
    b. A special fast-path exists if the condition *only* involves the `_indexlake_row_id` column, allowing for a more direct update.
4.  **Mark as Deleted**: An `UPDATE` statement is executed against the `indexlake_row_metadata_{table_id}` table, setting `deleted = TRUE` for all the collected `row_id`s.
5.  **Remove from Inline**: For any of the deleted `row_id`s that were in inline storage, a `DELETE` statement is executed against the `indexlake_inline_row_{table_id}` table to physically remove them. Data in external Parquet files is not touched.
6.  **Transaction Commit**: The transaction is committed.

*(Note: A separate, offline process, often called compaction or vacuuming, would be needed to physically remove the "deleted" data from the Parquet files and reclaim space.)*

## 4. `UPDATE` Operation

The `UPDATE` operation is implemented as a "delete-then-insert" pattern, which is common in systems with immutable data files.

**Flow:**
1.  **API Call**: `table.update(set_map, condition)` is called.
2.  **Transaction Start**: A new `TransactionHelper` is created.
3.  **Find Rows to Update**: The system finds all rows that match the `condition`, just like in a `DELETE` operation. This involves scanning both inline and external data.
4.  **Apply Updates in Memory**: For each `RecordBatch` of rows that match the condition, the `update_record_batch` function is called. It creates a new `RecordBatch` in memory where the specified columns are replaced with the new values from the `set_map`.
5.  **Insert Updated Rows**: The newly created, updated `RecordBatch` is inserted back into the table as if it were a new `INSERT`. This means the updated rows are written to the **inline storage tier**.
6.  **Update Location Metadata**: The original rows (which are now outdated) have their location updated.
    a. For rows that were originally in Parquet files, their `location` in `indexlake_row_metadata_{table_id}` is changed to `"inline"`.
    b. For rows that were already inline, they are simply updated in place.
7.  **Transaction Commit**: The transaction is committed. The old versions of the rows are now effectively invisible, and the new versions are live in the inline table.
