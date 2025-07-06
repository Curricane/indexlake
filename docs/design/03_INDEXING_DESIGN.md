# IndexLake Design - 03: Pluggable Indexing Framework

The ability to support various types of secondary indices is a cornerstone of IndexLake's design. This is achieved through a flexible and extensible indexing framework built on Rust's trait system. The design follows the **Strategy** and **Abstract Factory** patterns.

## 1. Core Abstractions

The framework is defined by a few key traits that separate the "what" from the "how".

### 1.1. `Index` Trait (The Strategy and Factory)

This is the central trait that every index type must implement. It cleverly combines two roles: defining the query-time behavior (Strategy) and acting as a factory for creating index builders (Abstract Factory).

```rust
pub trait Index: Debug + Send + Sync {
    // Returns the unique string identifier for this index type (e.g., "rstar").
    fn kind(&self) -> &str;

    // Decodes a JSON string into the specific, type-erased parameters struct for this index.
    fn decode_params(&self, value: &str) -> ILResult<Arc<dyn IndexParams>>;

    // Validates if this index type can be built on the given table/column definition.
    fn supports(&self, index_def: &IndexDefination) -> ILResult<()>;

    // --- Factory Method ---
    // Creates a builder instance responsible for constructing this type of index.
    fn builder(&self, index_def: &IndexDefinationRef) -> ILResult<Box<dyn IndexBuilder>>;

    // --- Strategy Methods ---
    // Performs a similarity search (e.g., for vector indices).
    async fn search(...) -> ILResult<SearchIndexEntries>;

    // Checks if the index can accelerate a given filter expression.
    fn supports_filter(&self, index_def: &IndexDefination, filter: &Expr) -> ILResult<bool>;

    // Uses the index to efficiently filter rows based on an expression.
    async fn filter(...) -> ILResult<FilterIndexEntries>;
}
```

### 1.2. `IndexBuilder` Trait (The Product)

This trait defines the contract for an object that can incrementally build an index from data.

```rust
pub trait IndexBuilder: Debug + Send + Sync {
    // Consumes a batch of data and updates the internal state of the in-progress index.
    fn update(&mut self, batch: &RecordBatch) -> ILResult<()>;

    // Finalizes the index and writes the resulting artifact to the provided output file.
    async fn write(&mut self, output_file: OutputFile) -> ILResult<()>;
}
```

### 1.3. `IndexParams` Trait (Type-Erased Configuration)

This trait allows each index type to have its own unique set of configuration parameters.

```rust
pub trait IndexParams: Debug + Send + Sync {
    // Allows for downcasting the trait object back to its concrete type.
    fn as_any(&self) -> &dyn Any;

    // Encodes the parameters into a string (typically JSON) for storage in the catalog.
    fn encode(&self) -> ILResult<String>;
}
```

## 2. Key Data Structures

- **`IndexDefination`**: This struct is the runtime representation of a specific index instance on a table. It holds all the metadata needed to work with the index: its ID, name, kind, the table schema it's built on, key columns, and the specific, decoded `IndexParams` for that instance.

- **`FilterIndexEntries` / `SearchIndexEntries`**: These structs define the standard return types for `filter` and `search` operations, primarily containing the `row_id`s of the matching rows.

## 3. Workflow: Creating and Using an Index

### 3.1. Registration

1.  At startup, concrete index implementations (e.g., `Arc::new(RStarIndex)`) are registered with the `LakeClient`. The client stores them in a `HashMap<String, Arc<dyn Index>>`, mapping the `kind` string to the `Index` trait object.

### 3.2. Index Creation (`table.create_index()`)

1.  A user provides an `IndexCreation` struct, specifying the name, kind, columns, and parameters.
2.  The system retrieves the corresponding `Arc<dyn Index>` from the `LakeClient`'s registry based on the `kind`.
3.  It calls the `supports()` method on the trait object to validate that the index can be created on the specified columns.
4.  It calls the `encode()` method on the provided `IndexParams` to serialize them into a string.
5.  A transaction is started, and the index's metadata (including the serialized params) is saved to the `indexlake_index` table in the Catalog.

### 3.3. Index Building (During `DumpTask`)

1.  When data is being dumped from inline storage to a Parquet file, the `DumpTask` begins.
2.  For each index defined on the table, it retrieves the corresponding `Arc<dyn Index>`.
3.  It calls the `builder()` factory method on the `Index` trait object, which returns a `Box<dyn IndexBuilder>` (e.g., an `RStarIndexBuilder`).
4.  As data is read from inline storage and formed into `RecordBatch`es, each batch is passed to the `update()` method of the `IndexBuilder`.
5.  The `RStarIndexBuilder`, for example, will extract the geometry data, compute bounding boxes (AABBs), and accumulate them in memory.
6.  After all data has been processed, the `write()` method is called on the builder. The builder then writes its finalized index structure (e.g., a Parquet file of AABBs and row_ids) to the `Storage` layer.
7.  The path to this new index file is recorded in the `indexlake_index_file` table in the Catalog.

### 3.4. Index-Accelerated Query (`table.scan()`)

1.  The `process_scan` logic receives a set of filters.
2.  It iterates through all available indexes on the table.
3.  For each index, it calls `supports_filter()` on the corresponding `Index` trait object for each filter expression.
4.  If an index supports a filter (e.g., `RStarIndex` supports an `intersects` function), the system can choose to use that index.
5.  It would then call the `filter()` method on the `Index` trait object, providing the relevant index file. The `filter()` method would read the index file, perform its efficient filtering, and return a set of `row_id`s, which can be used to dramatically reduce the amount of data that needs to be read from the main data files.
