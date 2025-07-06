# IndexLake Design - 01: Layered Architecture

This document outlines the high-level, layered architecture of the IndexLake system. The design emphasizes separation of concerns, modularity, and clear interfaces between components, primarily achieved through Rust's `trait` system.

```
+-------------------------------------------------+
|                  Client API                     |
|              (LakeClient, Table)                |
+-------------------------------------------------+
|               Query & DML Layer                 |
| (scan, insert, update, delete, create_index)    |
+-------------------------------------------------+
|      Core Abstraction & Logic Layers            |
|                                                 |
| +---------------+  +-------------+  +---------+ |
| |    Catalog    |  |   Storage   |  |  Index  | |
| | (Transaction) |  | (OpenDAL)   |  | (Trait) | |
| +---------------+  +-------------+  +---------+ |
|         |                |              |       |
+---------|----------------|--------------|-------+
          |                |              |
+---------+----------------+--------------+-------+
|      Concrete Implementation Layers             |
|                                                 |
| +---------------+  +-------------+  +---------+ |
| | PostgresCatalog| | FsStorage   |  | RStarIndex|
| | SqliteCatalog |  | S3Storage   |  | HashIndex |
| +---------------+  +-------------+  +---------+ |
|                                                 |
+-------------------------------------------------+
```

## 1. Layers Description

### 1.1. Client API Layer

- **Components**: `LakeClient`, `Table`
- **Responsibility**: Provides the primary public-facing interface for users to interact with the system. It's the entry point for all operations.
- **Logic**:
    - `LakeClient` acts as the main session or connection object. It holds references to the configured `Catalog` and `Storage` backends and manages the registration of available `Index` types.
    - `Table` represents a handle to a specific table that has been loaded. It exposes DML (`insert`, `update`, `delete`), DDL-like (`create_index`), and query (`scan`, `search`) methods. It encapsulates the logic for a single table, coordinating the other layers to fulfill user requests.

### 1.2. Query & DML Layer

- **Components**: Methods within the `Table` struct and the `table` module (`process_insert`, `process_scan`, etc.).
- **Responsibility**: Translates user-facing API calls into a sequence of coordinated actions across the core abstraction layers.
- **Logic**:
    - An `insert` operation is translated into: start a transaction, generate new row IDs, write to the inline storage table in the catalog, and write to the row metadata table.
    - A `scan` operation involves: analyzing the query filters, determining if an index can be used, querying the catalog for relevant data file locations, and reading from both inline storage and external Parquet files.
    - This layer embodies the core business logic of the database engine.

### 1.3. Core Abstraction & Logic Layers

This is the heart of the system's design, defining the contracts that all components must adhere to.

- **`Catalog` Trait**:
    - **Contract**: Defines how to interact with the metadata store. Key methods include `transaction()` and `query()`.
    - **Key Feature**: The `transaction()` method returns a `Box<dyn Transaction>`, ensuring that all metadata modifications are atomic and consistent. The `Transaction` trait itself defines `commit`, `rollback`, and execution methods. This abstraction is crucial for system robustness.

- **`Storage` Enum (acting as an abstraction)**:
    - **Contract**: Defines how to interact with the physical file storage. It wraps a configured `opendal::Operator`.
    - **Logic**: Provides methods like `create_file`, `open_file`, `delete`, etc. It abstracts away the difference between storing data on a local filesystem (`FsStorage`) versus an object store like S3 (`S3Storage`).

- **`Index` Trait**:
    - **Contract**: Defines the behavior of a secondary index. Key methods are `builder()` (a factory for creating an `IndexBuilder`), `filter()` (for index-accelerated filtering), and `search()` (for similarity search).
    - **Logic**: This trait, along with `IndexBuilder`, forms the pluggable indexing framework. It allows the query layer to use any index type without knowing its internal implementation details.

### 1.4. Concrete Implementation Layers

This layer provides the concrete implementations for the abstractions defined above.

- **Catalog Implementations**:
    - `PostgresCatalog`: Implements the `Catalog` trait using a PostgreSQL backend. It uses a connection pool (`bb8`) for efficient, concurrent access.
    - `SqliteCatalog`: Implements the `Catalog` trait using a local SQLite database file.

- **Storage Implementations**:
    - `FsStorage`: Configures OpenDAL to use the local filesystem.
    - `S3Storage`: Configures OpenDAL to use an S3-compatible object store.

- **Index Implementations**:
    - `RStarIndex`: Implements the `Index` trait for spatial data, using R-Trees to accelerate `intersects` queries.
    - `HashIndex`, `BM25`, `HNSW`: Placeholders or partial implementations for other index types, demonstrating the extensibility of the framework.

## 2. Data Flow Example: `table.insert()`

1.  **Client API**: A user calls `table.insert(record_batch)`.
2.  **DML Layer**: The `Table::insert` method takes over.
3.  **Coordination**:
    a. It requests a new transaction from the `Catalog` layer by calling `catalog.transaction()`, receiving a `TransactionHelper`.
    b. It generates new, unique `_indexlake_row_id`s for the incoming data.
    c. It calls `tx_helper.insert_inline_rows()` to write the actual data to the `indexlake_inline_row_{table_id}` table within the catalog.
    d. It calls `tx_helper.insert_row_metadatas()` to record the location of these new rows as `inline`.
    e. It calls `tx_helper.commit()` to finalize the transaction.
4.  **Background Task**: After a successful commit, it checks if the inline row count exceeds the configured limit. If so, it spawns a `DumpTask` to asynchronously move older inline data to a Parquet file in the `Storage` layer.
